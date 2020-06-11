use iced_nsview::{
    slider, Align, Color, Column, Command, Element, IcedView, Length, Program, Row, Size, Slider,
    Text, Viewport,
};
use iced_wgpu::Renderer;

fn main() {
    let controls = Controls::new();
    let viewport = Viewport::with_physical_size(Size::new(800, 600), 1.0);
    let view = IcedView::new(controls, viewport);
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

impl Program for Controls {
    type Renderer = Renderer;
    type Message = Message;

    fn update(&mut self, message: Message) -> Command<Message> {
        let Message::AmpChanged(amp) = message;
        self.amp = amp;

        Command::none()
    }

    fn view(&mut self) -> Element<Message, Renderer> {
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
