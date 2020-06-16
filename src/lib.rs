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

use std::ffi::c_void;
use std::marker::PhantomData;

use cocoa::appkit::{NSEvent, NSEventType, NSView};
use cocoa::base::{id, nil, BOOL};
use cocoa::foundation::{NSPoint, NSRect, NSSize};

use core_graphics::base::CGFloat;
use core_graphics::geometry::{CGPoint, CGRect};

use iced_wgpu::{wgpu, Backend, Renderer, Settings};

pub use iced_wgpu::Viewport;

pub use iced_native::{Element as NativeElement, *};

use objc::declare::ClassDecl;
use objc::runtime::{Class, Sel, YES};
use objc::{class, msg_send, sel, sel_impl};

pub use objc::runtime::Object;

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

    extern "C" fn update_layer(this: &mut Object, _cmd: Sel) {
        unsafe {
            let value = this.get_mut_ivar::<*mut c_void>(Self::EVENT_HANDLER_IVAR);
            let event_handler = *value as *mut EventHandler<A>;
            (*event_handler).redraw();
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

    fn queue_event(&mut self, event: Event) {
        self.state.queue_event(event);
    }

    fn redraw(&mut self) {
        self.update_state();

        if let Ok(frame) = self.swap_chain.get_next_texture() {
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

            self.render_pass(&frame, &mut encoder);

            let mouse_interaction = self.render_pass_iced(&frame, &mut encoder);

            self.queue.submit(&[encoder.finish()]);

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
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[wgpu::RenderPassColorAttachmentDescriptor {
                attachment: &frame.view,
                resolve_target: None,
                load_op: wgpu::LoadOp::Clear,
                store_op: wgpu::StoreOp::Store,
                clear_color: wgpu::Color {
                    r: 1.0,
                    g: 1.0,
                    b: 1.0,
                    a: 1.0,
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
                // NSEventType::NSKeyDown => ,
                // NSEventType::NSKeyUp => ,
                // NSEventType::NSFlagsChanged => ,
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
