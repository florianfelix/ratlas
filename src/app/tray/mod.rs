use anyhow::Result;
use tray_icon::{
    MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
    menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem},
};
use winit::event_loop::EventLoopProxy;

/// Embedded application icon bytes used for both window and system tray.
pub const APP_ICON_BYTES: &[u8] = include_bytes!("../../../assets/ratlas.png");

/// Actions triggered from tray icon interaction or its context menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayAction {
    /// Toggle window between hidden and visible (focused).
    ToggleWindow,
    /// Terminate the application cleanly.
    Quit,
}

/// Helper to load a 32-bit RGBA tray icon from embedded asset bytes.
pub fn load_tray_icon() -> Option<tray_icon::Icon> {
    let img = image::load_from_memory(APP_ICON_BYTES).ok()?.into_rgba8();
    let resized = image::imageops::resize(&img, 64, 64, image::imageops::FilterType::Lanczos3);
    let (width, height) = resized.dimensions();
    tray_icon::Icon::from_rgba(resized.into_raw(), width, height).ok()
}

/// Helper to load a window icon from embedded asset bytes.
pub fn load_window_icon() -> Option<winit::window::Icon> {
    let img = image::load_from_memory(APP_ICON_BYTES).ok()?.into_rgba8();
    let (width, height) = img.dimensions();
    winit::window::Icon::from_rgba(img.into_raw(), width, height).ok()
}

/// Manages the system tray icon, its context menu, and mapping tray events to actions.
pub struct TrayManager {
    _tray_icon: Option<TrayIcon>,
    toggle_id: MenuId,
    quit_id: MenuId,
}

impl TrayManager {
    /// Builds the system tray icon with a context menu ("Show / Hide", "Quit")
    /// and associates the embedded application icon.
    pub fn new() -> Result<Self> {
        let tray_menu = Menu::new();
        let toggle_item = MenuItem::new("Show / Hide Ratlas", true, None);
        let quit_item = MenuItem::new("Quit Ratlas", true, None);
        let toggle_id = toggle_item.id().clone();
        let quit_id = quit_item.id().clone();

        tray_menu.append(&toggle_item)?;
        tray_menu.append(&PredefinedMenuItem::separator())?;
        tray_menu.append(&quit_item)?;

        let mut tray_builder = TrayIconBuilder::new()
            .with_tooltip("Ratlas - World Radio")
            .with_menu(Box::new(tray_menu));

        if let Some(icon) = load_tray_icon() {
            tray_builder = tray_builder.with_icon(icon);
        }

        let tray_icon = tray_builder.build()?;

        Ok(Self {
            _tray_icon: Some(tray_icon),
            toggle_id,
            quit_id,
        })
    }

    /// Registers global event handlers for TrayIconEvent and MenuEvent to forward
    /// them asynchronously to the Winit event loop proxy.
    pub fn init_event_handlers<T>(
        proxy: EventLoopProxy<T>,
        map_tray: fn(TrayIconEvent) -> T,
        map_menu: fn(MenuEvent) -> T,
    ) where
        T: Send + 'static,
    {
        let proxy_tray = proxy.clone();
        TrayIconEvent::set_event_handler(Some(move |event| {
            let _ = proxy_tray.send_event(map_tray(event));
        }));

        let proxy_menu = proxy;
        MenuEvent::set_event_handler(Some(move |event| {
            let _ = proxy_menu.send_event(map_menu(event));
        }));
    }

    /// Maps a TrayIconEvent to a corresponding TrayAction.
    /// Returns `Some(TrayAction::ToggleWindow)` when left-clicked.
    pub fn handle_tray_event(&self, event: &TrayIconEvent) -> Option<TrayAction> {
        match event {
            TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state,
                ..
            } => {
                // ksni / statusnotifier triggers on Left click (typically MouseButtonState::Up)
                if *button_state == MouseButtonState::Up {
                    Some(TrayAction::ToggleWindow)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Maps a MenuEvent to a corresponding TrayAction.
    pub fn handle_menu_event(&self, event: &MenuEvent) -> Option<TrayAction> {
        if event.id == self.toggle_id {
            Some(TrayAction::ToggleWindow)
        } else if event.id == self.quit_id {
            Some(TrayAction::Quit)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedded_icon_decodes() {
        assert!(load_tray_icon().is_some());
        assert!(load_window_icon().is_some());
    }

    #[test]
    fn test_tray_action_matching() {
        let item1 = MenuItem::new("Show / Hide Ratlas", true, None);
        let item2 = MenuItem::new("Quit Ratlas", true, None);
        let manager = TrayManager {
            _tray_icon: None,
            toggle_id: item1.id().clone(),
            quit_id: item2.id().clone(),
        };

        assert_eq!(
            manager.handle_menu_event(&MenuEvent {
                id: item1.id().clone()
            }),
            Some(TrayAction::ToggleWindow)
        );
        assert_eq!(
            manager.handle_menu_event(&MenuEvent {
                id: item2.id().clone()
            }),
            Some(TrayAction::Quit)
        );

        let click_event = TrayIconEvent::Click {
            id: tray_icon::TrayIconId::new("test"),
            position: tray_icon::dpi::PhysicalPosition::new(0.0, 0.0),
            rect: tray_icon::Rect {
                position: tray_icon::dpi::PhysicalPosition::new(0.0, 0.0),
                size: tray_icon::dpi::PhysicalSize::new(0, 0),
            },
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
        };
        assert_eq!(
            manager.handle_tray_event(&click_event),
            Some(TrayAction::ToggleWindow)
        );
    }
}
