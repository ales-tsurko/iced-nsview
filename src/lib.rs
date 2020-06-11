//! This crate allows you to use Iced as NSView. Thus it makes Iced embeddable into macOS
//! application or AU/VST plugins, for example.
//!
//! You should implement your GUI using `program::Program`, then you can init `IcedView` from it.

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
use cocoa::quartzcore::CALayer;

use core_graphics::base::CGFloat;
use core_graphics::geometry::CGRect;

use iced_wgpu::wgpu;

pub use iced_native::*;
pub use iced_wgpu::{Renderer, Viewport};

use objc::declare::ClassDecl;
use objc::runtime::{Class, YES};
use objc::{class, msg_send, sel, sel_impl};

pub use objc::runtime::Object;

/// Iced view subclassed from NSView.
pub struct IcedView<P: Program> {
    object: *mut Object,
    program: P,
}

impl<P: Program> IcedView<P> {
    /// Constructor.
    pub fn new(program: P, viewport: Viewport) -> Self {
        let object = unsafe { IcedView::<P>::init_nsview(viewport.physical_size()) };
        let surface = unsafe { IcedView::<P>::init_surface_layer(object, viewport.scale_factor()) };

        Self { object, program }
    }

    unsafe fn init_nsview(size: Size<u32>) -> *mut Object {
        let class = IcedView::<P>::declare_class();
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
        // TODO methods declaration goes here
        decl.register()
    }

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

    /// Get a raw pointer to the Cocoa view.
    pub fn raw_object(&self) -> *mut Object {
        self.object
    }

    /// Make this view a subview of another view.
    pub unsafe fn make_subview_of(&self, view: *mut c_void) {
        NSView::addSubview_(view as id, self.object);
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
