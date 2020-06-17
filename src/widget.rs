//! Display information and interactive controls in your application.
//!
//! # Re-exports
//! For convenience, the contents of this module are available at the root
//! module. Therefore, you can directly type:
//!
//! ```
//! use iced_nsview::{button, Button};
//! ```
//!
//! # Stateful widgets
//! Some widgets need to keep track of __local state__.
//!
//! These widgets have their own module with a `State` type. For instance, a
//! [`TextInput`] has some [`text_input::State`].
//!
//! [`TextInput`]: text_input/struct.TextInput.html
//! [`text_input::State`]: text_input/struct.State.html

pub mod image {
    //! Display images in your user interface.
    pub use iced_native::image::{Handle, Image};
}

pub mod svg {
    //! Display vector graphics in your user interface.
    pub use iced_native::svg::{Handle, Svg};
}

pub use iced_wgpu::{
    button, checkbox, container, pane_grid, progress_bar, radio, scrollable, slider, text_input,
    widget::canvas, Column, Row, Space, Text,
};

#[doc(no_inline)]
pub use {
    button::Button, canvas::Canvas, checkbox::Checkbox, container::Container, image::Image,
    pane_grid::PaneGrid, progress_bar::ProgressBar, radio::Radio, scrollable::Scrollable,
    slider::Slider, svg::Svg, text_input::TextInput,
};
