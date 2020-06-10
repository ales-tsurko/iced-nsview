//! This crate allows you to use Iced as NSView. Thus it makes Iced embeddable into macOS
//! application or AU/VST plugins, for example.
//!
//! You should implement your GUI using `program::Program`, then you can init `IcedView` from it.

#![deny(
    nonstandard_style,
    rust_2018_idioms,
    trivial_casts,
    trivial_numeric_casts
)]
#![warn(
    deprecated_in_future,
    missing_docs,
    unused_import_braces,
    unused_labels,
    unused_lifetimes,
    unused_qualifications,
    unreachable_pub
)]

use std::ffi::c_void;

use objc::class;
use objc::declare::ClassDecl;

pub use iced_native::*;

/// Iced view subclassed from NSView.
pub struct IcedView<P: program::Program> {
    // raw_ptr: *mut c_void,
    decl: ClassDecl,
    program: P,
}

impl<P: program::Program> IcedView<P> {
    /// Constructor.
    pub fn new(program: P) -> Self {
        let superclass = class!(NSView);
        let decl = ClassDecl::new("IcedView", superclass).expect("Can't allocate IcedView");

        Self { decl, program }
    }

    /// Get a raw pointer to the Cocoa view.
    pub fn raw_ptr(&self) -> *mut c_void {
        todo!()
    }

    /// Make this view a subview of another view.
    pub fn make_subview_of(&self, view: *mut c_void) {
        todo!()
    }
}
