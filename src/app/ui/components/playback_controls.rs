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
        .width(Length::Fixed(280.0))
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
                        y: slider_y - 12.0,
                        width: slider_width,
                        height: 24.0,
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
                // Request redraw on hover movement to dynamically update hover highlights
                return Some(Action::request_redraw());
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

    fn mouse_interaction(
        &self,
        state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if state.is_dragging_slider {
            return mouse::Interaction::ResizingHorizontally;
        }

        if let Some(pos) = cursor.position_in(bounds) {
            let center_x = bounds.width / 2.0;
            let button_y = bounds.height * 0.4;
            let button_radius = 25.0;

            let slider_y = bounds.height * 0.8;
            let slider_width = bounds.width * 0.7;
            let slider_left = center_x - (slider_width / 2.0);

            if pos.distance(Point::new(center_x, button_y)) <= button_radius {
                return mouse::Interaction::Pointer;
            }

            let slider_rect = Rectangle {
                x: slider_left,
                y: slider_y - 12.0,
                width: slider_width,
                height: 24.0,
            };
            if slider_rect.contains(pos) {
                return mouse::Interaction::Pointer;
            }
        }

        mouse::Interaction::default()
    }

    fn draw(
        &self,
        state: &Self::State,
        renderer: &iced_wgpu::Renderer,
        theme: &Theme,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Vec<iced_wgpu::Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());

        let center_x = bounds.width / 2.0;
        let button_y = bounds.height * 0.4;
        let button_radius = 25.0;

        let slider_y = bounds.height * 0.8;
        let slider_width = bounds.width * 0.7;
        let slider_left = center_x - (slider_width / 2.0);
        let slider_right = center_x + (slider_width / 2.0);

        // Hover detection
        let cursor_pos = cursor.position_in(bounds);
        let is_over_button = cursor_pos
            .map(|pos| pos.distance(Point::new(center_x, button_y)) <= button_radius)
            .unwrap_or(false);

        let slider_rect = Rectangle {
            x: slider_left,
            y: slider_y - 12.0,
            width: slider_width,
            height: 24.0,
        };
        let is_over_slider = state.is_dragging_slider
            || cursor_pos
                .map(|pos| slider_rect.contains(pos))
                .unwrap_or(false);

        // Colors dynamically derived from active theme
        let palette = theme.extended_palette();
        let base_accent = palette.primary.base.color;
        let hover_accent = palette.primary.strong.color;

        let button_color = if is_over_button {
            hover_accent
        } else {
            base_accent
        };
        let slider_color = if is_over_slider {
            hover_accent
        } else {
            base_accent
        };

        let muted_track_color = palette.background.strong.color;
        let frame_color = if is_over_button || is_over_slider {
            palette.secondary.base.color
        } else {
            palette.background.strong.color
        };

        // Subtle background glow around button when hovered
        if is_over_button {
            let button_bg = Path::circle(Point::new(center_x, button_y), button_radius);
            frame.fill(
                &button_bg,
                Fill::from(Color {
                    a: 0.15,
                    ..base_accent
                }),
            );
            frame.stroke(
                &button_bg,
                Stroke::default()
                    .with_color(Color {
                        a: 0.40,
                        ..hover_accent
                    })
                    .with_width(1.0),
            );
        }

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
            Stroke::default().with_color(frame_color).with_width(2.0),
        );

        // --- 2. Volume Slider ---
        // Inactive track
        let slider_track = Path::line(
            Point::new(slider_left, slider_y),
            Point::new(slider_right, slider_y),
        );
        let track_width = if is_over_slider { 5.0 } else { 4.0 };
        frame.stroke(
            &slider_track,
            Stroke::default()
                .with_color(muted_track_color)
                .with_width(track_width),
        );

        // Active (filled) track portion up to current volume
        let handle_x = slider_left + (slider_width * self.volume);
        if self.volume > 0.0 {
            let active_track = Path::line(
                Point::new(slider_left, slider_y),
                Point::new(handle_x, slider_y),
            );
            frame.stroke(
                &active_track,
                Stroke::default()
                    .with_color(slider_color)
                    .with_width(track_width),
            );
        }

        // Volume handle (larger radius & highlight on hover/drag)
        let handle_radius = if is_over_slider { 8.0 } else { 6.0 };
        let slider_handle = Path::circle(Point::new(handle_x, slider_y), handle_radius);
        frame.fill(&slider_handle, Fill::from(slider_color));
        if is_over_slider {
            frame.stroke(
                &slider_handle,
                Stroke::default()
                    .with_color(palette.primary.strong.text)
                    .with_width(1.5),
            );
        }

        // --- 3. Play / Pause Button Graphics ---
        if self.is_playing {
            // Pause symbol (two vertical bars)
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
            frame.fill(&left_bar, Fill::from(button_color));
            frame.fill(&right_bar, Fill::from(button_color));
        } else {
            // Play symbol (triangle)
            let play_triangle = Path::new(|b| {
                let size = 10.0;
                b.move_to(Point::new(center_x - size, button_y - size * 1.2));
                b.line_to(Point::new(center_x + size * 1.4, button_y));
                b.line_to(Point::new(center_x - size, button_y + size * 1.2));
                b.close();
            });
            frame.fill(&play_triangle, Fill::from(button_color));
        }

        vec![frame.into_geometry()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_theme_palette_extraction() {
        for theme in [
            Theme::Dark,
            Theme::Light,
            Theme::SolarizedDark,
            Theme::SolarizedLight,
        ] {
            let palette = theme.extended_palette();
            assert_ne!(palette.primary.base.color.a, 0.0);
            assert_ne!(palette.primary.strong.color.a, 0.0);
            assert_ne!(palette.background.strong.color.a, 0.0);
            assert_ne!(palette.secondary.base.color.a, 0.0);
        }
    }
}
