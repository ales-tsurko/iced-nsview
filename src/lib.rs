//! This crate allows you to use Iced as NSView. Thus it makes Iced embeddable into a macOS
//! application or AU/VST plugins, for example.
//!
//! You should implement your GUI using `Application` trait, then you can initialize `IcedView`
//! with it.

#![deny(
    missing_docs,
    nonstandard_style,
    rust_2018_idioms,
    trivial_casts,
    trivial_numeric_casts
)]
#![warn(
    deprecated_in_future,
    unused_import_braces,
    unused_labels,
    unused_lifetimes,
    unused_qualifications,
    unreachable_pub
)]

pub mod widget;

use std::ffi::c_void;
use std::marker::PhantomData;

use cocoa::appkit::{NSEvent, NSEventModifierFlags, NSEventType, NSView};
use cocoa::base::{id, nil, BOOL};
use cocoa::foundation::{NSPoint, NSRect, NSSize};

use core_graphics::base::CGFloat;
use core_graphics::geometry::{CGPoint, CGRect};

use iced_wgpu::{wgpu, Backend, Renderer, Settings};

pub use iced_wgpu::Viewport;

use iced_native::{program, window, Debug, Element as NativeElement, Event};

pub use iced_native::{
    futures, keyboard, mouse, Align, Background, Color, Command, Font, HorizontalAlignment, Length,
    Point, Rectangle, Size, Vector, VerticalAlignment,
};

use objc::declare::ClassDecl;
use objc::runtime::{Class, Sel, YES};
use objc::{class, msg_send, sel, sel_impl};

pub use objc::runtime::Object;

#[doc(no_inline)]
pub use widget::*;

/// A composition of widgets.
pub type Element<'a, M> = NativeElement<'a, M, Renderer>;

/// Iced view which is a subclass of `NSView`.
pub struct IcedView<A: 'static + Application> {
    object: *mut Object,
    _phantom_app: PhantomData<A>,
}

impl<A: 'static + Application> IcedView<A> {
    const EVENT_HANDLER_IVAR: &'static str = "_event_handler";

    /// Constructor.
    pub fn new(application: A, viewport: Viewport) -> Self {
        let object = unsafe { Self::init_nsview(viewport.physical_size()) };
        let event_handler = EventHandler::new(application, object, viewport);
        unsafe {
            (*object).set_ivar(
                Self::EVENT_HANDLER_IVAR,
                Box::into_raw(Box::new(event_handler)) as *mut c_void,
            );
        };

        Self {
            object,
            _phantom_app: PhantomData,
        }
    }

    unsafe fn init_nsview(size: Size<u32>) -> *mut Object {
        let class = Self::declare_class();
        let rect = NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(size.width.into(), size.height.into()),
        );
        let allocation: *const Object = msg_send![class, alloc];
        let object: *mut Object = msg_send![allocation, initWithFrame: rect];
        // NSViewLayerContentsRedrawDuringViewResize
        let () = msg_send![object, setLayerContentsRedrawPolicy: 2];

        object
    }

    unsafe fn declare_class() -> &'static Class {
        let superclass = class!(NSView);
        let mut decl =
            ClassDecl::new("IcedView", superclass).expect("Can't declare IcedView class.");
        decl.add_ivar::<*mut c_void>(Self::EVENT_HANDLER_IVAR);

        let accepts_first_responder: extern "C" fn(&Object, Sel) -> BOOL =
            Self::accepts_first_responder;
        decl.add_method(sel!(acceptsFirstResponder), accepts_first_responder);

        let update_tracking_areas: extern "C" fn(&Object, Sel) = Self::update_tracking_areas;
        decl.add_method(sel!(updateTrackingAreas), update_tracking_areas);

        let update_layer: extern "C" fn(&mut Object, Sel) = Self::update_layer;
        decl.add_method(sel!(updateLayer), update_layer);

        let resize: extern "C" fn(&mut Object, Sel) = Self::resize;
        decl.add_method(sel!(viewWillStartLiveResize), resize);
        decl.add_method(sel!(viewDidEndLiveResize), resize);

        let handle_event: extern "C" fn(&mut Object, Sel, *mut Object) = Self::handle_event;
        decl.add_method(sel!(mouseDown:), handle_event);
        decl.add_method(sel!(mouseUp:), handle_event);
        decl.add_method(sel!(mouseDragged:), handle_event);
        decl.add_method(sel!(mouseMoved:), handle_event);
        decl.add_method(sel!(mouseEntered:), handle_event);
        decl.add_method(sel!(mouseExited:), handle_event);
        decl.add_method(sel!(rightMouseDown:), handle_event);
        decl.add_method(sel!(rightMouseUp:), handle_event);
        decl.add_method(sel!(scrollWheel:), handle_event);
        decl.add_method(sel!(keyDown:), handle_event);
        decl.add_method(sel!(keyUp:), handle_event);
        decl.add_method(sel!(flagsChanged:), handle_event);

        decl.register()
    }

    extern "C" fn accepts_first_responder(_this: &Object, _cmd: Sel) -> BOOL {
        return YES;
    }

    extern "C" fn update_tracking_areas(this: &Object, _cmd: Sel) {
        // NSTrackingMouseEnteredAndExited | NSTrackingMouseMoved | NSTrackingCursorUpdate |
        // NSTrackingActiveInKeyWindow
        let options = 0x01 | 0x02 | 0x04 | 0x20;
        let class = class!(NSTrackingArea);
        unsafe {
            let bounds: NSRect = msg_send![this, bounds];
            let alloc: *mut Object = msg_send![class, alloc];
            let tracking_area: *mut Object =
                msg_send![alloc, initWithRect:bounds options:options owner:this userInfo:nil];
            let () = msg_send![this, addTrackingArea: tracking_area];
        }
    }

    extern "C" fn update_layer(this: &mut Object, cmd: Sel) {
        unsafe {
            let in_resize: BOOL = msg_send![this, inLiveResize];
            if in_resize != 0 {
                Self::resize(this, cmd);
            }

            let value = this.get_mut_ivar::<*mut c_void>(Self::EVENT_HANDLER_IVAR);
            let event_handler = *value as *mut EventHandler<A>;
            (*event_handler).redraw();
        }
    }

    extern "C" fn resize(this: &mut Object, _cmd: Sel) {
        unsafe {
            let value = this.get_mut_ivar::<*mut c_void>(Self::EVENT_HANDLER_IVAR);
            let event_handler = *value as *mut EventHandler<A>;
            let this_ptr: *mut Object = this;
            let bounds = NSView::bounds(this_ptr);
            let parent_window: *mut Object = msg_send![this, window];
            let scale_factor: CGFloat = msg_send![parent_window, backingScaleFactor];
            (*event_handler).resize(
                Size::new(bounds.size.width as u32, bounds.size.height as u32),
                scale_factor,
            );
        }
    }

    extern "C" fn handle_event(this: &mut Object, _cmd: Sel, event: *mut Object) {
        unsafe {
            let value = this.get_mut_ivar::<*mut c_void>(Self::EVENT_HANDLER_IVAR);
            let event_handler = *value as *mut EventHandler<A>;
            if let Some(event) = NSEventT(event).into() {
                (*event_handler).queue_event(event);
                let () = msg_send![this, setNeedsDisplay: YES];
            }
        };
    }

    /// Get a raw pointer to the Cocoa view.
    pub fn raw_object(&self) -> *mut Object {
        self.object
    }

    /// Make this view a subview of another view.
    pub unsafe fn make_subview_of(&self, view: *mut c_void) {
        NSView::addSubview_(view as id, self.object);
    }
}

impl<A: 'static + Application> Drop for IcedView<A> {
    fn drop(&mut self) {
        unsafe {
            let value = self
                .object
                .as_mut()
                .unwrap()
                .get_mut_ivar::<*mut c_void>(Self::EVENT_HANDLER_IVAR);
            let _ = Box::from_raw(*value as *mut EventHandler<A>);
            let () = msg_send![self.object, release];
        }
    }
}

struct EventHandler<A: 'static + Application> {
    state: program::State<Program<A>>,
    viewport: Viewport,
    surface: wgpu::Surface,
    device: wgpu::Device,
    queue: wgpu::Queue,
    format: wgpu::TextureFormat,
    swap_chain: wgpu::SwapChain,
    debug: Debug,
    renderer: Renderer,
}

impl<A: 'static + Application> EventHandler<A> {
    fn new(application: A, object: *mut Object, viewport: Viewport) -> Self {
        let surface = unsafe { Self::init_surface_layer(object, viewport.scale_factor()) };
        let (mut device, queue) = Self::init_device_and_queue(&surface);
        let format = wgpu::TextureFormat::Bgra8UnormSrgb;
        let swap_chain =
            Self::init_swap_chain(&viewport.physical_size(), &device, &surface, &format);
        let mut debug = Debug::new();
        let mut renderer = Renderer::new(Backend::new(&mut device, Settings::default()));
        let program = Program::new(application);
        let state: program::State<Program<A>> =
            program::State::new(program, viewport.logical_size(), &mut renderer, &mut debug);

        Self {
            state,
            viewport,
            surface,
            device,
            queue,
            format,
            swap_chain,
            debug,
            renderer,
        }
    }

    unsafe fn init_surface_layer(view: *mut Object, scale: f64) -> wgpu::Surface {
        let class = class!(CAMetalLayer);
        let layer: *mut Object = msg_send![class, new];
        let () = msg_send![view, setWantsLayer: YES];
        let parent: *mut Object = msg_send![view, layer];
        let () = msg_send![parent, addSublayer: layer];
        let bounds: CGRect = msg_send![view, bounds];
        let () = msg_send![layer, setBounds: bounds];
        let () = msg_send![layer, setContentsScale: scale];
        let () = msg_send![layer, setAnchorPoint: CGPoint::new(0.0, 0.0)];
        // kCALayerWidthSizable | kCALayerHeightSizable
        let autoresizing_mask = 1u64 << 1 | 1 << 4;
        let () = msg_send![layer, setAutoresizingMask: autoresizing_mask];
        let _: *mut c_void = msg_send![view, retain];

        wgpu::Surface::create_surface_from_core_animation_layer(layer as *mut c_void)
    }

    fn init_device_and_queue(surface: &wgpu::Surface) -> (wgpu::Device, wgpu::Queue) {
        futures::executor::block_on(async {
            let adapter = wgpu::Adapter::request(
                &wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::Default,
                    compatible_surface: Some(&surface),
                },
                wgpu::BackendBit::PRIMARY,
            )
            .await
            .expect("Request adapter");

            adapter
                .request_device(&wgpu::DeviceDescriptor {
                    extensions: wgpu::Extensions {
                        anisotropic_filtering: false,
                    },
                    limits: wgpu::Limits::default(),
                })
                .await
        })
    }

    fn init_swap_chain(
        size: &Size<u32>,
        device: &wgpu::Device,
        surface: &wgpu::Surface,
        format: &wgpu::TextureFormat,
    ) -> wgpu::SwapChain {
        device.create_swap_chain(
            &surface,
            &wgpu::SwapChainDescriptor {
                usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
                format: format.clone(),
                width: size.width,
                height: size.height,
                present_mode: wgpu::PresentMode::Mailbox,
            },
        )
    }

    fn resize(&mut self, new_size: Size<u32>, scale_factor: f64) {
        self.viewport = Viewport::with_physical_size(new_size, scale_factor);

        self.swap_chain = self.device.create_swap_chain(
            &self.surface,
            &wgpu::SwapChainDescriptor {
                usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
                format: self.format,
                width: new_size.width,
                height: new_size.height,
                present_mode: wgpu::PresentMode::Mailbox,
            },
        );

        self.on_window_event(window::Event::Resized {
            width: new_size.width,
            height: new_size.height,
        });
    }

    fn on_window_event(&mut self, event: window::Event) {
        self.queue_event(Event::Window(event));
    }

    fn queue_event(&mut self, event: Event) {
        self.state.queue_event(event);
    }

    fn redraw(&mut self) {
        self.update_state();

        if let Ok(frame) = self.swap_chain.get_next_texture() {
            self.debug.render_started();

            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

            self.render_pass(&frame, &mut encoder);

            let mouse_interaction = self.render_pass_iced(&frame, &mut encoder);

            self.queue.submit(&[encoder.finish()]);

            self.debug.render_finished();

            self.set_cursor_icon(mouse_interaction);
        }
    }

    fn update_state(&mut self) {
        self.state.update(
            None,
            self.viewport.logical_size(),
            &mut self.renderer,
            &mut self.debug,
        );
    }

    fn render_pass(&mut self, frame: &wgpu::SwapChainOutput, encoder: &mut wgpu::CommandEncoder) {
        let background_color = self.state.program().application.background_color();

        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[wgpu::RenderPassColorAttachmentDescriptor {
                attachment: &frame.view,
                resolve_target: None,
                load_op: wgpu::LoadOp::Clear,
                store_op: wgpu::StoreOp::Store,
                clear_color: wgpu::Color {
                    r: background_color.r as f64,
                    g: background_color.g as f64,
                    b: background_color.b as f64,
                    a: background_color.a as f64,
                },
            }],
            depth_stencil_attachment: None,
        });
    }

    fn render_pass_iced(
        &mut self,
        frame: &wgpu::SwapChainOutput,
        encoder: &mut wgpu::CommandEncoder,
    ) -> mouse::Interaction {
        self.renderer.backend_mut().draw(
            &mut self.device,
            encoder,
            &frame.view,
            &self.viewport,
            self.state.primitive(),
            &self.debug.overlay(),
        )
    }

    fn set_cursor_icon(&self, cursor: mouse::Interaction) {
        unsafe {
            let class = class!(NSCursor);
            let cocoa_cursor: *mut Object = match cursor {
                mouse::Interaction::Idle => msg_send![class, arrowCursor],
                mouse::Interaction::Pointer => msg_send![class, pointingHandCursor],
                mouse::Interaction::Grab => msg_send![class, openHandCursor],
                mouse::Interaction::Text => msg_send![class, IBeamCursor],
                mouse::Interaction::Crosshair => msg_send![class, crosshairCursor],
                mouse::Interaction::Working => msg_send![class, arrowCursor],
                mouse::Interaction::Grabbing => msg_send![class, closedHandCursor],
                mouse::Interaction::ResizingHorizontally => msg_send![class, resizeLeftRightCursor],
                mouse::Interaction::ResizingVertically => msg_send![class, resizeUpDownCursor],
            };

            let () = msg_send![cocoa_cursor, set];
        }
    }
}

/// Implement this trait for your application then pass it into `IcedView::new`.
pub trait Application {
    /// The message your application will produce.
    type Message: Clone + std::fmt::Debug + Send;

    /// Message processing function.
    fn update(&mut self, message: Self::Message) -> Command<Self::Message>;

    /// Application interface.
    fn view(&mut self) -> Element<'_, Self::Message>;

    /// Returns the background color of the [`Application`].
    ///
    /// By default, it returns `Color::WHITE`.
    fn background_color(&self) -> Color {
        Color::WHITE
    }
}

struct Program<A: Application> {
    application: A,
}

impl<A: Application> Program<A> {
    fn new(application: A) -> Self {
        Self { application }
    }
}

impl<A: Application> program::Program for Program<A> {
    type Renderer = Renderer;
    type Message = A::Message;

    fn update(&mut self, message: Self::Message) -> Command<Self::Message> {
        self.application.update(message)
    }

    /// Application interface.
    fn view(&mut self) -> NativeElement<'_, Self::Message, Self::Renderer> {
        self.application.view()
    }
}

struct NSEventT<T: NSEvent + Copy>(T);

impl<T: NSEvent + Copy> From<NSEventT<T>> for Option<Event> {
    fn from(event: NSEventT<T>) -> Self {
        unsafe {
            let mouse_location: NSPoint = NSEvent::locationInWindow(event.0);
            let moved = Event::Mouse(mouse::Event::CursorMoved {
                x: mouse_location.x as f32,
                y: mouse_location.y as f32,
            });
            let button_num = NSEvent::buttonNumber(event.0);

            match NSEvent::eventType(event.0) {
                NSEventType::NSLeftMouseDown => Some(Event::Mouse(mouse::Event::ButtonPressed(
                    mouse::Button::Left,
                ))),
                NSEventType::NSLeftMouseUp => Some(Event::Mouse(mouse::Event::ButtonReleased(
                    mouse::Button::Left,
                ))),
                NSEventType::NSRightMouseDown => Some(Event::Mouse(mouse::Event::ButtonPressed(
                    mouse::Button::Right,
                ))),
                NSEventType::NSRightMouseUp => Some(Event::Mouse(mouse::Event::ButtonReleased(
                    mouse::Button::Right,
                ))),
                NSEventType::NSMouseMoved => Some(moved),
                NSEventType::NSLeftMouseDragged => Some(moved),
                NSEventType::NSMouseEntered => Some(Event::Mouse(mouse::Event::CursorEntered)),
                NSEventType::NSMouseExited => Some(Event::Mouse(mouse::Event::CursorLeft)),
                NSEventType::NSKeyDown => from_key_down(event.0),
                NSEventType::NSKeyUp => from_key_up(event.0),
                NSEventType::NSScrollWheel => Some(Event::Mouse(mouse::Event::WheelScrolled {
                    delta: mouse::ScrollDelta::Pixels {
                        x: NSEvent::scrollingDeltaX(event.0) as f32,
                        y: NSEvent::scrollingDeltaY(event.0) as f32,
                    },
                })),
                NSEventType::NSOtherMouseDown => Some(Event::Mouse(mouse::Event::ButtonPressed(
                    ButtonNumber(button_num).into(),
                ))),
                NSEventType::NSOtherMouseUp => Some(Event::Mouse(mouse::Event::ButtonReleased(
                    ButtonNumber(button_num).into(),
                ))),
                _ => None,
            }
        }
    }
}

unsafe fn from_key_down<T: NSEvent + Copy>(event: T) -> Option<Event> {
    let modifiers = keyboard::ModifiersState::from(ModifierFlags(NSEvent::modifierFlags(event)));
    let kc = Option::<keyboard::KeyCode>::from(NSKeyCode(NSEvent::keyCode(event)));
    // let chars = NSEvent::characters(event);

    kc.map(|kc| {
        Event::Keyboard(keyboard::Event::KeyPressed {
            key_code: kc,
            modifiers,
        })
    })
}

unsafe fn from_key_up<T: NSEvent + Copy>(event: T) -> Option<Event> {
    let modifiers = keyboard::ModifiersState::from(ModifierFlags(NSEvent::modifierFlags(event)));
    let kc = Option::<keyboard::KeyCode>::from(NSKeyCode(NSEvent::keyCode(event)));
    // let chars = NSEvent::characters(event);

    kc.map(|kc| {
        Event::Keyboard(keyboard::Event::KeyReleased {
            key_code: kc,
            modifiers,
        })
    })
}

struct NSKeyCode(u16);

impl From<NSKeyCode> for Option<keyboard::KeyCode> {
    fn from(key_code: NSKeyCode) -> Self {
        match key_code.0 {
            29 => Some(keyboard::KeyCode::Key0),
            18 => Some(keyboard::KeyCode::Key1),
            19 => Some(keyboard::KeyCode::Key2),
            20 => Some(keyboard::KeyCode::Key3),
            21 => Some(keyboard::KeyCode::Key4),
            23 => Some(keyboard::KeyCode::Key5),
            22 => Some(keyboard::KeyCode::Key6),
            26 => Some(keyboard::KeyCode::Key7),
            28 => Some(keyboard::KeyCode::Key8),
            25 => Some(keyboard::KeyCode::Key9),
            0 => Some(keyboard::KeyCode::A),
            11 => Some(keyboard::KeyCode::B),
            8 => Some(keyboard::KeyCode::C),
            2 => Some(keyboard::KeyCode::D),
            14 => Some(keyboard::KeyCode::E),
            3 => Some(keyboard::KeyCode::F),
            5 => Some(keyboard::KeyCode::G),
            4 => Some(keyboard::KeyCode::H),
            34 => Some(keyboard::KeyCode::I),
            38 => Some(keyboard::KeyCode::J),
            40 => Some(keyboard::KeyCode::K),
            37 => Some(keyboard::KeyCode::L),
            46 => Some(keyboard::KeyCode::M),
            45 => Some(keyboard::KeyCode::N),
            31 => Some(keyboard::KeyCode::O),
            35 => Some(keyboard::KeyCode::P),
            12 => Some(keyboard::KeyCode::Q),
            15 => Some(keyboard::KeyCode::R),
            1 => Some(keyboard::KeyCode::S),
            17 => Some(keyboard::KeyCode::T),
            32 => Some(keyboard::KeyCode::U),
            9 => Some(keyboard::KeyCode::V),
            13 => Some(keyboard::KeyCode::W),
            7 => Some(keyboard::KeyCode::X),
            16 => Some(keyboard::KeyCode::Y),
            6 => Some(keyboard::KeyCode::Z),
            // 10 => Some(::SectionSign),
            50 => Some(keyboard::KeyCode::Grave),
            27 => Some(keyboard::KeyCode::Minus),
            24 => Some(keyboard::KeyCode::Equals),
            33 => Some(keyboard::KeyCode::LBracket),
            30 => Some(keyboard::KeyCode::RBracket),
            41 => Some(keyboard::KeyCode::Semicolon),
            39 => Some(keyboard::KeyCode::Apostrophe),
            43 => Some(keyboard::KeyCode::Comma),
            47 => Some(keyboard::KeyCode::Period),
            44 => Some(keyboard::KeyCode::Slash),
            42 => Some(keyboard::KeyCode::Backslash),
            82 => Some(keyboard::KeyCode::Numpad0),
            83 => Some(keyboard::KeyCode::Numpad1),
            84 => Some(keyboard::KeyCode::Numpad2),
            85 => Some(keyboard::KeyCode::Numpad3),
            86 => Some(keyboard::KeyCode::Numpad4),
            87 => Some(keyboard::KeyCode::Numpad5),
            88 => Some(keyboard::KeyCode::Numpad6),
            89 => Some(keyboard::KeyCode::Numpad7),
            91 => Some(keyboard::KeyCode::Numpad8),
            92 => Some(keyboard::KeyCode::Numpad9),
            65 => Some(keyboard::KeyCode::NumpadComma),
            67 => Some(keyboard::KeyCode::Multiply),
            69 => Some(keyboard::KeyCode::Add),
            75 => Some(keyboard::KeyCode::Divide),
            78 => Some(keyboard::KeyCode::Minus),
            81 => Some(keyboard::KeyCode::NumpadEquals),
            // 71 => Some(::KeypadClear),
            76 => Some(keyboard::KeyCode::NumpadEnter),
            49 => Some(keyboard::KeyCode::Space),
            36 => Some(keyboard::KeyCode::Enter),
            48 => Some(keyboard::KeyCode::Tab),
            51 => Some(keyboard::KeyCode::Backspace),
            117 => Some(keyboard::KeyCode::Delete),
            // 52 => Some(::Linefeed),
            53 => Some(keyboard::KeyCode::Escape),
            55 => Some(keyboard::KeyCode::LWin),
            56 => Some(keyboard::KeyCode::LShift),
            57 => Some(keyboard::KeyCode::Capital),
            58 => Some(keyboard::KeyCode::LAlt),
            59 => Some(keyboard::KeyCode::LControl),
            60 => Some(keyboard::KeyCode::RShift),
            61 => Some(keyboard::KeyCode::RAlt),
            62 => Some(keyboard::KeyCode::RControl),
            // 63 => Some(::Function),
            122 => Some(keyboard::KeyCode::F1),
            120 => Some(keyboard::KeyCode::F2),
            99 => Some(keyboard::KeyCode::F3),
            118 => Some(keyboard::KeyCode::F4),
            96 => Some(keyboard::KeyCode::F5),
            97 => Some(keyboard::KeyCode::F6),
            98 => Some(keyboard::KeyCode::F7),
            100 => Some(keyboard::KeyCode::F8),
            101 => Some(keyboard::KeyCode::F9),
            109 => Some(keyboard::KeyCode::F10),
            103 => Some(keyboard::KeyCode::F11),
            111 => Some(keyboard::KeyCode::F12),
            105 => Some(keyboard::KeyCode::F13),
            107 => Some(keyboard::KeyCode::F14),
            113 => Some(keyboard::KeyCode::F15),
            106 => Some(keyboard::KeyCode::F16),
            64 => Some(keyboard::KeyCode::F17),
            79 => Some(keyboard::KeyCode::F18),
            80 => Some(keyboard::KeyCode::F19),
            90 => Some(keyboard::KeyCode::F20),
            72 => Some(keyboard::KeyCode::VolumeUp),
            73 => Some(keyboard::KeyCode::VolumeDown),
            74 => Some(keyboard::KeyCode::Mute),
            114 => Some(keyboard::KeyCode::Insert),
            115 => Some(keyboard::KeyCode::Home),
            119 => Some(keyboard::KeyCode::End),
            116 => Some(keyboard::KeyCode::PageUp),
            121 => Some(keyboard::KeyCode::PageDown),
            123 => Some(keyboard::KeyCode::Left),
            124 => Some(keyboard::KeyCode::Right),
            125 => Some(keyboard::KeyCode::Down),
            126 => Some(keyboard::KeyCode::Up),
            _ => None,
        }
    }
}

struct ModifierFlags(NSEventModifierFlags);

impl From<ModifierFlags> for keyboard::ModifiersState {
    fn from(flags: ModifierFlags) -> Self {
        Self {
            shift: flags.0.contains(NSEventModifierFlags::NSShiftKeyMask),
            control: flags.0.contains(NSEventModifierFlags::NSControlKeyMask),
            alt: flags.0.contains(NSEventModifierFlags::NSAlternateKeyMask),
            logo: flags.0.contains(NSEventModifierFlags::NSCommandKeyMask),
        }
    }
}

struct ButtonNumber(i64);

impl From<ButtonNumber> for mouse::Button {
    fn from(number: ButtonNumber) -> Self {
        match number.0 {
            2 => mouse::Button::Middle,
            value => mouse::Button::Other(value as u8),
        }
    }
}

/// This function returns scale factor of the passed view.
///
/// It returns `None` if the view has no window.
pub unsafe fn get_nsview_scale_factor(view: *mut c_void) -> Option<f64> {
    let window: id = msg_send![view as *mut Object, window];
    if window.is_null() {
        None
    } else {
        let scale_factor: CGFloat = msg_send![window, backingScaleFactor];
        Some(scale_factor)
    }
}
