use egui::Stroke;

/// Draws the play position as one continuous line from `top` to the bottom of the window, above
/// every panel and popup; it takes no input, so everything under it stays usable.
pub fn paint_seek_line(ctx: &egui::Context, x: f32, top: f32, left: f32) {
    let screen = ctx.screen_rect();
    if x >= left && x <= screen.right() {
        let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("seek_line")));
        painter.vline(x, top..=screen.bottom(), Stroke::new(2.0_f32, crate::theme::PLAYHEAD));
    }
}
