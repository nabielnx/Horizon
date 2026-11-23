//! Settings uses neutral charcoal surfaces with solid selection fills.
use crate::*;

const X: f32 = 204.;
const W: f32 = 482.;
const BACKGROUND: u32 = 0x111111;
const MUTED_TEXT: u32 = 0xaaaaaa;
const SIDEBAR: u32 = 0x171717;
const CARD: u32 = 0x1e1e1e;
const CONTROL: u32 = 0x2b2b2b;
const BORDER: u32 = 0x393939;
const SELECTED: u32 = 0x414141;
const PRIMARY: u32 = 0x656565;
const NAV_Y: [f32; 9] = [108., 146., 205., 243., 281., 319., 378., 416., 454.];
const PAGES: [(&str, &str); 9] = [
    ("Tampilan", "Bentuk dan karakter Horizon."),
    ("Perilaku", "Gerak, akses cepat, dan integrasi Windows."),
    ("Fokus", "Sesuaikan ritme sesi kerja Anda."),
    ("Minum air", "Target dan jadwal hidrasi harian."),
    ("Musik", "Warna sampul dan lirik yang mengikuti musik."),
    ("Notifikasi", "Suara dan waktu pengingat."),
    (
        "Opasitas",
        "Atur tiap halaman dan ukuran idle secara terpisah.",
    ),
    ("Tentang", "Horizon, versi, dan penyimpanan lokal."),
    ("AI", "Pilih penyedia dan model untuk Asisten Horizon."),
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
            c.rect(X, y, W, height, 12., CARD);
            c.outline(X, y, W, height, 12., BORDER);
        }
    }
    unsafe fn settings_divider(&self, c: &Canvas, y: f32) {
        unsafe { c.rect(X + 16., y, W - 32., 1., 0., BORDER) }
    }
    unsafe fn settings_caption(&self, c: &Canvas, text: &str, y: f32) {
        unsafe {
            c.text(text, X + 2., y, W, 10., true, MUTED_TEXT);
        }
    }
    unsafe fn settings_button(&self, c: &Canvas, b: (f32, f32, f32, f32), selected: bool) -> f32 {
        unsafe {
            let hovered = self.settings_hover(b);
            let hover = c.hover_amount(b, hovered);
            let press = c.animate_value(b, 1, f32::from(self.pressed && hovered), motion::PRESS);
            let active = c.animate_value(b, 2, f32::from(selected), motion::STATE);
            let dy = if self.shared.borrow().data.settings.motion {
                press
            } else {
                0.
            };
            let fill = motion::mix(CONTROL, SELECTED, active);
            c.rect(
                b.0,
                b.1 + dy,
                b.2,
                b.3,
                8.,
                motion::mix(fill, 0x606060, hover * 0.25),
            );
            dy
        }
    }
    unsafe fn settings_action(
        &mut self,
        c: &Canvas,
        text: &str,
        b: (i32, i32, i32, i32),
        action: Action,
        selected: bool,
    ) {
        unsafe {
            let bounds = (b.0 as f32, b.1 as f32, b.2 as f32, b.3 as f32);
            let dy = self.settings_button(c, bounds, selected);
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
    unsafe fn settings_value_row(
        &mut self,
        c: &Canvas,
        title: &str,
        value: &str,
        key: usize,
        step: i32,
        y: i32,
    ) {
        unsafe {
            c.text(title, X + 16., y as f32 + 17., 244., 12., true, INK);
            self.settings_stepper(
                c,
                (X + W - 178., y as f32 + 8., 162., 32.),
                value,
                key,
                step,
            );
        }
    }
    unsafe fn settings_stepper(
        &mut self,
        c: &Canvas,
        b: (f32, f32, f32, f32),
        value: &str,
        key: usize,
        step: i32,
    ) {
        unsafe {
            let (x, y, width, height) = b;
            let button = 30.;
            c.rect(x, y, width, height, 8., CONTROL);
            c.center(
                value,
                x + button,
                y + (height - 14.) / 2.,
                width - button * 2.,
                11.,
                true,
                INK,
            );
            for (icon, left, delta) in [("minus", x, -step), ("plus", x + width - button, step)] {
                let hit = (left, y, button, height);
                let dy = self.settings_button(c, hit, false);
                c.icon_centered(icon, (left, y + dy, button, height), 14.);
                self.hit(
                    left as i32,
                    y as i32,
                    button as i32,
                    height as i32,
                    Action::Setting(key, delta),
                );
            }
            for divider in [x + button, x + width - button] {
                c.rect(divider, y + 6., 1., height - 12., 0., BORDER);
            }
        }
    }
    unsafe fn settings_switch_row(
        &mut self,
        c: &Canvas,
        title: &str,
        detail: &str,
        key: usize,
        on: bool,
        y: i32,
    ) {
        unsafe {
            let b = (X + 4., y as f32 + 3., W - 8., 54.);
            let hover = c.hover_amount(b, self.settings_hover(b));
            c.rect(
                b.0,
                b.1,
                b.2,
                b.3,
                8.,
                motion::mix(CARD, CONTROL, hover * 0.55),
            );
            c.text(title, X + 16., y as f32 + 11., 354., 12., true, INK);
            c.text(
                detail,
                X + 16.,
                y as f32 + 32.,
                354.,
                10.,
                false,
                MUTED_TEXT,
            );
            let t = c.animate_value(
                (620., y as f32 + 17., 46., 26.),
                2,
                f32::from(on),
                motion::STATE,
            );
            c.rect(
                620.,
                y as f32 + 17.,
                46.,
                26.,
                13.,
                motion::mix(0x383838, PRIMARY, t),
            );
            let press = c.animate_value(
                b,
                1,
                f32::from(self.pressed && self.settings_hover(b)),
                motion::PRESS,
            );
            c.rect(
                623. + 20. * t,
                y as f32 + 20.,
                20. + press,
                20.,
                10.,
                0xf5f5f5,
            );
            self.hit(
                b.0 as i32,
                b.1 as i32,
                b.2 as i32,
                b.3 as i32,
                Action::Setting(key, 1),
            );
        }
    }
    pub(super) unsafe fn draw_settings(&mut self, c: &Canvas, state: &Shared) {
        unsafe {
            let s = &state.data.settings;
            c.rect(0., 0., 710., 550., 0., BACKGROUND);
            c.rect(0., 0., 176., 550., 0., SIDEBAR);
            c.rect(175., 0., 1., 550., 0., BORDER);
            let _ = c.design_asset(
                "horizon-brand",
                include_bytes!("../assets/branding/app-icon.png"),
                (18., 17., 34., 34.),
            );
            c.text("Horizon", 65., 22., 96., 17., true, INK);
            c.text(i18n::tr("PENGATURAN"), 66., 45., 96., 8., true, MUTED_TEXT);
            for (title, y) in [("UMUM", 87.), ("AKTIVITAS", 184.), ("APLIKASI", 357.)] {
                c.text(i18n::tr(title), 20., y, 130., 9., true, MUTED_TEXT);
            }
            for (i, &(name, _)) in PAGES.iter().enumerate() {
                let y = NAV_Y[i];
                let b = (12., y, 152., 34.);
                let hover = c.hover_amount(b, self.settings_hover(b));
                let active = c.animate_value(b, 2, f32::from(self.page == i), motion::STATE);
                let press = c.animate_value(
                    b,
                    1,
                    f32::from(self.pressed && self.settings_hover(b)),
                    motion::PRESS,
                );
                let dy = if s.motion { -hover * 0.5 + press } else { 0. };
                let row = motion::mix(SIDEBAR, CONTROL, hover);
                c.rect(
                    b.0,
                    y + dy,
                    b.2,
                    b.3,
                    9.,
                    motion::mix(row, SELECTED, active),
                );
                settings_icons::draw(c, i, 22., y + 7. + dy, 20.);
                c.text(
                    i18n::tr(name),
                    51.,
                    y + 9. + dy,
                    105.,
                    11.5,
                    self.page == i,
                    motion::mix(MUTED_TEXT, 0xffffff, active.max(hover)),
                );
                self.hit(12, y as i32, 152, 34, Action::Page(i));
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
            let page = self.page.min(PAGES.len() - 1);
            c.text(i18n::tr(PAGES[page].0), X, 24., W, 27., true, INK);
            c.text(i18n::tr(PAGES[page].1), X, 62., W, 11., false, MUTED_TEXT);
            c.rect(X, 86., W, 1., 0., BORDER);
            match page {
                0 => {
                    self.settings_card(c, 100., 151.);
                    c.rect(X + 16., 112., W - 32., 87., 8., BACKGROUND);
                    let nw = 180. + (s.width - 360) as f32 * 0.12
                        - if s.dynamic_island { 28. } else { 0. };
                    let nx = X + (W - nw) / 2.;
                    let py = if s.dynamic_island { 118. } else { 112. };
                    let floating = f32::from(s.dynamic_island);
                    let shape: Vec<_> =
                        layout::contour_styled(nw, 42., 1. - 9. / 14., 37., floating)
                            .into_iter()
                            .map(|(x, y)| (nx + x, py + y))
                            .collect();
                    c.polygon(&shape, render::palette(s.theme)[1]);
                    c.presence_eyes((nx + 12., py, 43., 42.), mascot::EyePose::default());
                    c.design_glass_clock_sized("12:42", (nx + 56., py + 3., nw - 112., 36.), 20.);
                    c.rect(nx + nw - 45., py + 19., 20., 3., 1.5, 0x9a9a9a);
                    for (i, name) in ["Notch", "Dynamic Island"].into_iter().enumerate() {
                        let active = s.dynamic_island == (i == 1);
                        let b = (X + 16. + i as f32 * 230., 209., 220., 31.);
                        let dy = self.settings_button(c, b, active);
                        c.center(name, b.0, b.1 + 8. + dy, b.2, 11., active, INK);
                        self.hit(
                            b.0 as i32,
                            b.1 as i32,
                            b.2 as i32,
                            b.3 as i32,
                            Action::Setting(19, if i == 1 { 1 } else { -1 }),
                        );
                    }
                    self.settings_caption(c, i18n::tr("UKURAN PANEL"), 259.);
                    self.settings_card(c, 276., 141.);
                    for y in [323., 370.] {
                        c.rect(X + 16., y, W - 32., 1., 0., BORDER);
                    }
                    self.settings_value_row(
                        c,
                        i18n::tr("Lebar panel"),
                        &format!("{} px", s.width),
                        0,
                        20,
                        276,
                    );
                    self.settings_value_row(
                        c,
                        i18n::tr("Lengkung panel"),
                        &format!("{} px", expanded_radius(s) as i32),
                        2,
                        2,
                        323,
                    );
                    self.settings_value_row(
                        c,
                        i18n::tr("Opasitas semua"),
                        &if (0..OPACITY_SURFACES).all(|i| s.opacity_for(i) == s.opacity) {
                            format!("{}%", (s.opacity as f32 / 255. * 100.).round())
                        } else {
                            i18n::tr("Beragam").to_string()
                        },
                        1,
                        13,
                        370,
                    );
                    self.settings_caption(c, i18n::tr("PALET DASAR"), 432.);
                    for (i, (name, swatch)) in [
                        ("Titanium", 0x9ea8b5),
                        ("Forest", 0x88a894),
                        ("Warm", 0xbea28b),
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        let b = (X + i as f32 * 162., 454., 150., 39.);
                        let dy = self.settings_button(c, b, s.theme == i);
                        c.ellipse(b.0 + 14., 466. + dy, 15., 15., swatch);
                        c.text(name, b.0 + 39., 467. + dy, 88., 11., s.theme == i, INK);
                        self.hit(
                            b.0 as i32,
                            454,
                            150,
                            39,
                            Action::Setting(3, i as i32 - s.theme as i32),
                        );
                    }
                }
                1 => {
                    self.settings_card(c, 110., 180.);
                    self.settings_switch_row(
                        c,
                        i18n::tr("Animasi & interaksi"),
                        i18n::tr("Spring, transisi halaman, dan refleksi saat hover."),
                        4,
                        s.motion,
                        110,
                    );
                    self.settings_divider(c, 170.);
                    self.settings_switch_row(
                        c,
                        i18n::tr("Mulai bersama Windows"),
                        i18n::tr("Horizon tersedia otomatis setelah Anda masuk."),
                        5,
                        s.startup,
                        170,
                    );
                    self.settings_divider(c, 230.);
                    self.settings_switch_row(
                        c,
                        i18n::tr("Tampil di atas taskbar"),
                        i18n::tr("Semua ukuran menimpa taskbar di tepi atas layar."),
                        17,
                        s.taskbar_layout,
                        230,
                    );
                    self.settings_caption(c, i18n::tr("Bahasa / Language"), 313.);
                    self.settings_card(c, 334., 52.);
                    for (i, name) in ["Bahasa Indonesia", "English"].into_iter().enumerate() {
                        let selected = (s.language == i18n::Language::English) == (i == 1);
                        let b = (216 + i as i32 * 232, 344, 226, 32);
                        self.settings_action(
                            c,
                            name,
                            b,
                            Action::Setting(20, if i == 1 { 1 } else { -1 }),
                            selected,
                        );
                    }
                    self.settings_caption(c, i18n::tr("AKSES CEPAT"), 412.);
                    c.text("Ctrl + Alt + H", X + 18., 439., 154., 11., true, INK);
                    c.text(
                        i18n::tr("Buka / ringkas panel"),
                        X + 200.,
                        439.,
                        247.,
                        11.,
                        false,
                        MUTED_TEXT,
                    );
                    c.text(
                        i18n::tr("Klik kanan panel"),
                        X + 18.,
                        467.,
                        166.,
                        11.,
                        true,
                        INK,
                    );
                    c.text(
                        i18n::tr("Buka pengaturan"),
                        X + 200.,
                        467.,
                        247.,
                        11.,
                        false,
                        MUTED_TEXT,
                    );
                }
                2 => {
                    self.settings_card(c, 110., 113.);
                    c.icon("timer", X + 20., 127., 17., INK);
                    c.text(
                        i18n::tr("Sesi saat ini"),
                        X + 46.,
                        129.,
                        340.,
                        11.,
                        true,
                        MUTED_TEXT,
                    );
                    let left = remaining(state);
                    c.text(
                        &format!("{:02}:{:02}", left / 60, left % 60),
                        X + 20.,
                        154.,
                        240.,
                        38.,
                        true,
                        INK,
                    );
                    self.settings_caption(c, i18n::tr("Durasi sesi"), 247.);
                    self.settings_card(c, 267., 125.);
                    self.settings_value_row(
                        c,
                        i18n::tr("Durasi sesi"),
                        &i18n::format!("{} menit", "{} min", s.focus_minutes),
                        6,
                        5,
                        267,
                    );
                    self.settings_divider(c, 315.);
                    c.text(
                        i18n::tr("DURASI CEPAT"),
                        X + 16.,
                        328.,
                        W - 32.,
                        9.,
                        true,
                        MUTED_TEXT,
                    );
                    for (i, minutes) in [5, 15, 25, 50].into_iter().enumerate() {
                        self.settings_action(
                            c,
                            &i18n::format!("{minutes} menit", "{minutes} min"),
                            (220 + i as i32 * 115, 347, 105, 32),
                            Action::Setting(6, minutes - s.focus_minutes as i32),
                            s.focus_minutes as i32 == minutes,
                        );
                    }
                    c.text(
                        i18n::tr("Waktu sesi berjalan tetap dipertahankan."),
                        X + 2.,
                        412.,
                        W,
                        10.5,
                        false,
                        MUTED_TEXT,
                    );
                }
                3 => {
                    self.settings_caption(c, i18n::tr("HIDRASI"), 104.);
                    self.settings_card(c, 124., 96.);
                    self.settings_value_row(
                        c,
                        i18n::tr("Target harian"),
                        &format!("{} ml", s.water_goal),
                        7,
                        250,
                        124,
                    );
                    self.settings_divider(c, 172.);
                    self.settings_value_row(
                        c,
                        i18n::tr("Ukuran gelas"),
                        &format!("{} ml", s.cup),
                        8,
                        50,
                        172,
                    );
                    self.settings_caption(c, i18n::tr("Pengingat minum"), 243.);
                    self.settings_card(c, 264., 204.);
                    self.settings_switch_row(
                        c,
                        i18n::tr("Pengingat minum"),
                        i18n::tr("Jadwal melewati tengah malam tetap didukung."),
                        11,
                        s.water_enabled,
                        264,
                    );
                    for y in [324., 372., 420.] {
                        self.settings_divider(c, y);
                    }
                    self.settings_value_row(
                        c,
                        i18n::tr("Interval minum"),
                        &i18n::format!("{} menit", "{} min", s.water_interval),
                        9,
                        1,
                        324,
                    );
                    self.settings_value_row(
                        c,
                        i18n::tr("Jam mulai"),
                        &format!("{:02}:00", s.water_start_hour),
                        12,
                        1,
                        372,
                    );
                    self.settings_value_row(
                        c,
                        i18n::tr("Jam selesai"),
                        &format!("{:02}:00", s.water_end_hour),
                        13,
                        1,
                        420,
                    );
                }
                4 => {
                    self.settings_card(c, 110., 180.);
                    self.settings_switch_row(
                        c,
                        i18n::tr("Warna dari sampul"),
                        i18n::tr("Palet dan glow mengikuti warna cover lagu."),
                        15,
                        s.album_colors,
                        110,
                    );
                    self.settings_divider(c, 170.);
                    self.settings_switch_row(
                        c,
                        i18n::tr("Lirik sinkron"),
                        i18n::tr("Karaoke serta lirik berjalan di panel kecil/sedang."),
                        16,
                        s.show_lyrics,
                        170,
                    );
                    self.settings_divider(c, 230.);
                    self.settings_switch_row(
                        c,
                        i18n::tr("Cover cadangan (Apple)"),
                        i18n::tr("Judul dan artis dikirim saat cover Windows kosong."),
                        18,
                        s.cover_fallback,
                        230,
                    );
                    self.settings_caption(c, i18n::tr("Musik & sumber konten"), 315.);
                    c.text(
                        i18n::tr("Kontrol media mengikuti sesi Windows."),
                        X + 2.,
                        340.,
                        W - 36.,
                        11.,
                        false,
                        MUTED_TEXT,
                    );
                    c.text(
                        i18n::tr("Lirik: LRCLIB. Cover cadangan: Apple Music."),
                        X + 2.,
                        366.,
                        W - 36.,
                        11.,
                        false,
                        MUTED_TEXT,
                    );
                }
                5 => {
                    self.settings_card(c, 110., 108.);
                    self.settings_switch_row(
                        c,
                        i18n::tr("Suara pengingat"),
                        i18n::tr("Untuk fokus, tugas, kalender, dan minum air."),
                        10,
                        s.sound,
                        110,
                    );
                    self.settings_divider(c, 170.);
                    self.settings_value_row(
                        c,
                        i18n::tr("Sebelum acara"),
                        &i18n::format!("{} menit", "{} min", s.event_lead_minutes),
                        14,
                        1,
                        170,
                    );
                    self.settings_card(c, 278., 150.);
                    c.text(
                        i18n::tr("Dengarkan nada pengingat"),
                        X + 18.,
                        299.,
                        W - 36.,
                        13.,
                        true,
                        INK,
                    );
                    c.text(
                        i18n::tr("Mengikuti volume dan perangkat audio Windows."),
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
                            i18n::tr("Putar nada uji"),
                            (230, 365, 154, 40),
                            Action::TestSound,
                            false,
                        );
                    } else {
                        c.text(
                            i18n::tr("Aktifkan suara untuk mencoba nada."),
                            X + 18.,
                            375.,
                            W - 36.,
                            11.,
                            false,
                            MUTED_TEXT,
                        );
                    }
                }
                6 => {
                    let right = X + 247.;
                    let row_height = 45.;
                    for (x, name, rows) in [(X, "HALAMAN", 8.), (right, "MODE PANEL", 3.)] {
                        c.text(i18n::tr(name), x + 2., 105., 230., 10., true, MUTED_TEXT);
                        c.rect(x, 125., 235., rows * row_height, 12., CARD);
                        c.outline(x, 125., 235., rows * row_height, 12., BORDER);
                    }
                    for (i, row, x, name) in [
                        (MONITOR_OPACITY, 0, X, "Monitor"),
                        (0, 1, X, "Musik"),
                        (1, 2, X, "Fokus"),
                        (2, 3, X, "Kalender"),
                        (3, 4, X, "Air"),
                        (4, 5, X, "Tugas"),
                        (5, 6, X, "Berkas"),
                        (ASSISTANT_OPACITY, 7, X, "Asisten"),
                        (6, 0, right, "Idle kecil"),
                        (7, 1, right, "Idle sedang"),
                        (8, 2, right, "Pengaturan"),
                    ] {
                        let y = 125. + row as f32 * row_height;
                        if row > 0 {
                            c.rect(x + 12., y, 211., 1., 0., BORDER);
                        }
                        c.text(i18n::tr(name), x + 12., y + 15., 102., 11., true, INK);
                        self.settings_stepper(
                            c,
                            (x + 116., y + 7., 107., 30.),
                            &format!("{}%", (f32::from(s.opacity_for(i)) / 255. * 100.).round()),
                            OPACITY_SETTING_START + i,
                            13,
                        );
                    }
                }
                assistant::SETTINGS_PAGE => self.draw_assistant_settings(c, s),
                _ => {
                    self.settings_card(c, 110., 100.);
                    c.rect(X + 16., 133., 76., 44., 15., BACKGROUND);
                    self.draw_companion(c, (X + 16., 133., 76., 44.), false);
                    c.text("Horizon", X + 110., 128., 300., 23., true, INK);
                    c.text(
                        &i18n::format!("Versi {}", "Version {}", env!("CARGO_PKG_VERSION")),
                        X + 111.,
                        161.,
                        300.,
                        11.,
                        false,
                        MUTED_TEXT,
                    );
                    self.settings_caption(
                        c,
                        i18n::tr("Ruang ringkas untuk ritme hari Anda."),
                        235.,
                    );
                    self.settings_card(c, 256., 126.);
                    for (i, text) in [
                        i18n::tr("Pengaturan, tugas, dan jadwal tersimpan lokal."),
                        i18n::tr("Kontrol media terhubung ke sesi Windows."),
                        i18n::tr("Lirik dimuat melalui layanan LRCLIB."),
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        let y = 256. + i as f32 * 42.;
                        if i > 0 {
                            self.settings_divider(c, y);
                        }
                        c.icon("check", X + 16., y + 14., 14., INK);
                        c.text(text, X + 43., y + 14., 398., 11., false, MUTED_TEXT);
                    }
                    self.settings_action(
                        c,
                        i18n::tr("Keluar dari Horizon"),
                        (204, 413, 180, 38),
                        Action::Exit,
                        false,
                    );
                }
            }
            c.rect(X, 495., W, 1., 0., BORDER);
            let failed = state.status.contains("belum") || state.status.contains("gagal");
            let footer = if failed {
                state.status.as_str()
            } else {
                i18n::tr("Perubahan tersimpan otomatis di perangkat ini.")
            };
            c.text(
                &i18n::message(footer),
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
