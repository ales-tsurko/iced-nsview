# iced-nsview

This crate allows you to use Iced as NSView. Thus it makes Iced embeddable into
a macOS application or AU/VST plugins, for example.




## Usage

You should implement your GUI using `Application` trait, then you can initialize
`IcedView` with it.
