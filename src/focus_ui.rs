use crate::*;

enum Control {
    Preset(bool),
    Primary,
    Reset,
}

impl Window {
    pub(super) unsafe fn draw_focus(&mut self, c: &Canvas, state: &Shared) {
        unsafe {
            let x = 70.;
            let cw = (state.data.settings.width - 88) as f32;
            let left = remaining(state);
            let total = state.data.settings.focus_minutes as u64 * 60;
            let running = state.running.is_some();
            let paused = !running && left > 0 && left < total;
            let (status, ink) = if left == 0 {
                ("Selesai", 0x9bcdbb)
            } else if running {
                ("Berjalan", 0xe6a3af)
            } else if paused {
                ("Dijeda", 0xffffff)
            } else {
                ("Siap mulai", 0x9baab5)
            };
            c.text("Fokus", x, 34., 100., 14., true, INK);
            let status_w = c.measure_text(status, 10., false);
            let status_x = x + cw - status_w;
            c.ellipse(status_x - 12., 40., 4., 4., ink);
            c.text(status, status_x, 35., status_w + 1., 10., false, ink);

            let narrow = cw < 352.;
            let timer_w = if narrow { cw } else { (cw * 0.54).round() };
            let timer_h = if narrow { 96. } else { 164. };
            c.rect(x, 64., timer_w, timer_h, 11., 0x241c25);
            c.outline(x, 64., timer_w, timer_h, 11., 0x473442);
            c.glass_edge(x, 64., timer_w, timer_h, 11., 0.);
            c.text(
                "WAKTU TERSISA",
                x + 16.,
                if narrow { 76. } else { 81. },
                timer_w - 32.,
                9.,
                true,
                0x8c9ba9,
            );
            let digits = format!("{:02}:{:02}", left / 60, left % 60);
            c.center(
                &digits,
                x,
                if narrow { 91. } else { 103. },
                timer_w,
                if narrow { 34. } else { 42. },
                true,
                if left == 0 { 0xb3d9c8 } else { 0xe9edf1 },
            );
            let duration = format!("dari {} menit", state.data.settings.focus_minutes);
            if !narrow {
                c.center(&duration, x, 168., timer_w, 10., false, 0x8c9ba9);
            }
            let bar_x = x + 18.;
            let bar_y = if narrow { 147. } else { 203. };
            let bar_w = timer_w - 36.;
            let fraction = (1. - left as f32 / total.max(1) as f32).clamp(0., 1.);
            let fraction = c.animate_value((bar_x, bar_y, bar_w, 4.), 4, fraction, motion::STATE);
            c.rect(bar_x, bar_y, bar_w, 4., 2., 0x2b333d);
            if fraction > 0. {
                c.rect(
                    bar_x,
                    bar_y,
                    (bar_w * fraction).max(1.),
                    4.,
                    2.,
                    if left == 0 { 0x9bcdbb } else { 0xd49aa7 },
                );
            }

            let controls_x = if narrow { x } else { x + timer_w + 14. };
            let controls_w = if narrow { cw } else { cw - timer_w - 14. };
            if !narrow {
                c.text(
                    "Durasi sesi",
                    controls_x,
                    66.,
                    controls_w,
                    10.,
                    false,
                    0x9baab5,
                );
            }
            for (i, minutes) in [15, 25, 45, 60].into_iter().enumerate() {
                let (xx, yy, ww, hh) = if narrow {
                    let ww = ((cw - 18.) / 4.).floor();
                    (x + i as f32 * (ww + 6.), 172., ww, 24.)
                } else {
                    let ww = ((controls_w - 8.) / 2.).floor();
                    (
                        controls_x + (i % 2) as f32 * (ww + 8.),
                        84. + (i / 2) as f32 * 44.,
                        ww,
                        36.,
                    )
                };
                self.focus_control(
                    c,
                    input::Bounds {
                        x: xx as i32,
                        y: yy as i32,
                        width: ww as i32,
                        height: hh as i32,
                    },
                    Action::SetFocusMinutes(minutes),
                    &format!("{} m", minutes),
                    Control::Preset(state.data.settings.focus_minutes == minutes),
                );
            }
            let button_y = if narrow { 204 } else { 190 };
            let button_h = if narrow { 28 } else { 38 };
            let main_w = controls_w as i32 - button_h - 8;
            self.focus_control(
                c,
                input::Bounds {
                    x: controls_x as i32,
                    y: button_y,
                    width: main_w,
                    height: button_h,
                },
                Action::Focus,
                if running {
                    "Jeda"
                } else if left == 0 {
                    "Mulai lagi"
                } else if paused {
                    "Lanjut"
                } else {
                    "Mulai"
                },
                Control::Primary,
            );
            self.focus_control(
                c,
                input::Bounds {
                    x: controls_x as i32 + main_w + 8,
                    y: button_y,
                    width: button_h,
                    height: button_h,
                },
                Action::Reset,
                "",
                Control::Reset,
            );
        }
    }

    unsafe fn focus_control(
        &mut self,
        c: &Canvas,
        b: input::Bounds,
        action: Action,
        label: &str,
        kind: Control,
    ) {
        unsafe {
            let hover = b.contains(self.pointer.0, self.pointer.1);
            let x = b.x as f32;
            let y = b.y as f32;
            let w = b.width as f32;
            let h = b.height as f32;
            let offset = c.control_glass(
                x,
                y,
                w,
                h,
                8.,
                None,
                hover,
                matches!(kind, Control::Primary | Control::Preset(true)),
            );
            let y = y + offset;
            let ink = 0xffffff;
            if matches!(kind, Control::Reset) {
                c.icon(
                    "rotate-ccw",
                    x + (w - 14.) / 2.,
                    y + (h - 14.) / 2.,
                    14.,
                    ink,
                );
            } else {
                c.center(
                    label,
                    x,
                    y + (h - 15.) / 2.,
                    w,
                    if h < 30. { 9.5 } else { 11. },
                    true,
                    ink,
                );
            }
            self.hit(b.x, b.y, b.width, b.height, action);
        }
    }
}
