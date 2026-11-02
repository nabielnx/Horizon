use crate::*;
use chrono::{Datelike, NaiveDate};

pub const MONTHS: [&str; 12] = [
    "Januari",
    "Februari",
    "Maret",
    "April",
    "Mei",
    "Juni",
    "Juli",
    "Agustus",
    "September",
    "Oktober",
    "November",
    "Desember",
];
pub const MONTHS_SHORT: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "Mei", "Jun", "Jul", "Ags", "Sep", "Okt", "Nov", "Des",
];
pub const DAYS_SHORT: [&str; 7] = ["Sen", "Sel", "Rab", "Kam", "Jum", "Sab", "Min"];
pub const DAYS_FULL: [&str; 7] = [
    "Senin", "Selasa", "Rabu", "Kamis", "Jumat", "Sabtu", "Minggu",
];
pub struct Calendar {
    pub month: NaiveDate,
    pub selected: NaiveDate,
    pub scroll: f32,
    pub target_scroll: f32,
    pub content_height: f32,
    pub form: bool,
    pub form_y: f32,
    pub status: String,
    pub editing: Option<usize>,
    pub agenda_offset: usize,
}
impl Default for Calendar {
    fn default() -> Self {
        let today = Local::now().date_naive();
        Self {
            month: today.with_day(1).unwrap(),
            selected: today,
            scroll: 0.,
            target_scroll: 0.,
            content_height: 360.,
            form: false,
            form_y: 0.,
            status: String::new(),
            editing: None,
            agenda_offset: 0,
        }
    }
}
impl Calendar {
    pub fn days(&self) -> [NaiveDate; 42] {
        let start =
            self.month - chrono::Duration::days(self.month.weekday().num_days_from_monday() as i64);
        std::array::from_fn(|i| start + chrono::Duration::days(i as i64))
    }
    pub fn move_month(&mut self, delta: i32) {
        let next = if delta < 0 {
            self.month
                .checked_sub_months(chrono::Months::new(delta.unsigned_abs()))
        } else {
            self.month
                .checked_add_months(chrono::Months::new(delta as u32))
        };
        if let Some(month) = next.filter(|d| (1900..=9998).contains(&d.year())) {
            self.month = month;
            self.scroll = 0.;
            self.target_scroll = 0.;
        }
    }
    pub fn select(&mut self, date: NaiveDate) {
        self.agenda_offset = 0;
        self.selected = date;
        self.month = date.with_day(1).unwrap();
    }
    pub fn wheel(&mut self, delta: f32, viewport: f32) {
        self.target_scroll =
            (self.target_scroll + delta).clamp(0., (self.content_height - viewport).max(0.));
    }
    pub fn active(&self) -> bool {
        (self.scroll - self.target_scroll).abs() > 0.2
    }
    pub fn step(&mut self, seconds: f32) {
        self.scroll += (self.target_scroll - self.scroll) * (1. - (-22. * seconds).exp());
        if !self.active() {
            self.scroll = self.target_scroll;
        }
    }
}

#[allow(dead_code)]
pub fn make_event(
    date: NaiveDate,
    title: &str,
    time: &str,
    lead: &str,
    url: &str,
    now: i64,
) -> std::result::Result<model::Event, String> {
    make_event_for_edit(date, title, time, lead, url, None, now)
}

pub fn make_event_for_edit(
    date: NaiveDate,
    title: &str,
    time: &str,
    lead: &str,
    url: &str,
    existing_at: Option<i64>,
    now: i64,
) -> std::result::Result<model::Event, String> {
    if title.trim().is_empty() {
        return Err("Isi judul acara dulu.".into());
    }
    let time = chrono::NaiveTime::parse_from_str(time.trim(), "%H:%M")
        .map_err(|_| "Isi jam dalam format HH:mm.".to_string())?;
    let at = Local
        .from_local_datetime(&date.and_time(time))
        .single()
        .ok_or("Jam ini tidak tersedia pada zona waktu Windows.")?
        .timestamp();
    if Some(at) != existing_at && at <= now {
        return Err("Pilih waktu acara yang belum lewat.".into());
    }
    let lead = lead
        .trim()
        .parse::<u32>()
        .ok()
        .filter(|v| *v <= 1440)
        .ok_or("Isi pengingat 0–1440 menit.")?;
    if !url.trim().is_empty() && !safe_url(url.trim()) {
        return Err("Gunakan link http:// atau https:// yang valid.".into());
    }
    Ok(model::Event {
        title: title.trim().chars().take(200).collect(),
        at,
        notified: false,
        lead_minutes: Some(lead),
        url: url.trim().into(),
    })
}
pub fn safe_url(url: &str) -> bool {
    if url
        .chars()
        .any(|c| c.is_whitespace() || c.is_control() || c == '"')
    {
        return false;
    }
    let Ok(uri) = windows::Foundation::Uri::CreateUri(&HSTRING::from(url)) else {
        return false;
    };
    matches!(
        uri.SchemeName()
            .ok()
            .as_ref()
            .map(|s| s.to_string())
            .as_deref(),
        Some("https" | "http")
    ) && uri.UserName().is_ok_and(|v| v.is_empty())
        && uri.Host().is_ok_and(|v| !v.is_empty())
}
#[link(name = "shell32")]
unsafe extern "system" {
    fn ShellExecuteW(
        hwnd: HWND,
        operation: PCWSTR,
        file: PCWSTR,
        parameters: PCWSTR,
        directory: PCWSTR,
        show: i32,
    ) -> HINSTANCE;
}
pub unsafe fn open_url(hwnd: HWND, url: &str) -> bool {
    if !safe_url(url) {
        return false;
    }
    unsafe {
        ShellExecuteW(
            hwnd,
            w!("open"),
            PCWSTR(HSTRING::from(url).as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            1,
        )
        .0 as isize
            > 32
    }
}
pub unsafe fn open_file(hwnd: HWND, path: &str) -> bool {
    if !crate::shelf::local_path(path) {
        return false;
    }
    let path = HSTRING::from(path);
    unsafe {
        ShellExecuteW(
            hwnd,
            w!("open"),
            PCWSTR(path.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            1,
        )
        .0 as isize
            > 32
    }
}
impl Window {
    fn calendar_hit(&mut self, x: f32, y: f32, width: f32, height: f32, action: Action) {
        let y = y - self.calendar.scroll;
        let top = y.max(22.);
        let bottom =
            (y + height).min(panel_height(&self.shared.borrow().data.settings) as f32 - 12.);
        if bottom > top {
            self.hit(
                x as i32,
                top.ceil() as i32,
                width as i32,
                (bottom - top).floor() as i32,
                action,
            );
        }
    }
    pub(super) unsafe fn draw_calendar(&mut self, c: &Canvas, state: &Shared) {
        unsafe {
            let s = &state.data.settings;
            let x = 70.;
            let width = (s.width - 88) as f32;
            let viewport = panel_height(s) as f32 - 34.;
            c.clip_scroll(
                62.,
                22.,
                (s.width - 63) as f32,
                viewport,
                self.calendar.scroll,
            );
            let grid_w = 238.0;
            let narrow = width < 434.;
            let month_name = MONTHS[self.calendar.month.month0() as usize];
            let month_title = format!("{} {}", month_name, self.calendar.month.year());
            let month_size = if c.measure_text(&month_title, 15., true) > 114. {
                12.5
            } else {
                15.
            };
            c.text(&month_title, x, 29., 114., month_size, true, INK);
            for (label, icon, xx, ww, a) in [
                ("", "chevron-left", x + 116., 24., Action::Month(-1)),
                ("Hari ini", "", x + 144., 66., Action::Today),
                ("", "chevron-right", x + 214., 24., Action::Month(1)),
            ] {
                let sy = 26. - self.calendar.scroll;
                let hover = self.pointer.0 >= xx as i32
                    && self.pointer.0 < (xx + ww) as i32
                    && self.pointer.1 >= sy as i32
                    && self.pointer.1 < (sy + 24.) as i32;
                let offset = c.control_glass(xx, 26., ww, 24., 6., Some(0x818cf8), hover, false);
                if icon.is_empty() {
                    c.center(
                        label,
                        xx,
                        31. + offset,
                        ww,
                        10.5,
                        true,
                        if hover { 0xffffff } else { 0xa5b4fc },
                    );
                } else {
                    c.icon(
                        icon,
                        xx + 5.,
                        31. + offset,
                        14.,
                        if hover { 0xffffff } else { 0xa5b4fc },
                    );
                }
                self.calendar_hit(xx, 26., ww, 24., a);
            }
            let cell_w = grid_w / 7.;
            for (i, day) in DAYS_SHORT.iter().enumerate() {
                let color = if i >= 5 { 0x8b97a5 } else { 0x55606d };
                c.center(day, x + i as f32 * cell_w, 55., cell_w, 10., false, color);
            }
            for (i, date) in self.calendar.days().into_iter().enumerate() {
                let col = i % 7;
                let row = i / 7;
                let xx = x + col as f32 * cell_w;
                let yy = 70. + row as f32 * 22.;
                let cell_h = 22.;
                let selected = date == self.calendar.selected;
                let today = date == Local::now().date_naive();
                let outside = date.month() != self.calendar.month.month();
                let box_x = xx + 2.;
                let box_y = yy + 1.;
                let box_w = cell_w - 4.;
                let box_h = cell_h - 2.;
                let hover = self.pointer.0 >= xx as i32
                    && self.pointer.0 < (xx + cell_w) as i32
                    && self.pointer.1 >= (yy - self.calendar.scroll) as i32
                    && self.pointer.1 < (yy + cell_h - self.calendar.scroll) as i32;
                let bounds = (box_x, box_y, box_w, box_h);
                let light = c.hover_amount(bounds, hover);
                let selection = c.animate_value(bounds, 2, f32::from(selected), motion::STATE);
                if selection > 0.001 {
                    c.rect(
                        box_x,
                        box_y,
                        box_w,
                        box_h,
                        6.,
                        motion::mix(0x181b24, 0x6352b0, selection),
                    );
                    c.outline(
                        box_x,
                        box_y,
                        box_w,
                        box_h,
                        6.,
                        motion::mix(0x2b2f3c, 0xada0eb, selection),
                    );
                } else if light > 0.001 {
                    c.rect(
                        box_x,
                        box_y,
                        box_w,
                        box_h,
                        6.,
                        motion::mix(0x11131a, 0x312b46, light),
                    );
                }
                if today && !selected {
                    c.outline(box_x, box_y, box_w, box_h, 6., 0x818cf8);
                }
                if today {
                    c.rect(
                        xx + cell_w / 2. - 3.,
                        yy + 17.5,
                        6.,
                        1.5,
                        0.75,
                        if selected { 0xffffff } else { 0xa5b4fc },
                    );
                }
                let ink = if selected {
                    0xffffff
                } else if today {
                    0xa5b4fc
                } else if outside {
                    0x38414a
                } else if col >= 5 {
                    0x9aa8b8
                } else {
                    0xcdd6df
                };
                c.center(
                    &date.day().to_string(),
                    xx,
                    yy + 3.5,
                    cell_w,
                    10.5,
                    selected,
                    ink,
                );
                if state.data.events.iter().any(|e| {
                    Local
                        .timestamp_opt(e.at, 0)
                        .single()
                        .is_some_and(|d| d.date_naive() == date)
                }) {
                    c.ellipse(xx + cell_w - 9., yy + 3., 3., 3., 0xffffff);
                }
                self.calendar_hit(
                    xx,
                    yy,
                    cell_w,
                    cell_h,
                    Action::Date(date.num_days_from_ce()),
                );
            }

            let div_x = x + grid_w + 8.;
            if !narrow {
                c.rect(div_x, 26., 1., 176., 0., 0x1b1f26);
            } else {
                c.control_glass(x, 205., width, 22., 6., Some(0x818cf8), false, false);
                c.center("Agenda ↓", x, 209., width, 10., true, 0xa5b4fc);
                self.calendar_hit(x, 205., width, 22., Action::CalendarAgenda);
            }

            let right_x = if narrow { x } else { x + grid_w + 16. };
            let right_w = if narrow { width } else { width - grid_w - 16. };
            let card_y = if narrow { 232. } else { 26. };
            let card_h = 176.;
            c.rect(right_x, card_y, right_w, card_h, 9., 0x1a1824);
            c.outline(right_x, card_y, right_w, card_h, 9., 0x1e232b);
            c.rect(
                right_x + 8.,
                card_y + 1.,
                right_w - 16.,
                1.,
                0.5,
                0x18ffffff,
            );

            let sel = self.calendar.selected;
            let weekday_name = DAYS_SHORT[sel.weekday().num_days_from_monday() as usize];
            let month_short = MONTHS_SHORT[sel.month0() as usize];
            let date_label = format!("{}, {} {}", weekday_name, sel.day(), month_short);
            c.text(
                &date_label,
                right_x + 12.,
                card_y + 10.,
                right_w - 74.,
                12.,
                true,
                0xffffff,
            );

            let btn_w = 56.;
            let btn_h = 22.;
            let btn_x = right_x + right_w - btn_w - 8.;
            let btn_y = card_y + 7.;
            let sy = btn_y - self.calendar.scroll;
            let hover = self.pointer.0 >= btn_x as i32
                && self.pointer.0 < (btn_x + btn_w) as i32
                && self.pointer.1 >= sy as i32
                && self.pointer.1 < (sy + btn_h) as i32;
            c.control_glass(
                btn_x,
                btn_y,
                btn_w,
                btn_h,
                6.,
                Some(0x818cf8),
                hover,
                self.calendar.form,
            );
            c.center(
                "+ Acara",
                btn_x,
                btn_y + 4.5,
                btn_w,
                9.5,
                true,
                if self.calendar.form || hover {
                    0xffffff
                } else {
                    0xa5b4fc
                },
            );
            self.calendar_hit(btn_x, btn_y, btn_w, btn_h, Action::EventForm);

            c.rect(right_x + 8., card_y + 35., right_w - 16., 1., 0., 0x1a1e25);

            let mut events: Vec<_> = state
                .data
                .events
                .iter()
                .enumerate()
                .filter(|(_, e)| {
                    Local
                        .timestamp_opt(e.at, 0)
                        .single()
                        .is_some_and(|d| d.date_naive() == self.calendar.selected)
                })
                .collect();
            events.sort_by_key(|(_, e)| e.at);

            self.calendar.agenda_offset = self
                .calendar
                .agenda_offset
                .min(events.len().saturating_sub(1) / 3 * 3);
            if events.is_empty() {
                c.icon(
                    "calendar-days",
                    right_x + right_w / 2. - 9.,
                    card_y + 64.,
                    18.,
                    0x526072,
                );
                c.center(
                    "Belum ada acara",
                    right_x + 8.,
                    card_y + 92.,
                    right_w - 16.,
                    10.5,
                    true,
                    0x64748b,
                );
                c.center(
                    "",
                    right_x + 8.,
                    card_y + 110.,
                    right_w - 16.,
                    9.0,
                    false,
                    0x404854,
                );
            } else {
                for (list_idx, &(index, event)) in events
                    .iter()
                    .skip(self.calendar.agenda_offset)
                    .take(3)
                    .enumerate()
                {
                    let ey = card_y + 42. + list_idx as f32 * 42.;
                    let is_editing_this = self.calendar.editing == Some(index);
                    let time = Local
                        .timestamp_opt(event.at, 0)
                        .single()
                        .map(|d| d.format("%H:%M").to_string())
                        .unwrap_or_default();

                    if is_editing_this {
                        c.control_glass(
                            right_x + 6.,
                            ey - 2.,
                            right_w - 12.,
                            38.,
                            6.,
                            Some(0x818cf8),
                            false,
                            true,
                        );
                    }

                    let time_ink = if is_editing_this { 0xa5b4fc } else { 0x818cf8 };
                    c.text(&time, right_x + 10., ey, 36., 11., true, time_ink);
                    let title_w = (right_w - 112.).max(40.);
                    c.text(
                        &event.title,
                        right_x + 48.,
                        ey,
                        title_w,
                        11.,
                        true,
                        if is_editing_this { 0xffffff } else { INK },
                    );
                    let lead_mins = event.lead_minutes.unwrap_or(s.event_lead_minutes);
                    c.text(
                        &format!("{} mnt sebelumnya", lead_mins),
                        right_x + 48.,
                        ey + 18.,
                        title_w,
                        9.0,
                        false,
                        MUTED,
                    );
                    self.calendar_hit(
                        right_x + 8.,
                        ey,
                        title_w + 40.,
                        32.,
                        Action::EditEvent(index),
                    );

                    let mut icon_x = right_x + right_w - 22.;
                    c.icon("x", icon_x, ey + 4., 14., 0x64748b);
                    self.calendar_hit(icon_x - 4., ey, 22., 26., Action::DeleteEvent(index));
                    if !event.url.is_empty() {
                        icon_x -= 22.;
                        c.icon("external-link", icon_x, ey + 4., 14., 0x64748b);
                        self.calendar_hit(icon_x - 4., ey, 22., 26., Action::EventLink(index));
                    }
                    icon_x -= 22.;
                    c.icon(
                        "pencil",
                        icon_x,
                        ey + 4.,
                        13.,
                        if is_editing_this { 0xa5b4fc } else { 0x64748b },
                    );
                    self.calendar_hit(icon_x - 4., ey, 22., 26., Action::EditEvent(index));

                    if list_idx < 2 && list_idx + 1 < events.len() && !is_editing_this {
                        c.rect(right_x + 10., ey + 39., right_w - 20., 1., 0., 0x161a20);
                    }
                }
                if events.len() > 3 {
                    c.center(
                        &format!(
                            "{}–{} / {}",
                            self.calendar.agenda_offset + 1,
                            (self.calendar.agenda_offset + 3).min(events.len()),
                            events.len()
                        ),
                        right_x + 35.,
                        card_y + 158.,
                        right_w - 70.,
                        9.,
                        false,
                        MUTED,
                    );
                    for (xx, icon, delta) in [
                        (right_x + 8., "chevron-left", -1),
                        (right_x + right_w - 30., "chevron-right", 1),
                    ] {
                        c.icon(icon, xx + 4., card_y + 158., 12., 0xa5b4fc);
                        self.calendar_hit(xx, card_y + 154., 22., 22., Action::AgendaPage(delta));
                    }
                }
            }

            let mut y = if narrow { 426. } else { 224. };
            c.icon(
                if self.calendar.form {
                    "chevron-down"
                } else {
                    "chevron-right"
                },
                x,
                y + 5.,
                16.,
                MUTED,
            );
            let form_header = if self.calendar.editing.is_some() {
                "Edit acara"
            } else {
                "Tambah acara"
            };
            c.text(
                form_header,
                x + 26.,
                y + 4.,
                width - 26.,
                12.,
                true,
                if self.calendar.editing.is_some() {
                    0xa5b4fc
                } else {
                    INK
                },
            );
            self.calendar_hit(x, y - 3., width, 36., Action::EventForm);
            y += 42.;
            self.calendar.form_y = y;
            if self.calendar.form {
                c.text("Judul acara", x, y, width, 10.5, false, MUTED);
                c.text(
                    "Pukul (HH:mm)",
                    x,
                    y + 76.,
                    width / 2. - 8.,
                    10.5,
                    false,
                    MUTED,
                );
                c.text(
                    "Ingatkan (menit)",
                    x + width / 2. + 8.,
                    y + 76.,
                    width / 2. - 8.,
                    10.5,
                    false,
                    MUTED,
                );
                c.text(
                    "Link acara (opsional)",
                    x,
                    y + 152.,
                    width,
                    10.5,
                    false,
                    MUTED,
                );
                let is_editing = self.calendar.editing.is_some();
                let submit_label = if is_editing {
                    "Perbarui acara"
                } else {
                    "Simpan acara"
                };
                let submit_w = if is_editing { 148. } else { 137. };
                let btn_x = x + width - submit_w;
                let btn_y = y + 228.;
                let sy = btn_y - self.calendar.scroll;
                let hover = self.pointer.0 >= btn_x as i32
                    && self.pointer.0 < (btn_x + submit_w) as i32
                    && self.pointer.1 >= sy as i32
                    && self.pointer.1 < (sy + 36.) as i32;
                c.control_glass(btn_x, btn_y, submit_w, 36., 9., Some(0x818cf8), hover, true);
                c.center(
                    submit_label,
                    btn_x,
                    btn_y + 8.5,
                    submit_w,
                    11.5,
                    true,
                    0xffffff,
                );
                self.calendar_hit(btn_x, btn_y, submit_w, 36., Action::AddEvent);

                if is_editing {
                    let cancel_w = 76.;
                    let cancel_x = btn_x - cancel_w - 10.;
                    let cancel_hover = self.pointer.0 >= cancel_x as i32
                        && self.pointer.0 < (cancel_x + cancel_w) as i32
                        && self.pointer.1 >= sy as i32
                        && self.pointer.1 < (sy + 36.) as i32;
                    c.control_glass(
                        cancel_x,
                        btn_y,
                        cancel_w,
                        36.,
                        9.,
                        None,
                        cancel_hover,
                        false,
                    );
                    c.center(
                        "Batal",
                        cancel_x,
                        btn_y + 8.5,
                        cancel_w,
                        11.5,
                        true,
                        if cancel_hover { 0xffffff } else { 0xe2e8f0 },
                    );
                    self.calendar_hit(cancel_x, btn_y, cancel_w, 36., Action::CancelEditEvent);
                }

                c.text(
                    &self.calendar.status,
                    x,
                    y + 276.,
                    width,
                    10.5,
                    false,
                    MUTED,
                );
                y += 306.;
            }
            self.calendar.content_height = y + 12.;
            self.calendar.target_scroll = self
                .calendar
                .target_scroll
                .min((self.calendar.content_height - viewport).max(0.));
            c.end_clip();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn month_grid_includes_adjacent_dates_and_leap_day() {
        let mut c = Calendar {
            month: NaiveDate::from_ymd_opt(2024, 2, 1).unwrap(),
            ..Default::default()
        };
        let days = c.days();
        assert_eq!(days[0], NaiveDate::from_ymd_opt(2024, 1, 29).unwrap());
        assert_eq!(days[0].weekday(), chrono::Weekday::Mon);
        assert_eq!(days.len(), 42);
        assert!(days.contains(&NaiveDate::from_ymd_opt(2024, 2, 29).unwrap()));
        c.move_month(-2);
        assert_eq!(c.month, NaiveDate::from_ymd_opt(2023, 12, 1).unwrap());
        c.select(days[0]);
        assert_eq!(c.month.month(), 1);
    }
    #[test]
    fn event_validation_and_legacy_fields() {
        let date = NaiveDate::from_ymd_opt(2030, 5, 10).unwrap();
        let e = make_event(
            date,
            " Meeting ",
            "09:30",
            "10",
            "https://example.com/meet",
            0,
        )
        .unwrap();
        assert_eq!(e.title, "Meeting");
        assert_eq!(e.lead_minutes, Some(10));
        assert!(make_event(date, "A", "25:00", "5", "", 0).is_err());
        assert!(make_event(date, "A", "09:00", "1441", "", 0).is_err());
        assert!(!safe_url("file:///C:/secret"));
        assert!(!safe_url("https://user:pass@example.com"));
        let old: model::Event =
            serde_json::from_str(r#"{"title":"Legacy","at":10,"notified":false}"#).unwrap();
        assert_eq!(old.lead_minutes, None);
        assert!(old.url.is_empty());
    }
}
