use iced::mouse;
use iced::widget::canvas::{self, Action, Fill, Frame, Path, Stroke};
use iced::widget::{container, row, space};
use iced::{Color, Length, Point, Rectangle, Size, Theme};


use crate::app::ui::{Element, Message, UiState};
use crate::playback::PlaybackStatus;

pub fn playback_controls<'a>(state: &'a UiState) -> Element<'a> {
    let is_playing = matches!(state.playback_status, Some(PlaybackStatus::Playing { .. }));

    let controller = AudioController {
        is_playing,
        volume: state.volume,
    };

    let canvas_widget = canvas::Canvas::new(controller)
        .width(Length::Fixed(180.0))
        .height(Length::Fixed(80.0));

    let content = row![
        space().width(Length::Fill),
        canvas_widget,
        space().width(Length::Fill),
    ];

    container(content).into()
}

pub struct AudioController {
    pub is_playing: bool,
    pub volume: f32, // 0.0 .. 1.0
}

#[derive(Default)]
pub struct CanvasState {
    is_dragging_slider: bool,
}

impl canvas::Program<Message, iced::Theme, iced_wgpu::Renderer> for AudioController {
    type State = CanvasState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<Action<Message>> {
        let center_x = bounds.width / 2.0;
        let button_y = bounds.height * 0.4;
        let button_radius = 25.0;

        let slider_y = bounds.height * 0.8;
        let slider_width = bounds.width * 0.7;
        let slider_left = center_x - (slider_width / 2.0);

        match event {
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(cursor_position) = cursor.position_in(bounds) {
                    // 1. Play / Pause click
                    let dist_to_button = cursor_position.distance(Point::new(center_x, button_y));
                    if dist_to_button <= button_radius {
                        return Some(Action::publish(Message::TogglePlayback).and_capture());
                    }

                    // 2. Volume slider click
                    let slider_rect = Rectangle {
                        x: slider_left,
                        y: slider_y - 10.0,
                        width: slider_width,
                        height: 20.0,
                    };
                    if slider_rect.contains(cursor_position) {
                        state.is_dragging_slider = true;
                        let new_vol =
                            ((cursor_position.x - slider_left) / slider_width).clamp(0.0, 1.0);
                        return Some(
                            Action::publish(Message::VolumeChanged(new_vol)).and_capture(),
                        );
                    }
                }
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if state.is_dragging_slider {
                    if let Some(cursor_position) = cursor.position_in(bounds) {
                        let new_vol =
                            ((cursor_position.x - slider_left) / slider_width).clamp(0.0, 1.0);
                        return Some(
                            Action::publish(Message::VolumeChanged(new_vol)).and_capture(),
                        );
                    }
                }
            }
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
                if state.is_dragging_slider =>
            {
                state.is_dragging_slider = false;
                return Some(Action::capture());
            }
            _ => {}
        }

        None
    }

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &iced_wgpu::Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<iced_wgpu::Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());

        let center_x = bounds.width / 2.0;
        let button_y = bounds.height * 0.4;
        let button_radius = 25.0;

        let slider_y = bounds.height * 0.8;
        let slider_width = bounds.width * 0.7;
        let slider_left = center_x - (slider_width / 2.0);
        let slider_right = center_x + (slider_width / 2.0);

        let accent_color = Color::from_rgb8(100, 150, 240);
        let muted_color = Color::from_rgb8(70, 70, 80);

        // --- 1. Curved Frame ---
        let curved_frame = Path::new(|b| {
            b.move_to(Point::new(slider_left, slider_y));
            b.bezier_curve_to(
                Point::new(center_x - button_radius * 2.5, slider_y),
                Point::new(center_x - button_radius * 1.5, button_y - button_radius),
                Point::new(center_x, button_y - button_radius - 10.0),
            );
            b.bezier_curve_to(
                Point::new(center_x + button_radius * 1.5, button_y - button_radius),
                Point::new(center_x + button_radius * 2.5, slider_y),
                Point::new(slider_right, slider_y),
            );
        });
        frame.stroke(
            &curved_frame,
            Stroke::default().with_color(muted_color).with_width(2.0),
        );

        // --- 2. Volume Slider ---
        let slider_track = Path::line(
            Point::new(slider_left, slider_y),
            Point::new(slider_right, slider_y),
        );
        frame.stroke(
            &slider_track,
            Stroke::default().with_color(muted_color).with_width(4.0),
        );

        let handle_x = slider_left + (slider_width * self.volume);
        let slider_handle = Path::circle(Point::new(handle_x, slider_y), 6.0);
        frame.fill(&slider_handle, Fill::from(accent_color));

        // --- 3. Play / Pause Button ---
        if self.is_playing {
            let bar_width = 6.0;
            let bar_height = 20.0;
            let spacing = 8.0;

            let left_bar = Path::rectangle(
                Point::new(
                    center_x - spacing / 2.0 - bar_width,
                    button_y - bar_height / 2.0,
                ),
                Size::new(bar_width, bar_height),
            );
            let right_bar = Path::rectangle(
                Point::new(center_x + spacing / 2.0, button_y - bar_height / 2.0),
                Size::new(bar_width, bar_height),
            );
            frame.fill(&left_bar, Fill::from(accent_color));
            frame.fill(&right_bar, Fill::from(accent_color));
        } else {
            let play_triangle = Path::new(|b| {
                let size = 10.0;
                b.move_to(Point::new(center_x - size, button_y - size * 1.2));
                b.line_to(Point::new(center_x + size * 1.4, button_y));
                b.line_to(Point::new(center_x - size, button_y + size * 1.2));
                b.close();
            });
            frame.fill(&play_triangle, Fill::from(accent_color));
        }

        vec![frame.into_geometry()]
    }
}
