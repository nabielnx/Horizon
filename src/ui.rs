use crate::*;
use chrono::Datelike;
impl Window {
    pub(super) unsafe fn paint(&mut self) -> Result<()> {
        let Some(c) = self.canvas.take() else {
            return Ok(());
        };
        self.hits.clear();
        let shared = self.shared.clone();
        let state = shared.borrow();
        let s = &state.data.settings;
        unsafe {
            crate::trace("paint: begin");
            let track = state
                .media
                .as_ref()
                .and_then(|m| m.snapshot.lock().ok().map(|t| t.clone()))
                .unwrap_or_default();
            if !self.settings {
                self.compact_activity
                    .observe_track(&track, state.running.is_some());
                self.compact_activity.observe_alert(state.alert.as_ref());
                let key = format!(
                    "{}|{}|{}|{}|{}",
                    track.title, track.artist, s.show_lyrics, track.playing, track.cover_revision
                );
                if s.motion
                    && !(self.offscreen && std::env::args().any(|a| a == "--render"))
                    && self.overlay.is_none()
                    && (self.page == 0 && self.expanded || !self.expanded)
                    && self.lyric_key.as_ref().is_some_and(|old| *old != key)
                    && let Ok(bitmap) = c.snapshot()
                {
                    self.overlay = Some(Overlay {
                        bitmap,
                        started: Instant::now(),
                        duration: 0.18,
                        shell: !self.expanded,
                    });
                }
                self.lyric_key = Some(key);
            }
            let colors =
                if !self.settings && s.album_colors && (track.playing || !track.title.is_empty()) {
                    track.palette.unwrap_or(render::palette(s.theme))
                } else {
                    render::palette(if self.settings { 0 } else { s.theme })
                };
            let now = Instant::now();
            let animate = s.motion
                && !self.settings
                && !(self.offscreen && std::env::args().any(|a| a == "--render"));
            self.palette.retarget(colors, animate, now);
            c.set_glass_motion(s.motion && !self.settings);
            c.set_interaction_pressed(self.pressed);
            c.begin_palette(self.palette.sample(now));
            if self.settings {
                self.draw_settings(&c, &state);
            } else {
                self.draw_notch(&c, &state);
                input::draw_fields(&c, self);
            }
            if let Some(overlay) = &self.overlay {
                let t = (overlay.started.elapsed().as_secs_f32() / overlay.duration).clamp(0., 1.);
                c.overlay(&overlay.bitmap, 1. - motion::ease(t), overlay.shell);
            }
            if !self.settings
                && (self.expanded || self.mode == shell_policy::Mode::Peek)
                && let Some(notice) = &self.power_notice
            {
                let x = (s.width as f32 - 240.) / 2.;
                c.rect(x, 1., 240., 18., 8., 0x202823);
                c.design_text(
                    &notice.text(),
                    (x + 5., 1., 230., 18.),
                    (10., 600, 1),
                    0xffffff,
                );
            }
            if let Some(h) = self.focused.and_then(|i| self.hits.get(i)) {
                c.outline(
                    h.x as f32 - 2.,
                    h.y as f32 - 2.,
                    h.w as f32 + 4.,
                    h.h as f32 + 4.,
                    10.,
                    0xcbd4dc,
                );
            }
            let result = c.end();
            self.canvas = Some(c);
            result
        }
    }
    pub(super) fn hit(&mut self, x: i32, y: i32, w: i32, h: i32, action: Action) {
        self.hits.push(Hit { x, y, w, h, action });
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "Buttons carry bounds, action and semantic accent"
    )]
    unsafe fn button(
        &mut self,
        c: &Canvas,
        label: &str,
        x: i32,
        y: i32,
        w: i32,
        action: Action,
        primary: bool,
        accent: Option<u32>,
    ) {
        unsafe {
            let hover = self.pointer.0 >= x
                && self.pointer.0 < x + w
                && self.pointer.1 >= y
                && self.pointer.1 < y + 36;
            let draw_y = y;
            let offset = c.control_glass(
                x as f32,
                draw_y as f32,
                w as f32,
                36.,
                9.,
                accent.or_else(|| (!self.settings).then(|| page_accent(self.page))),
                hover,
                primary,
            );
            c.center(
                label,
                x as f32,
                draw_y as f32 + 8.5 + offset,
                w as f32,
                11.5,
                true,
                if primary || hover { 0xffffff } else { 0xe2e8f0 },
            );
            self.hit(x, y, w, 36, action);
        }
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "Mini buttons carry bounds, custom height, action and accent"
    )]
    unsafe fn mini_button(
        &mut self,
        c: &Canvas,
        label: &str,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        action: Action,
        primary: bool,
        accent: Option<u32>,
    ) {
        unsafe {
            let hover = self.pointer.0 >= x
                && self.pointer.0 < x + w
                && self.pointer.1 >= y
                && self.pointer.1 < y + h;
            let rad = (h as f32 / 2.).min(8.);
            let offset = c.control_glass(
                x as f32,
                y as f32,
                w as f32,
                h as f32,
                rad,
                accent.or_else(|| (!self.settings).then(|| page_accent(self.page))),
                hover,
                primary,
            );
            c.center(
                label,
                x as f32,
                y as f32 + (h as f32 - 12.) / 2. + offset,
                w as f32,
                11.,
                true,
                if primary || hover { 0xffffff } else { 0xe2e8f0 },
            );
            self.hit(x, y, w, h, action);
        }
    }
    pub(super) unsafe fn icon_button(
        &mut self,
        c: &Canvas,
        name: &'static str,
        x: i32,
        y: i32,
        action: Action,
        selected: bool,
    ) {
        unsafe {
            let hover = self.pointer.0 >= x
                && self.pointer.0 < x + 38
                && self.pointer.1 >= y
                && self.pointer.1 < y + 38;
            let draw_y = y;
            let offset = c.control_glass(
                x as f32,
                draw_y as f32,
                38.,
                38.,
                10.,
                Some(button_accent(self.page, action)),
                hover,
                selected,
            );
            if name == "heart" && selected {
                c.icon_filled(
                    "heart",
                    x as f32 + 10.,
                    draw_y as f32 + 10. + offset,
                    18.,
                    0xff4272,
                );
            } else {
                c.icon(
                    name,
                    x as f32 + 10.,
                    draw_y as f32 + 10. + offset,
                    18.,
                    match name {
                        "mic-2" => 0xfff396bf,
                        "play" | "pause" | "external-link" | "plus" => 0xff81dfb5,
                        "folder" | "rotate-ccw" => 0xffedc783,
                        "copy" => 0xff9ecafa,
                        "heart" => 0xfff3a1b9,
                        _ => 0xffe0e9f0,
                    },
                );
            }
            self.hit(x, y, 38, 38, action);
        }
    }
    unsafe fn step_button(
        &mut self,
        c: &Canvas,
        icon: &'static str,
        x: i32,
        y: i32,
        action: Action,
    ) {
        unsafe {
            let size = 32;
            let hover = self.pointer.0 >= x
                && self.pointer.0 < x + size
                && self.pointer.1 >= y
                && self.pointer.1 < y + size;
            let offset = c.control_glass(
                x as f32,
                y as f32,
                size as f32,
                size as f32,
                8.,
                None,
                hover,
                false,
            );
            c.icon(
                icon,
                x as f32 + 8.,
                y as f32 + 8. + offset,
                16.,
                if hover { 0xffffff } else { 0xcbd5e1 },
            );
            self.hit(x, y, size, size, action);
        }
    }
    unsafe fn row(&mut self, c: &Canvas, label: &str, value: &str, key: usize, step: i32, y: i32) {
        unsafe {
            c.rect(230., y as f32, 444., 50., 10., 0x131518);
            c.outline(230., y as f32, 444., 50., 10., 0x22262c);
            c.text(label, 246., y as f32 + 9., 280., 13., true, INK);
            c.text(value, 246., y as f32 + 27., 280., 11., false, 0x8b949e);
            self.step_button(c, "minus", 588, y + 9, Action::Setting(key, -step));
            self.step_button(c, "plus", 628, y + 9, Action::Setting(key, step));
        }
    }
    unsafe fn toggle_row(
        &mut self,
        c: &Canvas,
        title: &str,
        detail: &str,
        key: usize,
        on: bool,
        y: i32,
    ) {
        unsafe {
            let hover = self.pointer.0 >= 230
                && self.pointer.0 < 230 + 444
                && self.pointer.1 >= y
                && self.pointer.1 < y + 50;
            let bg = if hover { 0x171a1e } else { 0x131518 };
            let border = if hover { 0x2c323a } else { 0x22262c };
            c.rect(230., y as f32, 444., 50., 10., bg);
            c.outline(230., y as f32, 444., 50., 10., border);
            c.text(title, 246., y as f32 + 9., 340., 13., true, INK);
            c.text(detail, 246., y as f32 + 27., 340., 11., false, 0x8b949e);
            c.toggle(620., y as f32 + 13., on);
            self.hit(230, y, 444, 50, Action::Setting(key, 1));
        }
    }
    unsafe fn draw_settings(&mut self, c: &Canvas, state: &Shared) {
        unsafe {
            let s = &state.data.settings;
            // Stealth deep black backdrop
            c.rect(0., 0., 710., 550., 0., 0x090a0c);
            c.rect(0., 0., 190., 550., 0., 0x0e1013);
            c.rect(189., 0., 1., 550., 0., 0x1c1f24);

            // Brand header
            c.icon("sparkles", 24., 28., 18., 0xffffff);
            c.text("Horizon", 48., 26., 120., 20., true, 0xffffff);
            c.text("PENGATURAN", 24., 62., 146., 9.5, true, 0x6e7681);

            for (i, (label, _icon)) in [
                ("Tampilan", "sparkles"),
                ("Perilaku", "sliders-horizontal"),
                ("Fokus", "timer"),
                ("Minum air", "droplet"),
                ("Musik", "music-2"),
                ("Notifikasi", "bell"),
                ("Tentang", "info"),
            ]
            .into_iter()
            .enumerate()
            {
                let y = 104 + i as i32 * 46;
                let selected = self.page == i;
                let hover = self.pointer.0 >= 14
                    && self.pointer.0 < 14 + 162
                    && self.pointer.1 >= y
                    && self.pointer.1 < y + 38;
                if selected || hover {
                    c.control_glass(14., y as f32, 162., 38., 8., None, hover, selected);
                }
                if selected {
                    c.rect(14., y as f32 + 9., 3., 20., 1.5, 0xffffff);
                }
                settings_icons::draw(c, i, 23., y as f32 + 5., 28.);
                c.text(
                    label,
                    56.,
                    y as f32 + 10.,
                    114.,
                    12.,
                    selected,
                    if selected {
                        0xffffff
                    } else if hover {
                        0xf1f5f9
                    } else {
                        0x8a94a0
                    },
                );
                self.hit(14, y, 162, 38, Action::Page(i));
            }
            let (title, subtitle) = match self.page {
                0 => ("Tampilan", "Ukuran, warna, dan material."),
                1 => (
                    "Perilaku",
                    "Atur animasi transisi dan integrasi Horizon pada Windows.",
                ),
                2 => (
                    "Fokus",
                    "Sesi kerja terarah dengan timer pengingat yang tenang.",
                ),
                3 => (
                    "Minum air",
                    "Pengingat hidrasi teratur untuk menjaga ritme hari Anda.",
                ),
                4 => (
                    "Musik",
                    "Tampilan lirik sinkron dan adaptasi warna dinamis album.",
                ),
                5 => (
                    "Notifikasi",
                    "Kustomisasi peringatan audio dan jeda waktu pengingat.",
                ),
                _ => (
                    "Tentang Horizon",
                    "Ruang ringkas di atas layar untuk hari yang lebih tenang.",
                ),
            };
            c.text(title, 230., 26., 380., 24., true, 0xffffff);
            settings_icons::draw(c, self.page, 638., 23., 36.);
            c.text(subtitle, 230., 56., 444., 11.5, false, 0x8b949e);
            match self.page {
                0 => {
                    let names = ["Dark titanium", "Forest graphite", "Warm graphite"];
                    let colors = render::palette(s.theme);
                    // Preview Card
                    c.rect(230., 90., 444., 86., 12., 0x131518);
                    c.outline(230., 90., 444., 86., 12., 0x22262c);
                    // Miniature notch preview in center of preview card
                    let notch_w = 260. * (s.width as f32 / 700.).clamp(0.6, 1.0);
                    let notch_h = 42.;
                    let notch_x = 230. + (444. - notch_w) / 2.;
                    c.rect(
                        notch_x,
                        90.,
                        notch_w,
                        notch_h,
                        (s.radius as f32 * 0.4).min(14.),
                        colors[1],
                    );
                    c.outline(
                        notch_x,
                        90.,
                        notch_w,
                        notch_h,
                        (s.radius as f32 * 0.4).min(14.),
                        0x333942,
                    );
                    // Inside mini notch: mini clock and mini droplet/wave
                    c.icon("clock", notch_x + 16., 104., 14., 0xffffff);
                    c.text("18:30", notch_x + 36., 104., 40., 11., true, 0xffffff);
                    c.icon("music-2", notch_x + notch_w - 30., 104., 14., 0xffffff);
                    // Card description
                    c.center(
                        &format!(
                            "{} · {} px · {}% opasitas",
                            names[s.theme],
                            s.width,
                            (s.opacity as f32 / 255. * 100.).round() as u32
                        ),
                        230.,
                        148.,
                        444.,
                        11.,
                        false,
                        0x8b949e,
                    );

                    self.row(c, "Lebar panel", &format!("{} px", s.width), 0, 20, 190);
                    self.row(
                        c,
                        "Opasitas panel",
                        &format!("{}%", (s.opacity as f32 / 255. * 100.).round() as u32),
                        1,
                        13,
                        248,
                    );
                    self.row(c, "Lengkung sudut", &format!("{} px", s.radius), 2, 2, 306);

                    // Theme selector header
                    c.text("Palet dasar tema", 230., 372., 200., 12., true, INK);
                    // 3 Theme cards side-by-side
                    let theme_data = [
                        ("Titanium", 0x7e8a94),
                        ("Forest", 0x6c8a80),
                        ("Warm", 0x9a8272),
                    ];
                    for (i, (name, dot_color)) in theme_data.into_iter().enumerate() {
                        let tx = 230 + i as i32 * 152;
                        let selected = s.theme == i;
                        let hover = self.pointer.0 >= tx
                            && self.pointer.0 < tx + 140
                            && self.pointer.1 >= 396
                            && self.pointer.1 < 396 + 46;
                        let bg = if selected {
                            0x1c2026
                        } else if hover {
                            0x16181c
                        } else {
                            0x131518
                        };
                        let border = if selected {
                            0xffffff
                        } else if hover {
                            0x323842
                        } else {
                            0x22262c
                        };
                        c.rect(tx as f32, 396., 140., 46., 10., bg);
                        c.outline(tx as f32, 396., 140., 46., 10., border);
                        // Swatch dot
                        c.rect(tx as f32 + 14., 409., 20., 20., 6., dot_color);
                        if selected {
                            c.outline(tx as f32 + 14., 409., 20., 20., 6., 0xffffff);
                        }
                        c.text(
                            name,
                            tx as f32 + 42.,
                            411.,
                            80.,
                            12.,
                            selected,
                            if selected { 0xffffff } else { 0x94a3b8 },
                        );
                        self.hit(
                            tx,
                            396,
                            140,
                            46,
                            Action::Setting(3, i as i32 - s.theme as i32),
                        );
                    }
                }
                1 => {
                    self.toggle_row(
                        c,
                        "Animasi antarmode",
                        "Transisi ukuran dan posisi notch bergerak secara halus.",
                        4,
                        s.motion,
                        110,
                    );
                    self.toggle_row(
                        c,
                        "Jalankan saat Windows masuk",
                        "Buka Horizon secara otomatis setelah pengguna masuk.",
                        5,
                        s.startup,
                        174,
                    );
                    // Shortcuts card
                    c.rect(230., 248., 444., 140., 12., 0x131518);
                    c.outline(230., 248., 444., 140., 12., 0x22262c);
                    c.text(
                        "Pintasan Keyboard & Mouse",
                        246.,
                        264.,
                        380.,
                        12.5,
                        true,
                        INK,
                    );

                    // Shortcut 1: Ctrl + Alt + H
                    c.rect(246., 296., 38., 22., 5., 0x1c2025);
                    c.outline(246., 296., 38., 22., 5., 0x323842);
                    c.center("Ctrl", 246., 301., 38., 10.5, true, 0xffffff);

                    c.text("+", 290., 300., 12., 12., false, 0x6e7681);

                    c.rect(304., 296., 36., 22., 5., 0x1c2025);
                    c.outline(304., 296., 36., 22., 5., 0x323842);
                    c.center("Alt", 304., 301., 36., 10.5, true, 0xffffff);

                    c.text("+", 346., 300., 12., 12., false, 0x6e7681);

                    c.rect(360., 296., 26., 22., 5., 0x1c2025);
                    c.outline(360., 296., 26., 22., 5., 0x323842);
                    c.center("H", 360., 301., 26., 10.5, true, 0xffffff);

                    c.text(
                        "Buka / ringkas notch",
                        402.,
                        299.,
                        250.,
                        11.5,
                        false,
                        0xcbd5e1,
                    );

                    // Divider inside card
                    c.rect(246., 334., 412., 1., 0., 0x1c1f24);

                    // Shortcut 2: Klik kanan notch
                    c.rect(246., 348., 120., 22., 5., 0x1c2025);
                    c.outline(246., 348., 120., 22., 5., 0x323842);
                    c.center("Klik kanan notch", 246., 353., 120., 10.5, true, 0xffffff);

                    c.text(
                        "Buka jendela pengaturan",
                        382.,
                        351.,
                        260.,
                        11.5,
                        false,
                        0xcbd5e1,
                    );
                }
                2 => {
                    self.row(
                        c,
                        "Durasi sesi",
                        &format!("{} menit", s.focus_minutes),
                        6,
                        5,
                        110,
                    );
                    c.text("Pilihan durasi cepat", 230., 178., 400., 12., true, INK);
                    for (i, minutes) in [5, 15, 25, 50].into_iter().enumerate() {
                        let bx = 230 + i as i32 * 114;
                        let active = s.focus_minutes as i32 == minutes;
                        self.button(
                            c,
                            &format!("{minutes} menit"),
                            bx,
                            202,
                            102,
                            Action::Setting(6, minutes - s.focus_minutes as i32),
                            active,
                            if active { Some(0xfa5f5f) } else { None },
                        );
                    }
                    // Current Timer Card
                    c.rect(230., 262., 444., 116., 12., 0x131518);
                    c.outline(230., 262., 444., 116., 12., 0x22262c);
                    let left = remaining(state);
                    c.icon("timer", 250., 282., 20., 0xffffff);
                    c.text("Timer saat ini", 278., 283., 340., 11.5, false, 0x8b949e);
                    c.text(
                        &format!("{:02}:{:02}", left / 60, left % 60),
                        250.,
                        314.,
                        360.,
                        34.,
                        true,
                        0xffffff,
                    );
                    c.text(
                        "Perubahan durasi tetap mempertahankan sisa waktu yang sedang berjalan.",
                        230.,
                        398.,
                        444.,
                        10.5,
                        false,
                        0x6e7681,
                    );
                }
                3 => {
                    self.row(
                        c,
                        "Target air harian",
                        &format!("{} ml", s.water_goal),
                        7,
                        250,
                        94,
                    );
                    self.row(c, "Ukuran gelas", &format!("{} ml", s.cup), 8, 50, 152);
                    self.row(
                        c,
                        "Interval minum",
                        &format!("{} menit", s.water_interval),
                        9,
                        1,
                        210,
                    );
                    self.row(
                        c,
                        "Mulai mengingatkan",
                        &format!("{:02}:00", s.water_start_hour),
                        12,
                        1,
                        268,
                    );
                    self.row(
                        c,
                        "Berhenti mengingatkan",
                        &format!("{:02}:00", s.water_end_hour),
                        13,
                        1,
                        326,
                    );
                    self.toggle_row(
                        c,
                        "Pengingat minum",
                        &format!(
                            "Aktif otomatis antara pukul {:02}:00 – {:02}:00.",
                            s.water_start_hour, s.water_end_hour
                        ),
                        11,
                        s.water_enabled,
                        384,
                    );
                }
                4 => {
                    self.toggle_row(
                        c,
                        "Warna dari sampul",
                        "Adaptasikan palet warna notch dengan artwork album lagu.",
                        15,
                        s.album_colors,
                        110,
                    );
                    self.toggle_row(
                        c,
                        "Lirik sinkron",
                        "Tampilkan lirik berjalan di notch pemutar dan saat mode idle.",
                        16,
                        s.show_lyrics,
                        174,
                    );
                    // Info Card
                    c.rect(230., 252., 444., 140., 12., 0x131518);
                    c.outline(230., 252., 444., 140., 12., 0x22262c);
                    c.icon("music-2", 248., 270., 18., 0xffffff);
                    c.text("Integrasi Lirik & Audio", 274., 268., 360., 13., true, INK);
                    c.text(
                        "• Pencarian lirik menggunakan database terbuka LRCLIB.",
                        248.,
                        302.,
                        400.,
                        11.,
                        false,
                        0x8b949e,
                    );
                    c.text(
                        "• Hanya judul lagu, artis, dan durasi yang dikirim saat sinkronisasi.",
                        248.,
                        326.,
                        400.,
                        11.,
                        false,
                        0x8b949e,
                    );
                    c.text(
                        "• Palet dasar tema kembali aktif otomatis saat musik dihentikan.",
                        248.,
                        350.,
                        400.,
                        11.,
                        false,
                        0x8b949e,
                    );
                }
                5 => {
                    self.toggle_row(
                        c,
                        "Suara pengingat",
                        "Bunyi audio notifikasi untuk fokus, tugas, kalender, dan hidrasi.",
                        10,
                        s.sound,
                        110,
                    );
                    self.row(
                        c,
                        "Pengingat acara",
                        &format!("{} menit sebelum acara dimulai", s.event_lead_minutes),
                        14,
                        1,
                        174,
                    );
                    // Audio Test Card
                    c.rect(230., 252., 444., 126., 12., 0x131518);
                    c.outline(230., 252., 444., 126., 12., 0x22262c);
                    c.icon("bell", 248., 270., 18., 0xffffff);
                    c.text("Uji Coba Suara", 274., 268., 360., 13., true, INK);
                    c.text(
                        "Suara pengingat mengikuti volume dan perangkat audio default Windows.",
                        248.,
                        298.,
                        400.,
                        11.,
                        false,
                        0x8b949e,
                    );
                    if s.sound {
                        self.button(
                            c,
                            "Putar nada uji",
                            248,
                            326,
                            140,
                            Action::TestSound,
                            true,
                            Some(0x818cf8),
                        );
                    } else {
                        c.rect(248., 326., 140., 32., 8., 0x181a1e);
                        c.outline(248., 326., 140., 32., 8., 0x282c33);
                        c.center(
                            "Suara dinonaktifkan",
                            248.,
                            334.,
                            140.,
                            11.,
                            false,
                            0x6e7681,
                        );
                    }
                }
                _ => {
                    // Hero About Card
                    c.rect(230., 96., 444., 230., 14., 0x131518);
                    c.outline(230., 96., 444., 230., 14., 0x22262c);

                    // Brand badge
                    c.rect(252., 120., 44., 44., 12., 0x1a1d22);
                    c.outline(252., 120., 44., 44., 12., 0x323842);
                    c.icon("sparkles", 262., 130., 24., 0xffffff);

                    c.text("Horizon", 310., 120., 200., 22., true, 0xffffff);
                    // Version pill badge
                    c.rect(400., 124., 58., 20., 10., 0x1a1d22);
                    c.outline(400., 124., 58., 20., 10., 0x323842);
                    c.center("v0.3.0", 400., 127., 58., 10.5, true, 0xe2e8f0);

                    c.text(
                        "Ruang ringkas di atas layar untuk hari yang lebih tenang.",
                        310.,
                        148.,
                        340.,
                        11.5,
                        false,
                        0x8b949e,
                    );

                    // Divider inside card
                    c.rect(252., 180., 400., 1., 0., 0x1c1f24);

                    // Feature / Storage bullets
                    c.icon("check", 252., 196., 16., 0xffffff);
                    c.text(
                        "Pengaturan, jadwal kalender, dan catatan tersimpan lokal di PC ini.",
                        276.,
                        196.,
                        370.,
                        11.5,
                        false,
                        0xcbd5e1,
                    );
                    c.icon("check", 252., 224., 16., 0xffffff);
                    c.text(
                        "Desain native ultra-ringan dengan konsumsi memori dan CPU minimal.",
                        276.,
                        224.,
                        370.,
                        11.5,
                        false,
                        0xcbd5e1,
                    );
                    c.icon("check", 252., 252., 16., 0xffffff);
                    c.text(
                        "Bebas pelacakan analitik dan menghormati privasi pengguna.",
                        276.,
                        252.,
                        370.,
                        11.5,
                        false,
                        0xcbd5e1,
                    );

                    // Exit Button Card
                    self.button(
                        c,
                        "Keluar dari Horizon",
                        230,
                        350,
                        180,
                        Action::Exit,
                        false,
                        None,
                    );
                }
            }
            // Bottom status bar
            c.rect(230., 512., 6., 6., 3., 0x6e7681);
            c.text(&state.status, 244., 507., 430., 11., false, 0x6e7681);
        }
    }
    unsafe fn draw_notch(&mut self, c: &Canvas, state: &Shared) {
        unsafe {
            if (self.page <= 2 || !self.expanded) && self.draw_figma(c, state) {
                return;
            }
            let s = &state.data.settings;
            let track = state
                .media
                .as_ref()
                .and_then(|m| m.snapshot.lock().ok().map(|t| t.clone()))
                .unwrap_or_default();
            let width = s.width as f32;
            let p = self.expansion;
            let radius = EXPANDED_RADIUS;
            let (compact_w, compact_h) = match self.mode {
                shell_policy::Mode::Indicator => (54., 6.),
                _ => (270., 36.),
            };
            let peek_h = 72.;
            let peek = self.peek;
            let full_w = width;
            let full_h = panel_height(s) as f32;
            let notch_w = compact_w + (full_w - compact_w) * peek.max(p);
            let idle_h = compact_h + (peek_h - compact_h) * peek;
            let notch_h = idle_h + (full_h - idle_h) * p;
            let offset = (full_w - notch_w) / 2.;
            let x = 70.;
            let cw = width - 88.;
            if !(self.offscreen && std::env::args().any(|a| a == "--render")) {
                c.rect(offset, 0., notch_w, notch_h, 0., 0x000000);
                if track.playing && s.album_colors && self.page != 3 {
                    let palette = track.palette.unwrap_or(render::palette(s.theme));
                    c.ambient_glow(notch_w, notch_h, offset, palette[0], palette[1]);
                }
            }
            if !self.expanded {
                c.draw_contour(notch_w, notch_h, p, radius, offset, 0x1c2127);
                if self.mode == shell_policy::Mode::Indicator {
                    c.rect((width - 42.) / 2., 1., 42., 4., 2., 0xc8ffffff);
                    return;
                }
                if self.mode == shell_policy::Mode::Peek {
                    let pad = 32.0f32;
                    let btn_x = width - pad - 38.;
                    if let Some(alert) = &state.alert {
                        let icon = if alert.is_water { "droplet" } else { "bell" };
                        let icon_color = if alert.is_water { 0x5fe0fa } else { 0xfa5f5f };
                        let alert_w = if alert.is_water {
                            (width - pad * 2. - 168.).max(80.)
                        } else {
                            (width - pad * 2. - 88.).max(80.)
                        };
                        c.icon(icon, pad, 28., 18., icon_color);
                        c.text("PENGINGAT", pad + 28., 18., alert_w, 10.5, true, MUTED);
                        c.text(&alert.text, pad + 28., 36., alert_w, 13., true, INK);
                        let close_x = (width - pad - 44.) as i32;
                        if alert.is_water {
                            self.mini_button(
                                c,
                                "+250 ml",
                                close_x - 80,
                                22,
                                72,
                                28,
                                Action::Drink,
                                true,
                                Some(0x5fe0fa),
                            );
                        }
                        self.mini_button(
                            c,
                            "✕",
                            close_x,
                            22,
                            44,
                            28,
                            Action::DismissAlert,
                            false,
                            None,
                        );
                        self.hit(0, 0, close_x - 90, 72, Action::OpenAlert);
                        return;
                    }
                    if track.playing {
                        let cover_x = pad;
                        let cover_y = 17.;
                        let cover_size = 38.;
                        if let Some(bytes) = &track.cover {
                            let _ = c.cover(
                                &format!(
                                    "{}|{}|{}",
                                    track.title, track.artist, track.cover_revision
                                ),
                                bytes,
                                cover_x,
                                cover_y,
                                cover_size,
                            );
                        } else {
                            c.rect(cover_x, cover_y, cover_size, cover_size, 8., 0x41474e);
                            c.icon("music-2", cover_x + 10., cover_y + 10., 18., 0xb4bec7);
                        }
                        let info_x = cover_x + cover_size + 12.;
                        let info_w = (width * 0.27).clamp(96., 146.);
                        c.text(&track.title, info_x, 18., info_w, 13.5, true, INK);
                        c.text(&track.artist, info_x, 38., info_w, 11., false, MUTED);

                        let lyrics_x = info_x + info_w + 16.;
                        let lyrics_max_w = (btn_x - 18. - lyrics_x).max(1.);
                        let time_str = format!(
                            "{} / {}",
                            clock(track.position_now()),
                            clock(track.duration)
                        );
                        let active_lyric = if s.show_lyrics {
                            track
                                .lyric_index()
                                .and_then(|i| track.lyrics.get(i).map(|l| (i, l)))
                        } else {
                            None
                        };
                        let (lyric_text, is_synced_lyric, line_progress) =
                            if let Some((curr_i, curr)) = active_lyric {
                                let line_start = curr.at;
                                let line_end = track
                                    .lyrics
                                    .get(curr_i + 1)
                                    .map(|l| l.at)
                                    .unwrap_or(line_start + 4.0)
                                    .max(line_start + 0.5);
                                let dur = (line_end - line_start).max(0.5);
                                let p = ((track.position_now() - line_start) / dur).clamp(0.0, 1.0)
                                    as f32;
                                (curr.text.as_str(), true, p)
                            } else if s.show_lyrics && !track.title.is_empty() {
                                (track.title.as_str(), false, 1.0)
                            } else {
                                (time_str.as_str(), false, 1.0)
                            };

                        if is_synced_lyric {
                            c.lyric_scroll(
                                lyric_text,
                                lyrics_x,
                                18.,
                                lyrics_max_w,
                                12.5,
                                true,
                                INK,
                                line_progress,
                            );
                        } else {
                            let now = Instant::now();
                            if self.marquee_text != lyric_text {
                                self.marquee_text = lyric_text.to_string();
                                self.marquee_tick = now;
                            }
                            let elapsed = self.marquee_tick.elapsed().as_secs_f64();
                            c.marquee(
                                lyric_text,
                                lyrics_x,
                                18.,
                                lyrics_max_w,
                                12.5,
                                true,
                                INK,
                                elapsed,
                            );
                        }

                        let measured_w = c.measure_text(lyric_text, 12.5, true);
                        let wave_w = measured_w.clamp(48_f32.min(lyrics_max_w), lyrics_max_w);
                        let wave_x = lyrics_x + (lyrics_max_w - wave_w) / 2.;
                        let fraction = if track.duration > 0. {
                            (track.position_now() / track.duration).clamp(0., 1.) as f32
                        } else {
                            0.
                        };
                        c.mini_wave(
                            wave_x,
                            44.,
                            wave_w,
                            fraction,
                            track.position_now(),
                            s.motion,
                        );
                        if track.duration > 0. {
                            self.hit(
                                (wave_x + 2.) as i32,
                                40,
                                (wave_w - 4.) as i32,
                                20,
                                Action::Seek,
                            );
                        }

                        self.icon_button(
                            c,
                            if track.playing { "pause" } else { "play" },
                            btn_x as i32,
                            17,
                            Action::Media(0),
                            false,
                        );
                        self.hit(0, 0, (btn_x - 10.) as i32, 72, Action::OpenAlert);
                    } else {
                        let now = Local::now();
                        let time_str = now.format("%H:%M").to_string();
                        let day_idx = now.weekday().num_days_from_monday() as usize;
                        let full_date = format!(
                            "{}, {} {} {}",
                            calendar::DAYS_FULL[day_idx],
                            now.day(),
                            calendar::MONTHS[now.month0() as usize],
                            now.year()
                        );

                        if state.running.is_some() {
                            c.icon("timer", pad, 28., 18., MUTED);
                            c.text(
                                "HORIZON",
                                pad + 28.,
                                18.,
                                width - pad * 2. - 80.,
                                10.5,
                                true,
                                MUTED,
                            );
                            let n = remaining(state);
                            let label = format!("{:02}:{:02} · Sesi fokus", n / 60, n % 60);
                            c.text(
                                &label,
                                pad + 28.,
                                36.,
                                width - pad * 2. - 80.,
                                13.,
                                true,
                                INK,
                            );
                            c.icon("chevron-down", width - pad - 20., 28., 16., MUTED);
                            self.hit(0, 0, width as i32 - 70, 72, Action::Page(1));
                        } else {
                            // Line 1: Clock icon + Time
                            c.icon("clock", pad, 19., 16., 0x828e9c);
                            c.text(&time_str, pad + 24., 18., 150., 15.5, true, INK);

                            // Line 2: Calendar icon + Full Date
                            c.icon("calendar-days", pad + 1., 40., 14., 0x6e7b88);
                            c.text(&full_date, pad + 24., 39., 160., 11.5, false, MUTED);

                            // Center Glance (starts at x = 246)
                            let glance_x = 246.;
                            let max_right = btn_x - 14.;
                            let content_w = (max_right - glance_x).max(100.);

                            // Tasks Chip (Left Glance)
                            let total_tasks = state.data.tasks.len();
                            let done_tasks = state.data.tasks.iter().filter(|t| t.done).count();
                            let task_pct = if total_tasks > 0 {
                                (done_tasks as f32 / total_tasks as f32).clamp(0., 1.)
                            } else {
                                0.
                            };
                            let col_w = (content_w / 2. - 6.).max(40.);
                            let card2_x = glance_x + col_w + 12.;

                            // Card 1: To-Do Glance
                            c.icon("list-checks", glance_x, 18., 13., 0x5ffa88);
                            c.text(
                                &format!("Tugas · {}/{} selesai", done_tasks, total_tasks),
                                glance_x + 18.,
                                17.,
                                col_w - 18.,
                                11.5,
                                true,
                                INK,
                            );
                            c.rect(glance_x + 18., 32., col_w - 18., 3., 1.5, 0x15251c);
                            let t_fill = (col_w - 18.) * task_pct;
                            if t_fill > 0. {
                                c.rect(glance_x + 18., 32., t_fill, 3., 1.5, 0x5ffa88);
                            }

                            // Card 2: Water Glance
                            let water_pct =
                                (state.data.water as f32 / s.water_goal as f32).clamp(0., 1.);
                            c.icon("droplet", card2_x, 18., 13., 0x5fe0fa);
                            c.text(
                                &format!("Air · {}/{} ml", state.data.water, s.water_goal),
                                card2_x + 18.,
                                17.,
                                col_w - 18.,
                                11.5,
                                true,
                                INK,
                            );
                            c.rect(card2_x + 18., 32., col_w - 18., 3., 1.5, 0x122230);
                            let w_fill = (col_w - 18.) * water_pct;
                            if w_fill > 0. {
                                c.rect(card2_x + 18., 32., w_fill, 3., 1.5, 0x5fe0fa);
                            }

                            // Subtext Line (y = 44)
                            let today = now.date_naive();
                            let today_event = state.data.events.iter().find(|e| {
                                Local
                                    .timestamp_opt(e.at, 0)
                                    .single()
                                    .map(|d| d.date_naive())
                                    == Some(today)
                                    && e.at >= now.timestamp()
                            });
                            if let Some(event) = today_event {
                                let event_time = Local
                                    .timestamp_opt(event.at, 0)
                                    .single()
                                    .map(|t| t.format("%H:%M").to_string())
                                    .unwrap_or_default();
                                c.icon("calendar-days", glance_x, 44., 11., 0xf5a623);
                                c.text(
                                    &format!("Agenda: {} ({})", event.title, event_time),
                                    glance_x + 16.,
                                    43.,
                                    content_w - 16.,
                                    10.0,
                                    false,
                                    0xd0d7dd,
                                );
                            } else if !track.title.is_empty() {
                                c.icon("music-2", glance_x, 44., 11., 0x8a97a4);
                                c.text(
                                    &format!("Jeda: {} · {}", track.title, track.artist),
                                    glance_x + 16.,
                                    43.,
                                    content_w - 16.,
                                    10.0,
                                    false,
                                    0x94a3b8,
                                );
                            } else if !state.data.shelf_files.is_empty() {
                                c.icon("folder", glance_x, 44., 11., 0xfbbf24);
                                c.text(
                                    &format!(
                                        "Rak Berkas: {} file diparkir",
                                        state.data.shelf_files.len()
                                    ),
                                    glance_x + 16.,
                                    43.,
                                    content_w - 16.,
                                    10.0,
                                    false,
                                    0xd0d7dd,
                                );
                            } else {
                                c.text(
                                    "Klik bar untuk membuka panel kontrol penuh",
                                    glance_x,
                                    43.,
                                    content_w,
                                    10.0,
                                    false,
                                    MUTED,
                                );
                            }

                            // Right Action button: Play if paused track exists, else Timer/Pause
                            if !track.title.is_empty() {
                                self.icon_button(
                                    c,
                                    "play",
                                    btn_x as i32,
                                    17,
                                    Action::Media(0),
                                    false,
                                );
                            } else {
                                self.icon_button(
                                    c,
                                    if state.running.is_some() {
                                        "pause"
                                    } else {
                                        "timer"
                                    },
                                    btn_x as i32,
                                    17,
                                    Action::Focus,
                                    false,
                                );
                            }

                            // Entire bar clickable to expand to full calendar/dashboard
                            self.hit(0, 0, (btn_x - 10.) as i32, 72, Action::OpenAlert);
                        }
                    }
                    return;
                }
                if let Some(alert) = &state.alert {
                    let left = (width - 270.) / 2.;
                    let icon = if alert.is_water { "droplet" } else { "bell" };
                    let icon_color = if alert.is_water { 0x5bc0de } else { 0xf5a623 };
                    c.icon(icon, left + 24., 10., 14., icon_color);
                    c.center(&alert.text, left + 45., 8., 196., 12., true, INK);
                    self.hit(left as i32, 0, 270, 36, Action::OpenAlert);
                    return;
                }
                let track = state
                    .media
                    .as_ref()
                    .and_then(|m| m.snapshot.lock().ok().map(|t| t.clone()))
                    .unwrap_or_default();
                if state.running.is_none() && track.playing {
                    let left = (width - 270.) / 2.;
                    if s.show_lyrics {
                        c.icon("music-2", left + 24., 10., 14., INK);
                        let active_lyric = track
                            .lyric_index()
                            .and_then(|i| track.lyrics.get(i).map(|l| (i, l)));
                        if let Some((curr_i, curr)) = active_lyric {
                            let line_start = curr.at;
                            let line_end = track
                                .lyrics
                                .get(curr_i + 1)
                                .map(|l| l.at)
                                .unwrap_or(line_start + 4.0)
                                .max(line_start + 0.5);
                            let dur = (line_end - line_start).max(0.5);
                            let p =
                                ((track.position_now() - line_start) / dur).clamp(0.0, 1.0) as f32;
                            c.lyric_scroll(&curr.text, left + 47., 6., 196., 13., true, INK, p);
                        } else {
                            let text = if track.title.is_empty() {
                                "Tidak ada musik"
                            } else {
                                &track.title
                            };
                            let now = Instant::now();
                            if self.marquee_text != text {
                                self.marquee_text = text.to_string();
                                self.marquee_tick = now;
                            }
                            let elapsed = self.marquee_tick.elapsed().as_secs_f64();
                            c.marquee(text, left + 47., 6., 196., 13., true, INK, elapsed);
                        }
                        let fraction = if track.duration > 0. {
                            (track.position_now() / track.duration).clamp(0., 1.) as f32
                        } else {
                            0.
                        };
                        c.mini_wave(
                            left + 47.,
                            23.,
                            196.,
                            fraction,
                            track.position_now(),
                            s.motion,
                        );
                        if track.duration > 0. {
                            self.hit((left + 49.) as i32, 20, 192, 16, Action::Seek);
                        }
                        self.hit(left as i32, 0, 270, 36, Action::OpenAlert);
                        return;
                    } else {
                        let now = Local::now();
                        let time_str = now.format("%H:%M").to_string();
                        let day_idx = now.weekday().num_days_from_monday() as usize;
                        let short_date = format!(
                            "{}, {} {}",
                            calendar::DAYS_SHORT[day_idx],
                            now.day(),
                            calendar::MONTHS_SHORT[now.month0() as usize]
                        );
                        let time_w = c.measure_text(&time_str, 12., true);
                        let date_w = c.measure_text(&short_date, 12., false);
                        let total_w = 12. + 6. + time_w + 10. + 2.5 + 10. + 12. + 6. + date_w;
                        let start_x = left + (270. - total_w) / 2.;
                        let y_icon = 5.5;
                        let y_text = 5.0;
                        let y_dot = 10.0;

                        c.icon("clock", start_x, y_icon, 12., 0x828e9c);
                        let time_x = start_x + 18.;
                        c.text(&time_str, time_x, y_text, time_w + 10., 12., true, INK);

                        let dot_x = time_x + time_w + 10.;
                        c.rect(dot_x, y_dot, 2.5, 2.5, 1.25, 0x626c77);

                        let cal_x = dot_x + 2.5 + 10.;
                        c.icon("calendar-days", cal_x, y_icon, 12., 0x828e9c);

                        let date_x = cal_x + 18.;
                        c.text(
                            &short_date,
                            date_x,
                            y_text,
                            date_w + 10.,
                            12.,
                            false,
                            0x9fb0c0,
                        );

                        let fraction = if track.duration > 0. {
                            (track.position_now() / track.duration).clamp(0., 1.) as f32
                        } else {
                            0.
                        };
                        c.mini_wave(
                            left + 36.,
                            22.,
                            198.,
                            fraction,
                            track.position_now(),
                            s.motion,
                        );
                        if track.duration > 0. {
                            self.hit((left + 38.) as i32, 20, 194, 16, Action::Seek);
                        }
                        self.hit(left as i32, 0, 270, 36, Action::OpenAlert);
                        return;
                    }
                }
                if state.running.is_some() {
                    let n = remaining(state);
                    let label = format!("{:02}:{:02} · Fokus", n / 60, n % 60);
                    let left = (width - 270.) / 2.;
                    c.icon("timer", left + 25., 10., 14., MUTED);
                    c.center(&label, left + 45., 8., 200., 13., true, INK);
                    self.hit(left as i32, 0, 270, 36, Action::Page(1));
                    return;
                }
                let now = Local::now();
                let time_str = now.format("%H:%M").to_string();
                let day_idx = now.weekday().num_days_from_monday() as usize;
                let short_date = format!(
                    "{}, {} {}",
                    calendar::DAYS_SHORT[day_idx],
                    now.day(),
                    calendar::MONTHS_SHORT[now.month0() as usize]
                );
                let left = (width - 270.) / 2.;
                let time_w = c.measure_text(&time_str, 12., true);
                let date_w = c.measure_text(&short_date, 12., false);
                let total_w = 12. + 6. + time_w + 10. + 2.5 + 10. + 12. + 6. + date_w;
                let start_x = left + (270. - total_w) / 2.;
                let y_icon = 12.5;
                let y_text = 12.0;
                let y_dot = 17.0;

                c.icon("clock", start_x, y_icon, 12., 0x828e9c);
                let time_x = start_x + 18.;
                c.text(&time_str, time_x, y_text, time_w + 10., 12., true, INK);

                let dot_x = time_x + time_w + 10.;
                c.rect(dot_x, y_dot, 2.5, 2.5, 1.25, 0x626c77);

                let cal_x = dot_x + 2.5 + 10.;
                c.icon("calendar-days", cal_x, y_icon, 12., 0x828e9c);

                let date_x = cal_x + 18.;
                c.text(
                    &short_date,
                    date_x,
                    y_text,
                    date_w + 10.,
                    12.,
                    false,
                    0x9fb0c0,
                );

                self.hit(left as i32, 0, 270, 36, Action::OpenAlert);
                return;
            }
            c.rect(width / 2. - 14., 7., 28., 3., 1.5, 0x656d74);
            if self.page != 2 {
                let now = Local::now();
                c.icon("clock", width - 82., 9., 11., 0x70808f);
                c.text(
                    &now.format("%H:%M").to_string(),
                    width - 66.,
                    8.,
                    45.,
                    11.,
                    true,
                    0x8a97a4,
                );
            }
            self.figma_sidebar(c);
            match self.page {
                0 => {
                    let track = state
                        .media
                        .as_ref()
                        .and_then(|m| m.snapshot.lock().ok().map(|s| s.clone()))
                        .unwrap_or_default();
                    let fraction = if track.duration > 0. {
                        (track.position_now() / track.duration).clamp(0., 1.) as f32
                    } else {
                        0.
                    };

                    if cw >= 360. {
                        let player_w = 216.0f32.min(cw * 0.48);
                        let lyrics_x = x + player_w + 22.;
                        let lyrics_w = (cw - player_w - 22.).max(150.);

                        // --- LEFT COLUMN: MEDIA PLAYER ---
                        let cover_size = 54.;
                        let cover_x = x;
                        let cover_y = 36.;
                        if let Some(bytes) = &track.cover {
                            let _ = c.cover(
                                &format!(
                                    "{}|{}|{}",
                                    track.title, track.artist, track.cover_revision
                                ),
                                bytes,
                                cover_x,
                                cover_y,
                                cover_size,
                            );
                        } else {
                            c.rect(cover_x, cover_y, cover_size, cover_size, 10., 0x181d22);
                            c.icon("music-2", cover_x + 18., cover_y + 18., 18., 0x8a97a4);
                        }
                        c.outline(cover_x, cover_y, cover_size, cover_size, 10., 0x2e353d);

                        let info_x = cover_x + cover_size + 12.;
                        let info_w = player_w - cover_size - 12.;
                        let title = if track.title.is_empty() {
                            "Tidak ada musik"
                        } else {
                            &track.title
                        };
                        let now = Instant::now();
                        if self.title_text != title {
                            self.title_text = title.to_string();
                            self.title_tick = now;
                        }
                        let elapsed = self.title_tick.elapsed().as_secs_f64();
                        c.marquee_left(title, info_x, 36., info_w, 14.5, true, INK, elapsed);
                        c.text(
                            if track.artist.is_empty() {
                                "Putar di Spotify / browser"
                            } else {
                                &track.artist
                            },
                            info_x,
                            58.,
                            info_w,
                            11.,
                            false,
                            MUTED,
                        );
                        c.icon("headphones", info_x, 80., 11., MUTED);
                        c.text(
                            "Media Windows",
                            info_x + 16.,
                            78.,
                            info_w - 16.,
                            10.,
                            false,
                            MUTED,
                        );

                        // Timeline & signature waveform scrubber
                        c.text(
                            &clock(track.position_now()),
                            x,
                            104.,
                            50.,
                            10.,
                            false,
                            MUTED,
                        );
                        c.text(
                            &clock(track.duration),
                            x + player_w - 40.,
                            104.,
                            40.,
                            10.,
                            false,
                            MUTED,
                        );
                        c.wave(x, 114., player_w, fraction, track.position_now(), s.motion);
                        if track.duration > 0. {
                            self.hit(x as i32 + 7, 120, player_w as i32 - 14, 28, Action::Seek);
                        }

                        // Media controls - 5 cohesive 38x38 squircle buttons with uniform spacing
                        let btn_y = 168;
                        let btn_size = 38.;
                        let total_btn_w = 5. * btn_size;
                        let gap = ((player_w - total_btn_w) / 4.).clamp(4., 14.);
                        let start_x = x + (player_w - (total_btn_w + gap * 4.)) * 0.5;

                        let b1 = start_x;
                        let b2 = b1 + btn_size + gap;
                        let b3 = b2 + btn_size + gap;
                        let b4 = b3 + btn_size + gap;
                        let b5 = b4 + btn_size + gap;

                        let track_key = format!("{}|{}", track.title, track.artist);
                        let is_fav =
                            !track.title.is_empty() && state.data.favorites.contains(&track_key);

                        self.icon_button(
                            c,
                            "mic-2",
                            b1.round() as i32,
                            btn_y,
                            Action::Setting(16, 1),
                            s.show_lyrics,
                        );
                        self.icon_button(
                            c,
                            "skip-back",
                            b2.round() as i32,
                            btn_y,
                            Action::Media(1),
                            false,
                        );
                        let pp_hover = self.pointer.0 >= b3.round() as i32
                            && self.pointer.0 < b3.round() as i32 + 38
                            && self.pointer.1 >= btn_y
                            && self.pointer.1 < btn_y + 38;
                        let pp_accent = track.palette.map(|p| p[0]);
                        let offset = c.control_glass(
                            b3,
                            btn_y as f32,
                            38.,
                            38.,
                            10.,
                            pp_accent,
                            pp_hover,
                            track.playing,
                        );
                        let play_icon = if track.playing { "pause" } else { "play" };
                        let icon_offset_x = if track.playing { 11. } else { 12. };
                        c.icon(
                            play_icon,
                            b3 + icon_offset_x,
                            btn_y as f32 + 11. + offset,
                            16.,
                            0xffffff,
                        );
                        self.hit(b3.round() as i32, btn_y, 38, 38, Action::Media(0));
                        self.icon_button(
                            c,
                            "skip-forward",
                            b4.round() as i32,
                            btn_y,
                            Action::Media(2),
                            false,
                        );
                        self.icon_button(
                            c,
                            "heart",
                            b5.round() as i32,
                            btn_y,
                            Action::Favorite,
                            is_fav,
                        );

                        // Divider line
                        c.rect(x + player_w + 11., 36., 1., 170., 0.5, 0x1c2127);

                        // --- RIGHT COLUMN: SYNCED LYRICS ---
                        if s.show_lyrics {
                            if track.lyrics.is_empty() {
                                c.text(
                                    if track.title.is_empty() {
                                        "Tidak ada musik"
                                    } else {
                                        &track.title
                                    },
                                    lyrics_x,
                                    90.,
                                    lyrics_w,
                                    15.,
                                    true,
                                    INK,
                                );
                                let status_text =
                                    if !track.lyrics_loaded && track.lyrics_retry.is_some() {
                                        "Menunggu layanan lirik"
                                    } else if !track.lyrics_loaded {
                                        "Mencari lirik"
                                    } else {
                                        "Lirik belum tersedia"
                                    };
                                c.text(
                                    status_text,
                                    lyrics_x,
                                    116.,
                                    lyrics_w,
                                    11.5,
                                    false,
                                    if !track.lyrics_loaded {
                                        MUTED
                                    } else {
                                        0x8a97a4
                                    },
                                );
                                if track.lyrics_loaded && !track.title.is_empty() {
                                    c.text(
                                        "LRCLIB", lyrics_x, 138., lyrics_w, 9.5, false, 0x525c68,
                                    );
                                }
                            } else {
                                // Lyric display area: y=36..206, height=170px
                                // Use lyric_window to centre active line with context above/below
                                let curr_i = track.lyric_index().unwrap_or(0);
                                let total = track.lyrics.len();
                                // capacity: active(1) + up to 3 past + up to 4 future = 8 rows
                                let capacity = 8_usize;
                                let window = crate::layout::lyric_window(total, curr_i, capacity);

                                // Compute progress for active line
                                let curr_opt = track.lyrics.get(curr_i);
                                let line_progress = if let Some(curr) = curr_opt {
                                    let ls = curr.at;
                                    let le = track
                                        .lyrics
                                        .get(curr_i + 1)
                                        .map(|l| l.at)
                                        .unwrap_or(ls + 4.0)
                                        .max(ls + 0.5);
                                    let dur = (le - ls).max(0.5);
                                    ((track.position_now() - ls) / dur).clamp(0.0, 1.0) as f32
                                } else {
                                    1.0_f32
                                };

                                // Active line at y=96 (size=16.5, ~30px tall)
                                // Each context row = 20px, size=12.5
                                let active_y = 96.;
                                let row_h = 20.;
                                let ctx_size = 12.5;

                                for (slot, line_i) in window.enumerate() {
                                    let offset = line_i as i32 - curr_i as i32;
                                    if offset == 0 {
                                        // Active line — karaoke scroll effect
                                        if let Some(curr) = track.lyrics.get(curr_i) {
                                            c.lyric_scroll(
                                                &curr.text,
                                                lyrics_x,
                                                active_y,
                                                lyrics_w,
                                                16.5,
                                                true,
                                                0xffffff,
                                                line_progress,
                                            );
                                        }
                                    } else {
                                        // Context lines — fade by distance from active
                                        let y = if offset < 0 {
                                            // Past lines above active
                                            active_y + offset as f32 * row_h
                                        } else {
                                            // Future lines below active (gap for 30px active row)
                                            active_y + 30. + (offset - 1) as f32 * row_h
                                        };
                                        if y < 36. || y + ctx_size > 210. {
                                            continue;
                                        }
                                        let dist = offset.unsigned_abs();
                                        // dist=1 brightest context, gets dimmer further out
                                        let col = match (offset < 0, dist) {
                                            (true, 1) => 0x6e7681,  // 1 before  — light muted
                                            (true, 2) => 0x505b68,  // 2 before
                                            (true, _) => 0x3d4652,  // 3+ before — very dim
                                            (false, 1) => 0x87939f, // 1 after   — lightest future
                                            (false, 2) => 0x6b7785, // 2 after
                                            (false, 3) => 0x505b68, // 3 after
                                            (false, _) => 0x3d4652, // 4+ after  — very dim
                                        };
                                        if let Some(line) = track.lyrics.get(line_i) {
                                            c.text(
                                                &line.text, lyrics_x, y, lyrics_w, ctx_size, false,
                                                col,
                                            );
                                        }
                                    }
                                    let _ = slot;
                                }
                            }
                        } else {
                            c.text(
                                if track.title.is_empty() {
                                    "Tidak ada musik"
                                } else {
                                    &track.title
                                },
                                lyrics_x,
                                90.,
                                lyrics_w,
                                15.,
                                true,
                                INK,
                            );
                            c.text(
                                "Lirik nonaktif",
                                lyrics_x,
                                116.,
                                lyrics_w,
                                11.,
                                false,
                                MUTED,
                            );
                        }
                    } else {
                        self.draw_compact_player(c, state);
                    }
                }
                1 => self.draw_focus(c, state),
                2 => self.draw_calendar(c, state),
                3 => self.draw_water(c, state),
                4 => self.draw_tasks(c, state),
                5 => self.draw_shelf(c, state),
                _ => {}
            }
        }
    }

    pub(super) unsafe fn draw_water(&mut self, c: &Canvas, state: &Shared) {
        unsafe {
            let s = &state.data.settings;
            let width = s.width as f32;
            let x = 70.;
            let cw = width - 88.;
            let card_y = 26.0;
            let card_h = panel_height(s) as f32 - card_y - 16.;

            // Smoked glass container with 1px top specular highlight
            c.rect(x, card_y, cw, card_h, 12., 0x191919);
            c.outline(x, card_y, cw, card_h, 12., 0x1e232b);
            c.glass_edge(x, card_y, cw, card_h, 12., 0.);
            c.rect(x + 12., card_y + 1., cw - 24., 1., 0.5, 0x18ffffff);

            // Monotonic time for live undulating wave animation
            let seconds = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs_f64())
                .unwrap_or(0.);

            let remaining_ml = s.water_goal.saturating_sub(state.data.water);
            let fraction = (remaining_ml as f32 / s.water_goal as f32).clamp(0., 1.);
            if self.water_fill < 0. {
                self.water_fill = fraction;
            }
            let visible_fill = if s.motion && !self.offscreen {
                self.water_fill
            } else {
                fraction
            };

            // --- LEFT COLUMN: HERO WATER BOTTLE STAGE ---
            let stage_x = x + 16.;
            let stage_w = 170.0f32.min(cw * 0.38);
            let bottle_w = 84.0f32.min(stage_w - 44.);
            let bottle_h = card_h - 28.;
            let bottle_x = stage_x + (stage_w - bottle_w);
            let bottle_y = card_y + 14.; // Aligned flush with right column header (card_y + 14.)

            // Sisa Air display placed beside the bottle (left column) in pure crisp white
            let info_x = stage_x + 2.;
            let info_w = (bottle_x - stage_x - 8.).max(54.);
            let pct_left = ((visible_fill * 100.).round() as u32).min(100);

            if remaining_ml > 0 {
                c.text(
                    "SISA AIR",
                    info_x,
                    bottle_y + 2.,
                    info_w,
                    9.0,
                    true,
                    0x8294a5,
                );
                let sisa_str = format!("{} ml", remaining_ml);
                c.text(
                    &sisa_str,
                    info_x,
                    bottle_y + 14.,
                    info_w,
                    15.0,
                    true,
                    0xffffff,
                );
                let pct_str = format!("{}% tersisa", pct_left);
                c.text(
                    &pct_str,
                    info_x,
                    bottle_y + 34.,
                    info_w,
                    9.5,
                    false,
                    0xd1d5db,
                );
            } else {
                c.text("STATUS", info_x, bottle_y + 2., info_w, 9.0, true, 0x8294a5);
                c.text(
                    "HABIS",
                    info_x,
                    bottle_y + 14.,
                    info_w,
                    15.0,
                    true,
                    0x5ffa88,
                );
                c.text(
                    "Target tercapai",
                    info_x,
                    bottle_y + 34.,
                    info_w,
                    9.5,
                    false,
                    0xd1d5db,
                );
            }

            // Measurement Ticks on the left side of the bottle body
            let body_top = bottle_y + 16.;
            let body_h = bottle_h - 16.;
            let inner_h = body_h - 4.;
            let liquid_bottom = body_top + 2. + inner_h;

            let marks: [(f32, &str); 3] = [(0.75, "1.5L"), (0.50, "1.0L"), (0.25, "0.5L")];

            for &(frac, default_lbl) in &marks {
                let y_tick = liquid_bottom - (inner_h - 4.) * frac;
                let reached = visible_fill >= frac;

                let dynamic_ml = (s.water_goal as f32 * frac).round();
                let label = if (s.water_goal as i32 - 2000).abs() < 50 {
                    default_lbl.to_string()
                } else if dynamic_ml >= 1000. {
                    format!("{:.1}L", dynamic_ml / 1000.)
                } else {
                    format!("{}m", dynamic_ml as u32)
                };

                let tick_col = if reached { 0x3de8ff } else { 0x22323e };
                let text_col = if reached { 0x3de8ff } else { 0x485a6a };

                c.line(
                    (bottle_x - 14., y_tick),
                    (bottle_x - 4., y_tick),
                    1.5,
                    tick_col,
                );
                c.text(
                    &label,
                    bottle_x - 42.,
                    y_tick - 6.,
                    26.,
                    8.5,
                    reached,
                    text_col,
                );
                if reached {
                    c.rect(bottle_x - 5., y_tick - 1., 2., 2., 1., 0x3de8ff);
                }
            }

            // Render living water bottle with undulating waves and bubbles
            c.water_bottle(
                bottle_x,
                bottle_y,
                bottle_w,
                bottle_h,
                visible_fill,
                seconds,
                s.motion,
            );

            // Interactive tap-to-drink hit region on the bottle itself
            self.hit(
                bottle_x as i32,
                bottle_y as i32,
                bottle_w as i32,
                bottle_h as i32,
                Action::Drink,
            );

            // Vertical column divider
            let divider_x = stage_x + stage_w + 14.;
            c.rect(divider_x, card_y + 16., 1., card_h - 32., 0.5, 0x1a232c);

            // --- RIGHT COLUMN: METRICS & CONTROLS ---
            let right_x = divider_x + 18.;
            let right_w = (x + cw - 18. - right_x).max(120.);

            // Header: Droplet Icon + "HIDRASI"
            c.icon("droplet", right_x, card_y + 14., 14., 0x3de8ff);
            c.text(
                "HIDRASI",
                right_x + 20.,
                card_y + 14.,
                120.,
                11.0,
                true,
                0x3de8ff,
            );

            // Hero metric display
            let val_str = format!("{} ml", state.data.water);
            c.text(
                &val_str,
                right_x,
                card_y + card_h * 0.22,
                200.,
                24.0,
                true,
                0xf0f6fc,
            );
            let pct_drunk =
                ((state.data.water as f32 / s.water_goal as f32).min(1.0) * 100.).round() as u32;
            let sub_str = format!("{}% diminum · Target {} ml", pct_drunk, s.water_goal);
            c.text(
                &sub_str,
                right_x,
                card_y + card_h * 0.36,
                right_w,
                10.5,
                false,
                0x8294a5,
            );

            // Separator line
            c.rect(right_x, card_y + card_h * 0.47, right_w, 1., 0.5, 0x303030);

            // Quick Drink Pill Deck: 4 Buttons (-150, -250, -500, Reset)
            let btn_y = (card_y + card_h - 90.) as i32;
            let btn_gap = 6.0f32;
            let btn_w = ((right_w - btn_gap * 3.) / 4.).floor() as i32;

            let b1 = right_x.round() as i32;
            let b2 = (right_x + (btn_w as f32 + btn_gap)).round() as i32;
            let b3 = (right_x + (btn_w as f32 + btn_gap) * 2.).round() as i32;
            let b4 = (right_x + (btn_w as f32 + btn_gap) * 3.).round() as i32;

            self.mini_button(
                c,
                "-150",
                b1,
                btn_y,
                btn_w,
                30,
                Action::DrinkAmount(150),
                false,
                None,
            );

            // Selected quick action uses the same opaque petrol material.
            let b2_hover = self.pointer.0 >= b2
                && self.pointer.0 < b2 + btn_w
                && self.pointer.1 >= btn_y
                && self.pointer.1 < btn_y + 30;
            let offset = c.control_glass(
                b2 as f32,
                btn_y as f32,
                btn_w as f32,
                30.,
                8.,
                Some(render::WATER_FILL),
                b2_hover,
                true,
            );
            c.center(
                "-250",
                b2 as f32,
                btn_y as f32 + (30. - 12.) / 2. + offset,
                btn_w as f32,
                11.,
                true,
                0xffffff,
            );
            self.hit(b2, btn_y, btn_w, 30, Action::DrinkAmount(250));

            self.mini_button(
                c,
                "-500",
                b3,
                btn_y,
                btn_w,
                30,
                Action::DrinkAmount(500),
                false,
                None,
            );

            // Button 4: Reset (centered rotate-ccw SVG icon + typography)
            let b4_hover = self.pointer.0 >= b4
                && self.pointer.0 < b4 + btn_w
                && self.pointer.1 >= btn_y
                && self.pointer.1 < btn_y + 30;
            let rad = (30.0f32 / 2.).min(8.);
            let offset = c.control_glass(
                b4 as f32,
                btn_y as f32,
                btn_w as f32,
                30.,
                rad,
                Some(render::WATER_FILL),
                b4_hover,
                false,
            );
            let b4_col = if b4_hover { 0xffffff } else { 0xe2e8f0 };
            let reset_text = "Reset";
            let icon_sz = 10.0f32;
            let gap = 4.0f32;
            let text_w = c.measure_text(reset_text, 10.5, true);
            let total_w = icon_sz + gap + text_w;
            if btn_w as f32 >= total_w + 6.0 {
                let start_x = b4 as f32 + (btn_w as f32 - total_w) * 0.5;
                let icon_y = btn_y as f32 + (30.0 - icon_sz) * 0.5 + offset;
                let text_y = btn_y as f32 + (30.0 - 12.0) * 0.5 + offset;
                c.icon("rotate-ccw", start_x, icon_y, icon_sz, b4_col);
                c.text(
                    reset_text,
                    start_x + icon_sz + gap,
                    text_y,
                    text_w + 4.0,
                    10.5,
                    true,
                    b4_col,
                );
            } else {
                c.center(
                    reset_text,
                    b4 as f32,
                    btn_y as f32 + (30.0 - 12.0) * 0.5 + offset,
                    btn_w as f32,
                    10.5,
                    true,
                    b4_col,
                );
            }
            self.hit(b4, btn_y, btn_w, 30, Action::ResetWater);

            // Primary Action Button: 100% full opacity solid button [ - Minum Air (-250 ml) ]
            let prim_y = (card_y + card_h - 50.) as i32;
            let prim_x = right_x.round() as i32;
            let prim_w = right_w.round() as i32;
            let prim_hover = self.pointer.0 >= prim_x
                && self.pointer.0 < prim_x + prim_w
                && self.pointer.1 >= prim_y
                && self.pointer.1 < prim_y + 36;
            let draw_prim_y = prim_y;

            let cup_amount = s.cup.max(100);
            let drink_label = format!("- Minum Air (-{} ml)", cup_amount);

            let offset = c.control_glass(
                prim_x as f32,
                draw_prim_y as f32,
                prim_w as f32,
                36.,
                9.,
                Some(render::WATER_FILL),
                prim_hover,
                true,
            );
            c.center(
                &drink_label,
                prim_x as f32,
                draw_prim_y as f32 + 8.5 + offset,
                prim_w as f32,
                12.0,
                true,
                0xffffff,
            );
            self.hit(prim_x, prim_y, prim_w, 36, Action::Drink);
        }
    }
}
pub(super) fn clock(seconds: f64) -> String {
    let n = seconds.max(0.) as u64;
    format!("{:02}:{:02}", n / 60, n % 60)
}

pub(super) fn format_file_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

pub(super) fn file_category(ext: &str) -> (&'static str, u32, &'static str) {
    match ext {
        "png" | "jpg" | "jpeg" | "webp" | "bmp" | "ico" | "gif" | "svg" => {
            ("sparkles", 0xfbbf24, "Gambar")
        }
        "txt" | "md" | "rs" | "json" | "log" | "py" | "js" | "ts" | "toml" | "yaml" | "yml"
        | "html" | "css" | "c" | "cpp" | "sql" => ("file-text", 0x818cf8, "Dokumen/Kode"),
        "pdf" | "doc" | "docx" | "ppt" | "pptx" | "xls" | "xlsx" => {
            ("file-text", 0xfb7185, "Dokumen")
        }
        "zip" | "rar" | "7z" | "tar" | "gz" => ("folder", 0x34d399, "Arsip"),
        "mp3" | "flac" | "wav" | "m4a" | "ogg" => ("music-2", 0xa78bfa, "Audio"),
        "mp4" | "mkv" | "mov" | "avi" => ("play", 0x38bdf8, "Video"),
        _ => ("folder", 0x94a3b8, "Berkas"),
    }
}

fn page_accent(page: usize) -> u32 {
    match page {
        1 => 0xfa5f5f,
        2 => 0x818cf8,
        3 => 0x5fe0fa,
        4 => render::TASK_ACCENT,
        5 => 0xeec16c,
        _ => 0xffffff,
    }
}

fn button_accent(page: usize, action: Action) -> u32 {
    match action {
        Action::Media(_) | Action::Seek | Action::Favorite | Action::Setting(16, _) => {
            page_accent(0)
        }
        _ => page_accent(page),
    }
}

#[cfg(test)]
mod material_tests {
    use super::*;

    #[test]
    fn media_buttons_keep_music_colour_after_visiting_any_page() {
        for last_page in 0..6 {
            for action in [
                Action::Media(0),
                Action::Media(1),
                Action::Media(2),
                Action::Favorite,
                Action::Setting(16, 1),
            ] {
                assert_eq!(button_accent(last_page, action), page_accent(0));
            }
            assert_eq!(
                button_accent(last_page, Action::Reset),
                page_accent(last_page)
            );
        }
    }
}
