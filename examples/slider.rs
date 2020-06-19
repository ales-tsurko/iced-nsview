use cocoa::appkit::{
    NSApp, NSApplication, NSApplicationActivationPolicyRegular, NSBackingStoreBuffered, NSWindow,
    NSWindowStyleMask,
};
use cocoa::base::{id, nil, NO};
use cocoa::foundation::{NSAutoreleasePool, NSPoint, NSRect, NSSize};

use iced_nsview::{
    slider, Align, Application, Column, Command, Element, IcedView, Length, Row, Size, Slider,
    Text, Viewport,
};

fn main() {
    let size = Size::new(800, 600);
    let app = unsafe { init_app() };
    let window = unsafe { init_window(&size) };
    let scale_factor = unsafe { window.backingScaleFactor() };

    let controls = Controls::new();
    let viewport = Viewport::with_physical_size(size, scale_factor);
    let view = IcedView::new(controls, viewport);

    unsafe {
        NSWindow::setContentView_(window, view.raw_object());
        app.run();
    }
}

unsafe fn init_app() -> id {
    let _pool = NSAutoreleasePool::new(nil);
    let app = NSApp();
    NSApplication::setActivationPolicy_(app, NSApplicationActivationPolicyRegular);

    app
}

unsafe fn init_window(size: &Size<u32>) -> id {
    let window = NSWindow::alloc(nil)
        .initWithContentRect_styleMask_backing_defer_(
            NSRect::new(
                NSPoint::new(0.0, 0.0),
                NSSize::new(size.width as f64, size.height as f64),
            ),
            NSWindowStyleMask::NSTitledWindowMask
                | NSWindowStyleMask::NSClosableWindowMask
                | NSWindowStyleMask::NSResizableWindowMask,
            NSBackingStoreBuffered,
            NO,
        )
        .autorelease();
    window.makeKeyAndOrderFront_(nil);
    window
}

struct Controls {
    amp: f32,
    slider: slider::State,
}

#[derive(Debug, Clone)]
enum Message {
    AmpChanged(f32),
}

impl Controls {
    fn new() -> Controls {
        Controls {
            amp: 0.0,
            slider: Default::default(),
        }
    }
}

impl Application for Controls {
    type Message = Message;

    fn update(&mut self, message: Message) -> Command<Message> {
        let Message::AmpChanged(amp) = message;
        self.amp = amp;

        Command::none()
    }

    fn view(&mut self) -> Element<Message> {
        let slider = Row::new()
            .width(Length::Units(500))
            .spacing(20)
            .push(Slider::new(
                &mut self.slider,
                0.0..=1.0,
                self.amp,
                move |r| Message::AmpChanged(r),
            ));

        Row::new()
            .width(Length::Fill)
            .height(Length::Fill)
            .align_items(Align::Center)
            .push(
                Column::new()
                    .width(Length::Fill)
                    .align_items(Align::Center)
                    .padding(10)
                    .spacing(10)
                    .push(Text::new("Amp"))
                    .push(slider)
                    .push(Text::new(format!("{:.2}", self.amp))),
            )
            .into()
    }
}
