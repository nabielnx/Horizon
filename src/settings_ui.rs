//! Settings composition uses the same opaque materials and motion as the shell.
use crate::*;

const X: f32 = 212.;
const W: f32 = 474.;
const MUTED_TEXT: u32 = 0xa2aab7;
const PAGES: [(&str, &str); 7] = [
    ("Tampilan", "Bentuk dan karakter Horizon."),
    ("Perilaku", "Gerak, akses cepat, dan integrasi Windows."),
    ("Fokus", "Sesuaikan ritme sesi kerja Anda."),
    ("Minum air", "Target dan jadwal hidrasi harian."),
    ("Musik", "Warna sampul dan lirik yang mengikuti musik."),
    ("Notifikasi", "Suara dan waktu pengingat."),
    ("Tentang", "Horizon, versi, dan penyimpanan lokal."),
];

impl Window {
    fn settings_hover(&self, b: (f32, f32, f32, f32)) -> bool {
        self.pointer.0 as f32 >= b.0
            && (self.pointer.0 as f32) < b.0 + b.2
            && self.pointer.1 as f32 >= b.1
            && (self.pointer.1 as f32) < b.1 + b.3
    }
    unsafe fn settings_card(&self, c: &Canvas, y: f32, height: f32) {
        unsafe {
            c.glass_panel((X, y, W, height), 14., 0x1a1c21, 0.);
        }
    }
    unsafe fn settings_caption(&self, c: &Canvas, text: &str, y: f32) {
        unsafe {
            c.text(text, X + 2., y, W, 10., true, MUTED_TEXT);
        }
    }
    unsafe fn settings_action(
        &mut self,
        c: &Canvas,
        text: &str,
        b: (i32, i32, i32, i32),
        action: Action,
    ) {
        unsafe {
            let bounds = (b.0 as f32, b.1 as f32, b.2 as f32, b.3 as f32);
            let dy = c.glass_button(bounds, 10., self.settings_hover(bounds));
            c.center(
                text,
                bounds.0,
                bounds.1 + 10. + dy,
                bounds.2,
                11.,
                true,
                0xffffff,
            );
            self.hit(b.0, b.1, b.2, b.3, action);
        }
    }
    unsafe fn settings_value(
        &mut self,
        c: &Canvas,
        title: &str,
        value: &str,
        key: usize,
        step: i32,
        y: i32,
    ) {
        unsafe {
            self.settings_card(c, y as f32, 52.);
            c.text(title, X + 16., y as f32 + 17., 220., 12., true, INK);
            c.center(value, 438., y as f32 + 18., 152., 11., false, 0xdce2eb);
            for (icon, x, delta) in [("minus", 598, -step), ("plus", 640, step)] {
                let b = (x as f32, y as f32 + 9., 32., 34.);
                let dy = c.glass_button(b, 9., self.settings_hover(b));
                c.icon_centered(icon, (b.0, b.1 + dy, b.2, b.3), 15.);
                self.hit(x, y + 9, 32, 34, Action::Setting(key, delta));
            }
        }
    }
    unsafe fn settings_switch(
        &mut self,
        c: &Canvas,
        title: &str,
        detail: &str,
        key: usize,
        on: bool,
        y: i32,
    ) {
        unsafe {
            let b = (X, y as f32, W, 66.);
            let hover = c.hover_amount(b, self.settings_hover(b));
            c.glass_panel(b, 14., motion::mix(0x1a1c21, 0x25282e, hover), hover);
            c.text(title, X + 16., y as f32 + 13., 354., 12., true, INK);
            c.text(
                detail,
                X + 16.,
                y as f32 + 35.,
                354.,
                10.,
                false,
                MUTED_TEXT,
            );
            let t = c.animate_value(
                (620., y as f32 + 20., 46., 26.),
                2,
                f32::from(on),
                motion::STATE,
            );
            c.glass_panel(
                (620., y as f32 + 20., 46., 26.),
                13.,
                motion::mix(0x363b44, 0x3c9b67, t),
                hover,
            );
            let press = c.animate_value(
                b,
                1,
                f32::from(self.pressed && self.settings_hover(b)),
                motion::PRESS,
            );
            c.rect(
                623. + 20. * t,
                y as f32 + 23.,
                20. + press,
                20.,
                10.,
                0xf5f8fb,
            );
            self.hit(X as i32, y, W as i32, 66, Action::Setting(key, 1));
        }
    }
    pub(super) unsafe fn draw_settings(&mut self, c: &Canvas, state: &Shared) {
        unsafe {
            let s = &state.data.settings;
            c.rect(0., 0., 710., 550., 0., 0x101216);
            c.rect(0., 0., 190., 550., 0., 0x17191e);
            c.rect(189., 0., 1., 550., 0., 0x343840);
            c.glass_panel((18., 22., 34., 34.), 11., 0x30343c, 0.);
            c.icon_centered("sparkles", (18., 22., 34., 34.), 21.);
            c.text("Horizon", 62., 23., 112., 18., true, INK);
            c.text("PENGATURAN", 62., 47., 114., 8., true, MUTED_TEXT);
            for (i, (name, _)) in PAGES.iter().enumerate() {
                let y = 100. + i as f32 * 49.;
                let b = (12., y, 166., 42.);
                let hover = c.hover_amount(b, self.settings_hover(b));
                let active = c.animate_value(b, 2, f32::from(self.page == i), motion::STATE);
                let press = c.animate_value(
                    b,
                    1,
                    f32::from(self.pressed && self.settings_hover(b)),
                    motion::PRESS,
                );
                let dy = if s.motion { -hover * 0.5 + press } else { 0. };
                c.glass_panel(
                    (b.0, y + dy, b.2, b.3),
                    12.,
                    motion::mix(0x17191e, 0x353a43, active.max(hover * 0.55)),
                    active.max(hover),
                );
                settings_icons::draw(c, i, 23., y + 9. + dy, 24.);
                c.text(
                    name,
                    60.,
                    y + 13. + dy,
                    112.,
                    11.5,
                    self.page == i,
                    motion::mix(0xa4adba, 0xffffff, active.max(hover)),
                );
                self.hit(12, y as i32, 166, 42, Action::Page(i));
            }
            c.text(
                &format!("v{}", env!("CARGO_PKG_VERSION")),
                24.,
                506.,
                140.,
                10.,
                false,
                MUTED_TEXT,
            );
            let page = self.page.min(6);
            c.text(PAGES[page].0, X, 27., W, 25., true, INK);
            c.text(PAGES[page].1, X, 65., W, 11., false, MUTED_TEXT);
            match page {
                0 => {
                    self.settings_card(c, 100., 100.);
                    c.text("PRATINJAU", X + 16., 114., 130., 8., true, MUTED_TEXT);
                    let nw = 180. + (s.width - 360) as f32 * 0.12;
                    let nx = X + (W - nw) / 2.;
                    c.glass_panel(
                        (nx, 137., nw, 43.),
                        (expanded_radius(s) * 0.55).min(21.),
                        render::palette(s.theme)[1],
                        0.3,
                    );
                    c.icon_centered("music-notes", (nx + 12., 145., 27., 27.), 18.);
                    c.text("Horizon", nx + 49., 145., 118., 11., true, INK);
                    c.rect(nx + 49., 165., nw - 101., 3., 1.5, 0x9ea8b5);
                    self.settings_value(c, "Lebar panel", &format!("{} px", s.width), 0, 20, 212);
                    self.settings_value(
                        c,
                        "Opasitas Air & Berkas",
                        &format!("{}%", (s.opacity as f32 / 255. * 100.).round()),
                        1,
                        13,
                        272,
                    );
                    self.settings_value(
                        c,
                        "Lengkung panel",
                        &format!("{} px", expanded_radius(s) as i32),
                        2,
                        2,
                        332,
                    );
                    self.settings_caption(c, "PALET DASAR", 402.);
                    for (i, (name, swatch)) in [
                        ("Titanium", 0x9ea8b5),
                        ("Forest", 0x88a894),
                        ("Warm", 0xbea28b),
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        let b = (X + i as f32 * 162., 424., 150., 49.);
                        let t = c.animate_value(b, 2, f32::from(s.theme == i), motion::STATE);
                        let h = c.hover_amount(b, self.settings_hover(b));
                        c.glass_panel(
                            b,
                            12.,
                            motion::mix(0x1d2026, 0x353b45, t.max(h * 0.5)),
                            t.max(h),
                        );
                        c.ellipse(b.0 + 14., 440., 15., 15., swatch);
                        c.text(name, b.0 + 39., 441., 88., 11., s.theme == i, INK);
                        self.hit(
                            b.0 as i32,
                            424,
                            150,
                            49,
                            Action::Setting(3, i as i32 - s.theme as i32),
                        );
                    }
                }
                1 => {
                    self.settings_switch(
                        c,
                        "Animasi & interaksi",
                        "Spring, transisi halaman, dan refleksi saat hover.",
                        4,
                        s.motion,
                        108,
                    );
                    self.settings_switch(
                        c,
                        "Mulai bersama Windows",
                        "Horizon tersedia otomatis setelah Anda masuk.",
                        5,
                        s.startup,
                        184,
                    );
                    self.settings_switch(
                        c,
                        "Tampil di atas taskbar",
                        "Semua ukuran menimpa taskbar di tepi atas layar.",
                        17,
                        s.taskbar_layout,
                        260,
                    );
                    self.settings_card(c, 348., 128.);
                    self.settings_caption(c, "AKSES CEPAT", 362.);
                    c.text("Ctrl + Alt + H", X + 18., 391., 154., 12., true, INK);
                    c.text(
                        "Buka / ringkas panel",
                        X + 200.,
                        392.,
                        247.,
                        11.,
                        false,
                        MUTED_TEXT,
                    );
                    c.rect(X + 18., 421., W - 36., 1., 0., 0x343840);
                    c.text("Klik kanan panel", X + 18., 441., 166., 12., true, INK);
                    c.text(
                        "Buka pengaturan",
                        X + 200.,
                        442.,
                        247.,
                        11.,
                        false,
                        MUTED_TEXT,
                    );
                }
                2 => {
                    self.settings_value(
                        c,
                        "Durasi sesi",
                        &format!("{} menit", s.focus_minutes),
                        6,
                        5,
                        108,
                    );
                    self.settings_caption(c, "DURASI CEPAT", 187.);
                    for (i, minutes) in [5, 15, 25, 50].into_iter().enumerate() {
                        self.settings_action(
                            c,
                            &format!("{minutes} menit"),
                            (212 + i as i32 * 122, 211, 108, 40),
                            Action::Setting(6, minutes - s.focus_minutes as i32),
                        );
                    }
                    self.settings_card(c, 278., 134.);
                    c.icon("timer", X + 18., 299., 20., 0xffffff);
                    c.text("Sesi saat ini", X + 49., 301., 340., 11., false, MUTED_TEXT);
                    let left = remaining(state);
                    c.text(
                        &format!("{:02}:{:02}", left / 60, left % 60),
                        X + 18.,
                        335.,
                        240.,
                        34.,
                        true,
                        INK,
                    );
                    c.text(
                        "Waktu sesi berjalan tetap dipertahankan.",
                        X + 2.,
                        433.,
                        W,
                        10.5,
                        false,
                        MUTED_TEXT,
                    );
                }
                3 => {
                    self.settings_value(
                        c,
                        "Target harian",
                        &format!("{} ml", s.water_goal),
                        7,
                        250,
                        104,
                    );
                    self.settings_value(c, "Ukuran gelas", &format!("{} ml", s.cup), 8, 50, 164);
                    self.settings_value(
                        c,
                        "Interval minum",
                        &format!("{} menit", s.water_interval),
                        9,
                        1,
                        224,
                    );
                    self.settings_value(
                        c,
                        "Jam mulai",
                        &format!("{:02}:00", s.water_start_hour),
                        12,
                        1,
                        284,
                    );
                    self.settings_value(
                        c,
                        "Jam selesai",
                        &format!("{:02}:00", s.water_end_hour),
                        13,
                        1,
                        344,
                    );
                    self.settings_switch(
                        c,
                        "Pengingat minum",
                        "Jadwal melewati tengah malam tetap didukung.",
                        11,
                        s.water_enabled,
                        411,
                    );
                }
                4 => {
                    self.settings_switch(
                        c,
                        "Warna dari sampul",
                        "Palet dan glow mengikuti warna cover lagu.",
                        15,
                        s.album_colors,
                        108,
                    );
                    self.settings_switch(
                        c,
                        "Lirik sinkron",
                        "Karaoke serta lirik berjalan di panel kecil/sedang.",
                        16,
                        s.show_lyrics,
                        184,
                    );
                    self.settings_card(c, 278., 150.);
                    c.icon("music-notes", X + 18., 299., 22., 0xffffff);
                    c.text(
                        "Musik yang sedang berjalan",
                        X + 52.,
                        301.,
                        388.,
                        12.,
                        true,
                        INK,
                    );
                    c.text(
                        "Kontrol mengikuti sesi media Windows.",
                        X + 18.,
                        340.,
                        W - 36.,
                        11.,
                        false,
                        MUTED_TEXT,
                    );
                    c.text(
                        "Lirik dari LRCLIB; timing kata mengikuti data lagu.",
                        X + 18.,
                        368.,
                        W - 36.,
                        11.,
                        false,
                        MUTED_TEXT,
                    );
                    c.text(
                        "Warna sampul tetap tersedia saat musik dijeda.",
                        X + 18.,
                        396.,
                        W - 36.,
                        10.5,
                        false,
                        MUTED_TEXT,
                    );
                }
                5 => {
                    self.settings_switch(
                        c,
                        "Suara pengingat",
                        "Untuk fokus, tugas, kalender, dan minum air.",
                        10,
                        s.sound,
                        108,
                    );
                    self.settings_value(
                        c,
                        "Sebelum acara",
                        &format!("{} menit", s.event_lead_minutes),
                        14,
                        1,
                        190,
                    );
                    self.settings_card(c, 278., 150.);
                    c.text(
                        "Dengarkan nada pengingat",
                        X + 18.,
                        299.,
                        W - 36.,
                        13.,
                        true,
                        INK,
                    );
                    c.text(
                        "Mengikuti volume dan perangkat audio Windows.",
                        X + 18.,
                        329.,
                        W - 36.,
                        11.,
                        false,
                        MUTED_TEXT,
                    );
                    if s.sound {
                        self.settings_action(
                            c,
                            "Putar nada uji",
                            (230, 365, 154, 40),
                            Action::TestSound,
                        );
                    } else {
                        c.text(
                            "Aktifkan suara untuk mencoba nada.",
                            X + 18.,
                            375.,
                            W - 36.,
                            11.,
                            false,
                            MUTED_TEXT,
                        );
                    }
                }
                _ => {
                    self.settings_card(c, 108., 300.);
                    c.glass_panel((234., 130., 48., 48.), 15., 0x333943, 0.3);
                    c.icon_centered("sparkles", (234., 130., 48., 48.), 28.);
                    c.text("Horizon", 300., 131., 300., 24., true, INK);
                    c.text(
                        &format!("Versi {}", env!("CARGO_PKG_VERSION")),
                        300.,
                        168.,
                        300.,
                        11.,
                        false,
                        MUTED_TEXT,
                    );
                    c.text(
                        "Ruang ringkas untuk ritme hari Anda.",
                        234.,
                        215.,
                        422.,
                        13.,
                        true,
                        INK,
                    );
                    c.rect(234., 253., 430., 1., 0., 0x343840);
                    for (i, text) in [
                        "Pengaturan, tugas, dan jadwal tersimpan lokal.",
                        "Kontrol media terhubung ke sesi Windows.",
                        "Lirik dimuat melalui layanan LRCLIB.",
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        c.icon("check", 234., 278. + i as f32 * 34., 15., 0xffffff);
                        c.text(
                            text,
                            262.,
                            278. + i as f32 * 34.,
                            398.,
                            11.,
                            false,
                            MUTED_TEXT,
                        );
                    }
                    self.settings_action(
                        c,
                        "Keluar dari Horizon",
                        (212, 432, 180, 40),
                        Action::Exit,
                    );
                }
            }
            c.rect(X, 495., W, 1., 0., 0x30353e);
            let failed = state.status.contains("belum") || state.status.contains("gagal");
            let footer = if failed {
                state.status.as_str()
            } else {
                "Perubahan tersimpan otomatis di perangkat ini."
            };
            c.text(
                footer,
                X + 2.,
                510.,
                W - 4.,
                10.,
                false,
                if failed { 0xffb1a7 } else { MUTED_TEXT },
            );
        }
    }
}
