use crate::render::Canvas;

pub unsafe fn draw(c: &Canvas, index: usize, x: f32, y: f32, size: f32) {
    let icon = [
        "sparkles",
        "sliders-horizontal",
        "timer",
        "drop",
        "music-notes",
        "bell",
        "info",
    ][index.min(6)];
    unsafe {
        c.icon_centered(icon, (x, y, size, size), size * 0.85);
    }
}
