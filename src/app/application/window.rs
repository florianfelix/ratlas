use std::sync::Arc;
use winit::{event_loop::ActiveEventLoop, window::Window};

use crate::app::{
    app_state::{canvas_renderer::CanvasRenderer, gpu::Gpu},
    application::input::MouseTracker,
    tray::load_window_icon,
    ui::IcedUi,
};

/// Creates a new main Winit window configured with application dimensions, title, and icon.
pub fn create_window(event_loop: &ActiveEventLoop) -> Arc<Window> {
    let window_attributes = Window::default_attributes()
        .with_title("Ratlas floating")
        .with_window_icon(load_window_icon())
        .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 800.0))
        .with_min_inner_size(winit::dpi::LogicalSize::new(640.0, 480.0));

    Arc::new(
        event_loop
            .create_window(window_attributes)
            .expect("unable to create window"),
    )
}

/// Fully unmaps and drops the window and releases all external Arc<Window> handles
/// from the UI clipboard and GPU surfaces so that Wayland / X11 compositors remove the surface.
pub fn hide_window(
    window: &mut Option<Arc<Window>>,
    gpu: &mut Gpu,
    canvas_renderer: &mut CanvasRenderer,
    ui: &mut IcedUi,
    mouse: &mut MouseTracker,
) {
    ui.disconnect_window();
    gpu.destroy_surface();
    canvas_renderer.gpu.destroy_surface();

    if let Some(w) = window.take() {
        let remaining = Arc::strong_count(&w) - 1;
        tracing::info!(
            "Window dropped: unmapping surface (remaining Arc references: {})",
            remaining
        );
        drop(w);
    }

    mouse.reset();
    tracing::info!("Window successfully hidden to system tray");
}

/// Recreates a new Winit window and reconnects GPU surface and UI clipboard context.
pub fn restore_window(
    event_loop: &ActiveEventLoop,
    window: &mut Option<Arc<Window>>,
    gpu: &mut Gpu,
    canvas_renderer: &mut CanvasRenderer,
    ui: &mut IcedUi,
) {
    if window.is_none() {
        let new_window = create_window(event_loop);
        gpu.recreate_surface(new_window.clone());
        canvas_renderer.gpu = gpu.clone();
        ui.reconnect_window(new_window.clone());
        new_window.request_redraw();
        *window = Some(new_window);
        tracing::info!("Window restored from system tray");
    }
}

/// Toggles between hidden and restored window state.
pub fn toggle_window(
    event_loop: &ActiveEventLoop,
    window: &mut Option<Arc<Window>>,
    gpu: &mut Gpu,
    canvas_renderer: &mut CanvasRenderer,
    ui: &mut IcedUi,
    mouse: &mut MouseTracker,
) {
    if window.is_some() {
        hide_window(window, gpu, canvas_renderer, ui, mouse);
    } else {
        restore_window(event_loop, window, gpu, canvas_renderer, ui);
    }
}
