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

use cocoa::appkit::NSView;
use cocoa::base::id;
use cocoa::foundation::{NSPoint, NSRect, NSSize};

use core_graphics::base::CGFloat;
use core_graphics::geometry::CGRect;

use iced_wgpu::{wgpu, Backend, Renderer, Settings};

pub use iced_wgpu::Viewport;

pub use iced_native::{Element as NativeElement, *};

use objc::declare::ClassDecl;
use objc::runtime::{Class, YES};
use objc::{class, msg_send, sel, sel_impl};

pub use objc::runtime::Object;

/// A composition of widgets.
pub type Element<'a, M> = NativeElement<'a, M, Renderer>;

/// Iced view which is a subclass of NSView.
pub struct IcedView<A: 'static + Application> {
    object: *mut Object,
    state: program::State<Program<A>>,
}

impl<A: 'static + Application> IcedView<A> {
    /// Constructor.
    pub fn new(application: A, viewport: Viewport) -> Self {
        let object = unsafe { Self::init_nsview(viewport.physical_size()) };
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

        Self { object, state }
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

    fn declare_class() -> &'static Class {
        let superclass = class!(NSView);
        let decl = ClassDecl::new("IcedView", superclass).expect("Can't declare IcedView");
        let draw_rect: extern "C" fn(&Object, *mut Object) = Self::draw_rect;
        // decl.add_method(sel!(drawRect), draw_rect);
        decl.register()
    }

    extern "C" fn draw_rect(_this: &Object, dirty_rect: *mut Object) {}

    unsafe fn init_surface_layer(view: *mut Object, scale: f64) -> wgpu::Surface {
        let class = class!(CAMetalLayer);
        let layer: *mut Object = msg_send![class, new];
        let () = msg_send![view, setLayer: layer];
        let () = msg_send![view, setWantsLayer: YES];
        let bounds: CGRect = msg_send![view, bounds];
        let () = msg_send![layer, setBounds: bounds];
        let () = msg_send![layer, setContentsScale: scale];
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

    /// Get a raw pointer to the Cocoa view.
    pub fn raw_object(&self) -> *mut Object {
        self.object
    }

    /// Make this view a subview of another view.
    pub unsafe fn make_subview_of(&self, view: *mut c_void) {
        NSView::addSubview_(view as id, self.object);
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
