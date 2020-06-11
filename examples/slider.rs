use cocoa::appkit::NSView;

use iced_nsview::{
    slider, Align, Application, Color, Column, Command, Element, IcedView, Length, Row, Size,
    Slider, Text, Viewport,
};

use winit::dpi;
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::platform::macos::WindowExtMacOS;
use winit::window::WindowBuilder;

fn main() {
    let size = Size::new(800, 600);
    let win_size = dpi::Size::Physical((size.width, size.height).into());
    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_inner_size(win_size)
        .build(&event_loop)
        .unwrap();

    let controls = Controls::new();
    let viewport = Viewport::with_physical_size(size, window.scale_factor());
    let view = IcedView::new(controls, viewport);
    unsafe { view.make_subview_of(window.ns_view()) };

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Poll;

        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => *control_flow = ControlFlow::Exit,
            Event::MainEventsCleared => {
                window.request_redraw();
            }
            Event::RedrawRequested(_) => {}
            _ => (),
        }
    });
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
                    .push(Text::new("Amp").color(Color::WHITE))
                    .push(slider)
                    .push(Text::new(format!("{:.2}", self.amp)).color(Color::WHITE)),
            )
            .into()
    }
}
