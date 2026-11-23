//! Native implementation of the approved Figma frames. Positions are relative
//! to the 591px notch, excluding the grey artboard surrounding the design.
use crate::*;
use chrono::Datelike;

const AGENDA_INK: u32 = 0x201431;
const AGENDA_MUTED: u32 = 0x493359;
const FOCUS_ACCENT: u32 = 0xff9317;

pub(super) const SIDEBAR_ICONS: [(usize, &str); 8] = [
    (monitor::PAGE, "monitor-pulse"),
    (0, "music-notes"),
    (1, "clock"),
    (2, "calendar-dots"),
    (3, "drop"),
    (4, "list-checks"),
    (5, "files"),
    (assistant::PAGE, "sparkles"),
];

pub(super) fn sidebar_index(page: usize) -> usize {
    SIDEBAR_ICONS
        .iter()
        .position(|&(id, _)| id == page)
        .unwrap_or(0)
}

fn cover_wave_tints(track: &media::Track) -> [u32; 2] {
    let restore = |tone: u32, exposure: f32| {
        let channels = [16, 8, 0]
            .map(|shift| ((((tone >> shift) & 255) as f32 - 8.) / exposure).clamp(0., 255.));
        let gain = (150. / channels.into_iter().fold(1_f32, f32::max)).max(1.);
        channels
            .into_iter()
            .zip([16, 8, 0])
            .map(|(v, shift)| ((v * gain).min(255.) as u32) << shift)
            .sum()
    };
    track.palette.map_or([0xcbd0d7; 2], |palette| {
        [restore(palette[0], 0.34), restore(palette[2], 0.16)]
    })
}

fn idle_lyric(track: &media::Track, enabled: bool) -> &str {
    if !enabled {
        i18n::tr("Lirik nonaktif")
    } else if track.title.is_empty() {
        "Horizon"
    } else if !track.lyrics.is_empty() {
        ""
    } else {
        track.lyrics_message()
    }
}
#[derive(Default)]
pub(super) struct CompactActivity {
    owner: Option<usize>,
    playing: bool,
    focusing: bool,
    media_gap: Option<Instant>,
    last_alert: Option<(usize, String)>,
    alert_after_activity: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CompactIndicator {
    None,
    Music,
    Focus,
    Reminder,
}
impl CompactActivity {
    pub fn reset_focus(&mut self) {
        if self.owner == Some(1) {
            self.owner = None;
        }
        self.focusing = false;
    }
    pub fn indicator(
        &self,
        focusing: bool,
        track: &media::Track,
        alert: Option<usize>,
        last_page: usize,
    ) -> CompactIndicator {
        let playing = !track.title.is_empty() && track.playing;
        if alert.is_some() && (self.alert_after_activity || !focusing && !playing) {
            return CompactIndicator::Reminder;
        }
        let page = self.page(alert, focusing, playing, last_page);
        if focusing && (page == 1 || !playing) {
            return CompactIndicator::Focus;
        }
        if playing {
            CompactIndicator::Music
        } else {
            CompactIndicator::None
        }
    }
    pub fn observe_alert(&mut self, alert: Option<&Alert>) {
        let same = match (&self.last_alert, alert) {
            (Some((page, text)), Some(alert)) => *page == alert.page && *text == alert.text,
            (None, None) => true,
            _ => false,
        };
        if !same {
            self.last_alert = alert.map(|alert| (alert.page, alert.text.clone()));
            self.alert_after_activity = alert.is_some();
        }
    }
    pub fn observe_track(&mut self, track: &media::Track, focusing: bool) {
        // Providers can briefly clear metadata between automatically queued
        // songs. Such a gap is not an intentional pause/resume activation.
        let playing = if track.title.is_empty() || track.updated.is_none() && !track.playing {
            let gap = self.media_gap.get_or_insert_with(Instant::now);
            if gap.elapsed().as_secs_f32() < 1. {
                self.playing
            } else {
                false
            }
        } else {
            self.media_gap = None;
            track.playing
        };
        self.observe(playing, focusing);
    }
    pub fn observe(&mut self, playing: bool, focusing: bool) {
        if playing && !self.playing {
            self.owner = Some(0);
            self.alert_after_activity = false;
        }
        if focusing && !self.focusing {
            self.owner = Some(1);
            self.alert_after_activity = false;
        }
        self.playing = playing;
        self.focusing = focusing;
    }
    pub fn page(
        &self,
        alert: Option<usize>,
        focusing: bool,
        playing: bool,
        last_page: usize,
    ) -> usize {
        if self.alert_after_activity
            && let Some(page) = alert
        {
            return page.min(5);
        }
        match self.owner {
            Some(0) if playing => 0,
            Some(1) if focusing => 1,
            _ if focusing => 1,
            _ if playing => 0,
            _ => alert.unwrap_or(last_page).min(5),
        }
    }
}

unsafe fn mono_badge(c: &Canvas, icon: &'static str, b: (f32, f32, f32, f32)) {
    unsafe {
        c.glass_button(b, b.2 * 0.28, false);
        c.icon(
            icon,
            b.0 + b.2 * 0.12,
            b.1 + b.3 * 0.12,
            b.2 * 0.76,
            0xffffff,
        );
    }
}
unsafe fn asset(c: &Canvas, name: &'static str, b: (f32, f32, f32, f32)) {
    let bytes: &[u8] = match name {
        "nav-music" => include_bytes!("../assets/figma/nav-music.png"),
        "media-lyrics" => include_bytes!("../assets/figma/media-lyrics-glyph.png"),
        "media-previous" => include_bytes!("../assets/figma/media-previous.png"),
        "media-pause" => include_bytes!("../assets/figma/media-pause-glyph.png"),
        "media-next" => include_bytes!("../assets/figma/media-next.png"),
        "peek-pause" => include_bytes!("../assets/figma/peek-pause-glyph.png"),
        "spotify" => include_bytes!("../assets/figma/spotify.png"),
        "repeat" => include_bytes!("../assets/figma/repeat.png"),
        "fire" => include_bytes!("../assets/figma/fire.png"),
        "edit-event" => include_bytes!("../assets/figma/edit-event.png"),
        "delete-event" => include_bytes!("../assets/figma/delete-event.png"),
        _ => return,
    };
    unsafe {
        let b = if name == "nav-music" {
            b
        } else {
            c.scaled_icon_bounds(b)
        };
        let _ = c.design_asset(name, bytes, b);
    }
}
unsafe fn label(c: &Canvas, text: &str, b: (f32, f32, f32), style: (f32, u16, u32, u8)) {
    unsafe {
        c.design_text(
            text,
            (b.0, b.1, b.2, style.0 * 1.6),
            (style.0, style.1, style.3),
            style.2,
        );
    }
}

impl Window {
    pub(super) fn companion_visible(&self) -> bool {
        if self.settings {
            return self.page == 7;
        }
        let state = self.shared.borrow();
        let (playing, has_track) = state
            .media
            .as_ref()
            .and_then(|m| {
                m.snapshot
                    .lock()
                    .ok()
                    .map(|t| (t.playing, !t.title.is_empty()))
            })
            .unwrap_or_default();
        if self.expanded {
            self.page == 0 && !has_track
        } else {
            match self.mode {
                shell_policy::Mode::Compact => {
                    !playing && state.running.is_none() && state.alert.is_none()
                }
                shell_policy::Mode::Peek => !has_track,
                _ => false,
            }
        }
    }
    pub(super) unsafe fn draw_companion(
        &mut self,
        c: &Canvas,
        b: (f32, f32, f32, f32),
        design: bool,
    ) {
        unsafe {
            let (scale, offset) = if design {
                self.figma_transform()
            } else {
                (1., 0.)
            };
            let pointer = (
                (self.pointer.0 as f32 - offset) / scale,
                self.pointer.1 as f32,
            );
            let hover = pointer.0 >= b.0
                && pointer.0 < b.0 + b.2
                && pointer.1 >= b.1
                && pointer.1 < b.1 + b.3;
            let look = self.companion_pointer.map_or((0., 0.), |position| {
                mascot::cursor_look(position, (b.0 * scale + offset, b.1, b.2 * scale, b.3))
            });
            let motion = self.shared.borrow().data.settings.motion && !self.offscreen;
            let interacting = hover || !self.settings && self.hovered;
            c.presence_eyes(b, self.companion.eyes(motion, interacting, look));
            if design {
                self.figma_hit(b, Action::PetMascot);
            } else {
                self.hit(
                    b.0 as i32,
                    b.1 as i32,
                    b.2 as i32,
                    b.3 as i32,
                    Action::PetMascot,
                );
            }
        }
    }
    unsafe fn figma_lyric(
        &self,
        c: &Canvas,
        track: &media::Track,
        settings: &model::Settings,
        b: (f32, f32, f32, f32),
        style: (f32, u16, u8),
    ) {
        unsafe {
            if !settings.show_lyrics || track.lyrics.is_empty() {
                c.design_text(idle_lyric(track, settings.show_lyrics), b, style, 0xffffff);
                return;
            }
            let position = track.position_now();
            let frame = track.lyric_frame(position, settings.motion);
            c.push_design_clip(b);
            if let Some((index, fade)) = frame.outgoing {
                let line = &track.lyrics[index];
                let next = track.next_lyric_time(index);
                let (overlap, lift) = if frame.active.is_some() {
                    (0.5, 9.)
                } else {
                    (1., 0.)
                };
                c.design_karaoke(
                    line,
                    (line.playback_end(next), next),
                    (b.0, b.1 - lift - 7. * fade, b.2, b.3),
                    style,
                    true,
                    overlap * (1. - fade),
                );
            }
            if let Some(index) = frame.active {
                c.design_karaoke(
                    &track.lyrics[index],
                    (position, track.next_lyric_time(index)),
                    (b.0, b.1 + 2. * (1. - frame.entry), b.2, b.3),
                    style,
                    true,
                    0.88 + 0.12 * frame.entry,
                );
            }
            c.pop_design_clip();
        }
    }
    pub(super) fn figma_transform(&self) -> (f32, f32) {
        let width = self.client_width(&self.shared.borrow().data.settings) as f32;
        if self.expanded && self.page >= 3 {
            (1., 0.)
        } else if !self.expanded && self.mode == shell_policy::Mode::Compact {
            (1., (width - 591.) / 2.)
        } else {
            (
                (width / 591.).min(1.),
                if self.expanded {
                    0.
                } else {
                    ((width - 591.) / 2.).max(0.)
                },
            )
        }
    }
    unsafe fn figma_idle_wave(
        &mut self,
        c: &Canvas,
        state: &Shared,
        track: &media::Track,
        b: (f32, f32, f32, f32),
    ) {
        unsafe {
            let position = track.position_now();
            let progress = if track.duration > 0. {
                (position / track.duration).clamp(0., 1.) as f32
            } else {
                0.
            };
            let motion = state.data.settings.motion;
            let compact = !self.expanded && self.mode == shell_policy::Mode::Compact;
            let rail_h = if compact { 0. } else { 4. };
            let bottom = b.1 + b.3;
            let rail_center = if compact { bottom } else { bottom - 3.5 };
            c.mini_wave_tinted(
                (
                    b.0 - 2.,
                    rail_center - 5. - rail_h / 2.,
                    b.2 + 4.,
                    (b.3 - 6.).clamp(2.2, 14.),
                ),
                progress,
                if motion { position * 1.8 } else { 0. },
                motion && track.playing,
                Some(cover_wave_tints(track)),
                rail_h,
            );
            if !compact {
                let thumb = 11.;
                c.ellipse(
                    b.0 + b.2 * progress - thumb / 2.,
                    rail_center - thumb / 2.,
                    thumb,
                    thumb,
                    0xffffff,
                );
                let hit_top = b.1.min(rail_center - thumb / 2.);
                let hit_bottom = (b.1 + b.3).max(rail_center + thumb / 2.);
                if track.can_seek {
                    self.figma_hit((b.0, hit_top, b.2, hit_bottom - hit_top), Action::Seek);
                }
            }
        }
    }
    unsafe fn figma_peek_playback(&mut self, c: &Canvas, track: &media::Track) {
        unsafe {
            let b = (518., 29., 40., 39.);
            if track.title.is_empty() {
                self.draw_companion(c, (509., 27., 58., 43.), true);
                return;
            }
            let hover = self.figma_hover(b);
            let dy = c.glass_button(b, 11., hover);
            if track.playing {
                asset(c, "peek-pause", (b.0, b.1 + dy, b.2, b.3));
            } else {
                c.icon_centered("play", (b.0, b.1 + dy, b.2, b.3), 24.);
            }
            self.figma_hit(b, Action::Media(0));
        }
    }
    pub(super) fn figma_hit(&mut self, b: (f32, f32, f32, f32), action: Action) {
        let (scale, offset) = self.figma_transform();
        self.hit(
            (b.0 * scale + offset).round() as i32,
            b.1.round() as i32,
            (b.2 * scale).round() as i32,
            b.3.round() as i32,
            action,
        );
    }
    pub(super) fn figma_hover(&self, b: (f32, f32, f32, f32)) -> bool {
        let (scale, offset) = self.figma_transform();
        let p = (
            (self.pointer.0 as f32 - offset) / scale,
            self.pointer.1 as f32,
        );
        p.0 >= b.0 && p.0 < b.0 + b.2 && p.1 >= b.1 && p.1 < b.1 + b.3
    }
    unsafe fn figma_plate(
        &mut self,
        c: &Canvas,
        b: (f32, f32, f32, f32),
        radius: f32,
        action: Action,
    ) -> f32 {
        unsafe {
            let hover = self.figma_hover(b);
            let dy = c.glass_button(b, radius, hover);
            self.figma_hit(b, action);
            dy
        }
    }
    unsafe fn figma_asset_button(
        &mut self,
        c: &Canvas,
        name: &'static str,
        b: (f32, f32, f32, f32),
        action: Action,
    ) {
        unsafe {
            // Every transport shares one opaque surface. Raster assets contain
            // glyphs only, so hover never stacks a second baked-in plate/rim.
            if name.starts_with("media-") {
                let dy = self.figma_plate(c, b, 17., action);
                asset(c, name, (b.0, b.1 + dy, b.2, b.3));
                return;
            }
            let hit = if b.2 <= 18. && b.3 <= 18. {
                (b.0 - (24. - b.2) / 2., b.1 - (24. - b.3) / 2., 24., 24.)
            } else {
                b
            };
            let hover = self.figma_hover(hit);
            if matches!(action, Action::EditEvent(_) | Action::DeleteEvent(_)) {
                let plate = (hit.0 + 2., hit.1 + 2., hit.2 - 4., hit.3 - 4.);
                let dy = c.glass_button(plate, 6., hover);
                asset(c, name, (b.0, b.1 + dy, b.2, b.3));
                self.figma_hit(hit, action);
                return;
            }
            let light = c.hover_amount(b, hover);
            let press = c.animate_value(b, 1, f32::from(hover && self.pressed), motion::PRESS);
            let dy = if self.shared.borrow().data.settings.motion {
                -light + press * 2.
            } else {
                0.
            };
            if matches!(name, "calendar-back" | "calendar-next") {
                c.icon(
                    if name == "calendar-back" {
                        "chevron-left"
                    } else {
                        "chevron-right"
                    },
                    b.0 - 2.,
                    b.1 - 2. + dy,
                    20.,
                    0xffffff,
                );
            } else {
                asset(c, name, (b.0, b.1 + dy, b.2, b.3));
            }
            if light > 0.001 {
                c.outline(
                    hit.0,
                    hit.1 + dy,
                    hit.2,
                    hit.3,
                    hit.2 * 0.28,
                    motion::mix(0x555555, 0xaaaaaa, light),
                );
            }
            self.figma_hit(hit, action);
        }
    }
    unsafe fn figma_toggle_plate(
        &mut self,
        c: &Canvas,
        b: (f32, f32, f32, f32),
        radius: f32,
        action: Action,
        active: bool,
    ) -> f32 {
        let dy = unsafe { c.glass_content(b, radius, self.figma_hover(b), Some(active), |_| {}) };
        self.figma_hit(b, action);
        dy
    }
    pub(super) unsafe fn draw_figma(&mut self, c: &Canvas, state: &Shared) -> bool {
        unsafe {
            let s = &state.data.settings;
            let width = self.client_width(s) as f32;
            let mut track = state
                .media
                .as_ref()
                .and_then(|m| m.snapshot.lock().ok().map(|t| t.clone()))
                .unwrap_or_default();
            if let Some(scrub) = self
                .scrubbing
                .filter(|scrub| scrub.generation == track.generation)
            {
                track.position = scrub.fraction(self.pointer.0) * track.duration;
                track.updated = None;
            }
            let idle = self.idle_geometry();
            let nw = idle.width + (width - idle.width) * self.expansion;
            let nh = idle.height + (host_height() as f32 - idle.height) * self.expansion;
            c.rect((width - nw) / 2., 0., nw, nh, 0., 0x000000);
            if self.mode == shell_policy::Mode::Indicator && self.expansion < 0.01 {
                return true;
            }
            let (scale, offset) = self.figma_transform();
            c.design_transform(scale, offset);
            if !self.expanded {
                if self.mode == shell_policy::Mode::Compact {
                    let floating = self.island_style.sample(Instant::now());
                    let compact_width = self.compact_width(floating);
                    let compact_height = layout::compact_height(floating);
                    let left = (591. - compact_width) / 2.;
                    let indicator = self.compact_activity.indicator(
                        state.running.is_some(),
                        &track,
                        state.alert.as_ref().map(|a| a.page),
                        self.page,
                    );
                    let icon_right = compact_width - 47. + 15. * floating;
                    let content_left = 56. - 10. * floating;
                    let content_right = if indicator == CompactIndicator::None {
                        compact_width - 33.
                    } else {
                        icon_right - 9.
                    };
                    let text_width = content_right - content_left;
                    let quiet = !track.playing && state.running.is_none() && state.alert.is_none();
                    let opacity = 0.28 + 0.72 * self.compact_activity_progress;
                    let fading = !quiet && s.motion && opacity < 0.999;
                    if fading {
                        c.push_design_opacity((left, 0., compact_width, compact_height), opacity);
                    }
                    if !quiet && s.album_colors && track.palette.is_some() {
                        c.design_glow_at(
                            (left, 0., compact_width, compact_height),
                            self.palette.sample(Instant::now())[0],
                        );
                    }
                    // The supplied compact reference has a 29px music plate,
                    // separate from the shell contour applied by the window region.
                    let context = self.compact_activity.page(
                        state.alert.as_ref().map(|a| a.page),
                        state.running.is_some(),
                        track.playing,
                        self.page,
                    );
                    let icon_size = 29. - 5. * floating;
                    let icon_bounds = (
                        left + 18. - 10. * floating,
                        (compact_height - icon_size) / 2.,
                        icon_size,
                        icon_size,
                    );
                    if quiet {
                        self.draw_companion(
                            c,
                            (left, 0., 65. - 5. * floating, compact_height),
                            true,
                        );
                        if let Some(notice) = &self.power_notice {
                            label(
                                c,
                                &notice.text(),
                                (
                                    left + 65.,
                                    (compact_height - 14.4) / 2.,
                                    (compact_width - 79.).max(1.),
                                ),
                                (9., 600, 0xffffff, 1),
                            );
                        } else {
                            let time = Local::now().format("%H:%M").to_string();
                            let size = 24. - 4. * floating;
                            let text_width = c.design_measure(&time, 110., (size, 600, 0)).0;
                            c.design_glass_clock_sized(
                                &time,
                                (
                                    left + compact_width - 18. - text_width,
                                    (compact_height - size * 1.5) / 2.,
                                    text_width + 1.,
                                    size * 1.5,
                                ),
                                size,
                            );
                        }
                    } else {
                        let mut cover_drawn = false;
                        let dy = c.glass_content(
                            icon_bounds,
                            8.,
                            self.figma_hover(icon_bounds),
                            None,
                            |b| {
                                cover_drawn = context == 0
                                    && track.cover.as_ref().is_some_and(|bytes| {
                                        c.cover_rect(
                                            &format!(
                                                "{}|{}|{}",
                                                track.title, track.artist, track.cover_revision
                                            ),
                                            bytes,
                                            (b.0 + 1.2, b.1 + 1.2, b.2 - 2.4, b.3 - 2.4),
                                            6.8,
                                        )
                                        .is_ok()
                                    });
                            },
                        );
                        if !cover_drawn {
                            c.icon_centered(
                                [
                                    "music-notes",
                                    "timer",
                                    "calendar-dots",
                                    "drop",
                                    "list-checks",
                                    "files",
                                ][context],
                                (
                                    icon_bounds.0,
                                    icon_bounds.1 + dy,
                                    icon_bounds.2,
                                    icon_bounds.3,
                                ),
                                22. - 3. * floating,
                            );
                        }
                        if let Some(notice) = &self.power_notice {
                            label(
                                c,
                                &notice.text(),
                                (left + content_left, 7. - 3. * floating, text_width),
                                (10.5, 600, 0xffffff, 1),
                            );
                        } else if let Some(alert) = &state.alert {
                            label(
                                c,
                                &alert.text,
                                (left + content_left, 7. - 3. * floating, text_width),
                                (12., 700, 0xffffff, 1),
                            );
                        } else if context == 1 && state.running.is_some() {
                            let seconds = remaining(state);
                            let (minutes, seconds) = (seconds / 60, seconds % 60);
                            let caption = if state.data.focus.phase == focus::Phase::Break {
                                i18n::format!(
                                    "{:02}:{:02} · Jeda",
                                    "{:02}:{:02} · Break",
                                    minutes,
                                    seconds
                                )
                            } else {
                                i18n::format!(
                                    "{:02}:{:02} · Fokus",
                                    "{:02}:{:02} · Focus",
                                    minutes,
                                    seconds
                                )
                            };
                            label(
                                c,
                                &caption,
                                (left + content_left, 12. - 5. * floating, text_width),
                                (12., 700, 0xffffff, 1),
                            );
                        } else {
                            self.figma_lyric(
                                c,
                                &track,
                                s,
                                (left + content_left, 10. - 5. * floating, text_width, 21.),
                                (13. - 0.5 * floating, 700, 1),
                            );
                        }
                        if track.playing && context == 0 {
                            let wave_height = 15. - 4. * floating;
                            self.figma_idle_wave(
                                c,
                                state,
                                &track,
                                (
                                    left + content_left,
                                    compact_height - wave_height,
                                    text_width,
                                    wave_height,
                                ),
                            );
                        }
                    }
                    if indicator != CompactIndicator::None {
                        let size = 30. - 6. * floating;
                        let b = (
                            left + icon_right,
                            (compact_height - size) / 2.,
                            32. - 8. * floating,
                            size,
                        );
                        c.glass_button(b, size / 2., false);
                        match indicator {
                            CompactIndicator::Music => {
                                let phase = if s.motion {
                                    track.position_now() as f32 * 5.
                                } else {
                                    0.
                                };
                                for i in 0..4 {
                                    let h = (5.
                                        + ((phase + i as f32 * 1.7).sin() * 0.5 + 0.5) * 10.)
                                        * (1. - 0.2 * floating);
                                    let x = b.0 + b.2 / 2. - 6.75 + i as f32 * 4.5;
                                    c.line(
                                        (x, b.1 + b.3 / 2. - h / 2.),
                                        (x, b.1 + b.3 / 2. + h / 2.),
                                        2.,
                                        0xffffff,
                                    );
                                }
                            }
                            CompactIndicator::Focus => {
                                c.icon_centered("timer", b, 19. - 3. * floating)
                            }
                            CompactIndicator::Reminder => {
                                c.icon_centered("bell", b, 18. - 2. * floating)
                            }
                            CompactIndicator::None => unreachable!(),
                        }
                    }
                    if fading {
                        c.pop_design_opacity();
                    }
                    self.figma_hit((left, 0., compact_width, compact_height), Action::OpenAlert);
                } else if track.playing {
                    if s.album_colors {
                        c.design_glow(101., self.palette.sample(Instant::now())[0]);
                    }
                    self.figma_cover(c, &track, (28., 14., 64., 64.));
                    label(c, &track.title, (108., 22., 160.), (16., 600, 0xffffff, 0));
                    label(c, &track.artist, (108., 50., 160.), (12., 600, 0xffffff, 0));
                    c.rect(276., 30., 1., 41., 0., 0x434343);
                    self.figma_lyric(c, &track, s, (292., 27., 196., 20.), (11.5, 600, 0));
                    self.figma_idle_wave(c, state, &track, (292., 49., 196., 12.));
                    label(
                        c,
                        &crate::ui::clock(track.position_now()),
                        (292., 65., 80.),
                        (9.5, 600, 0xffffff, 0),
                    );
                    label(
                        c,
                        &crate::ui::clock(track.duration),
                        (408., 65., 80.),
                        (9.5, 600, 0xffffff, 4),
                    );
                    self.figma_peek_playback(c, &track);
                    self.figma_hit((28., 8., 475., 80.), Action::OpenAlert);
                } else {
                    self.figma_dashboard(c, state, &track);
                }
                c.design_scale(1.);
                return true;
            }
            c.rect(0., 0., width.max(591.), host_height() as f32, 0., 0x000000);
            let lyrics_reveal = if self.page == 0 {
                c.animate_value(
                    (-1., -1., 0., 0.),
                    5,
                    f32::from(s.show_lyrics || self.spotify_search.open),
                    motion::PAGE,
                )
            } else {
                0.
            };
            if self.page == 0 && s.album_colors && track.palette.is_some() {
                c.design_glow_at(
                    (0., 0., width.max(591.), host_height() as f32),
                    self.palette.sample(Instant::now())[0],
                );
                if lyrics_reveal > 0. {
                    c.lyric_glow(
                        (382., 0., width.max(591.) - 402., host_height() as f32),
                        self.palette.sample(Instant::now()),
                        self.lyric_glow_phase,
                        self.lyric_glow_seed,
                        lyrics_reveal * (1. - 0.55 * self.spotify_search_progress(s.motion)),
                    );
                }
            }
            self.figma_sidebar(c, &track);
            match self.page {
                0 => self.figma_music(c, state, &track, lyrics_reveal),
                1 => self.figma_focus(c, state),
                2 => {
                    self.figma_calendar(c, state);
                }
                _ => {}
            }
            c.design_scale(1.);
            true
        }
    }
    unsafe fn figma_cover(&mut self, c: &Canvas, track: &media::Track, b: (f32, f32, f32, f32)) {
        unsafe {
            if track.title.is_empty() {
                self.draw_companion(c, b, true);
                return;
            }
            let drawn = track.cover.as_ref().is_some_and(|bytes| {
                let result = c.cover_rect(
                    &format!("{}|{}|{}", track.title, track.artist, track.cover_revision),
                    bytes,
                    b,
                    13.,
                );
                match result {
                    Ok(()) => {
                        self.cover_render_error.borrow_mut().clear();
                        true
                    }
                    Err(error) => {
                        *self.cover_render_error.borrow_mut() = error.to_string();
                        false
                    }
                }
            });
            if !drawn {
                c.rect(b.0, b.1, b.2, b.3, 13., 0x232323);
                asset(
                    c,
                    "nav-music",
                    (b.0 + (b.2 - 40.) / 2., b.1 + (b.3 - 40.) / 2., 40., 40.),
                );
            }
        }
    }
    pub(super) unsafe fn figma_sidebar(&mut self, c: &Canvas, track: &media::Track) {
        unsafe {
            let bottom = sidebar_bottom();
            let max_scroll = sidebar_max_scroll(SIDEBAR_ICONS.len());
            let scroll = self.sidebar_scroll.clamp(0., max_scroll);
            let animate = self.shared.borrow().data.settings.motion;
            c.push_design_clip((5., SIDEBAR_TOP - 2., 53., bottom - SIDEBAR_TOP + 4.));
            for (i, (page, icon)) in SIDEBAR_ICONS.into_iter().enumerate() {
                let y = SIDEBAR_TOP + i as f32 * SIDEBAR_STEP;
                let key = (14., y, SIDEBAR_SIZE, SIDEBAR_SIZE);
                let y = y - scroll;
                let hit_y = y.max(SIDEBAR_TOP);
                let hit_h = (y + SIDEBAR_SIZE).min(bottom) - hit_y;
                if hit_h <= 0. {
                    continue;
                }
                let hit = (14., hit_y, SIDEBAR_SIZE, hit_h);
                let hovered = self.figma_hover(hit);
                let hover = c.hover_amount(key, hovered);
                let selection =
                    c.animate_value(key, 2, f32::from(page == self.page), motion::STATE);
                let press =
                    c.animate_value(key, 1, f32::from(self.pressed && hovered), motion::PRESS);
                let dy = if animate { -hover + press * 1.5 } else { 0. };
                let plate = (14., y + dy, SIDEBAR_SIZE, SIDEBAR_SIZE);
                c.glass_panel(
                    plate,
                    11.,
                    motion::mix(
                        motion::mix(0x242424, 0x515153, selection),
                        0x666668,
                        hover * 0.25,
                    ),
                    hover,
                );
                let marker_h = 14. * selection;
                let accent = match page {
                    0 => render::wave_front_color(cover_wave_tints(track)[0]),
                    1 => FOCUS_ACCENT,
                    assistant::PAGE => 0xcbb6ff,
                    _ => ui::page_accent(page),
                };
                if marker_h > 0.05 {
                    c.rect(
                        7.,
                        plate.1 + (SIDEBAR_SIZE - marker_h) / 2.,
                        2.,
                        marker_h,
                        1.,
                        motion::mix(0xe9e9ec, accent, selection),
                    );
                }
                c.icon_centered_tinted(icon, plate, 24., motion::mix(0xffffff, accent, selection));
                self.figma_hit(hit, Action::Page(page));
            }
            c.pop_design_clip();
            if max_scroll > 0. {
                let track = bottom - SIDEBAR_TOP;
                let thumb = (track * track / (track + max_scroll)).max(20.);
                c.rect(56., SIDEBAR_TOP, 1., track, 0.5, 0x434343);
                c.rect(
                    56.,
                    SIDEBAR_TOP + (track - thumb) * scroll / max_scroll,
                    1.,
                    thumb,
                    0.5,
                    0xb8b8b8,
                );
            }
            let settings_bounds = (14., sidebar_settings_y(), SIDEBAR_SIZE, SIDEBAR_SIZE);
            c.rect(
                settings_bounds.0 + (SIDEBAR_SIZE - 14.) / 2.,
                (sidebar_bottom() + settings_bounds.1) / 2.,
                14.,
                0.7,
                0.,
                0x555555,
            );
            c.liquid_glass(
                settings_bounds.0,
                settings_bounds.1,
                settings_bounds.2,
                settings_bounds.3,
                11.,
                None,
                self.figma_hover(settings_bounds),
                false,
            );
            c.icon_centered("settings-gear", settings_bounds, 24.);
            self.figma_hit(settings_bounds, Action::Settings);
        }
    }
    unsafe fn figma_music(
        &mut self,
        c: &Canvas,
        state: &Shared,
        track: &media::Track,
        lyrics_reveal: f32,
    ) {
        unsafe {
            let panel_width = (state.data.settings.width as f32).max(591.);
            let extra = (panel_width - 374.) * (1. - lyrics_reveal);
            let rail_width = 210. + extra;
            let button_step = 57. + extra / 4.;
            let button = |index| (73. + button_step * index, 187., 50., 50.);
            // Cover bottom aligns with the source row; keep a 12px text gutter.
            self.figma_cover(c, track, (73., 20., 99., 99.));
            let title = if track.title.is_empty() {
                i18n::tr("Tidak ada musik")
            } else {
                &track.title
            };
            // Two fixed title rows keep artist/source positions stable for every song.
            c.design_text(
                title,
                (184., 20., (167. + extra).min(panel_width - 246.), 51.),
                (17., 700, 3),
                0xffffff,
            );
            let artist_y = 75.;
            label(
                c,
                &track.artist,
                (184., artist_y, 167. + extra),
                (12., 400, 0xb7bec8, 0),
            );
            let source_y = 99.;
            let spotify = track.source_app.to_lowercase().contains("spotify");
            if track.cover_source_url.is_some() {
                c.icon("external-link", 184., source_y, 18., 0xffffff);
            } else if spotify {
                asset(c, "spotify", (184., source_y, 18., 18.));
            } else {
                c.icon("headphones", 184., source_y, 18., 0xffffff);
            }
            label(
                c,
                if track.cover_source_url.is_some() {
                    i18n::tr("Cover Apple Music ↗")
                } else if spotify {
                    "Spotify"
                } else {
                    i18n::tr("Media Windows")
                },
                (208., source_y + 1., 143. + extra),
                (10., 600, 0xffffff, 0),
            );
            if track.cover_source_url.is_some() {
                self.figma_hit((184., source_y, 167. + extra, 20.), Action::CoverSource);
            }
            self.figma_idle_wave(c, state, track, (73., 128., 278. + extra, 25.));
            label(
                c,
                &crate::ui::clock(track.position_now()),
                (73., 160., 80.),
                (11., 600, 0xffffff, 0),
            );
            label(
                c,
                &crate::ui::clock(track.duration),
                (308. + extra, 160., 43.),
                (11., 600, 0xffffff, 1),
            );
            if track
                .seek_feedback
                .is_some_and(|at| at.elapsed().as_secs_f32() < 2.5)
            {
                label(
                    c,
                    i18n::tr("Posisi tidak bisa diubah"),
                    (151., 160., 153. + extra),
                    (10., 500, 0xd3af99, 1),
                );
            }
            let panel_icon_scale = c.set_icon_scale(0.70);
            let (x, y, w, h) = button(0.);
            let dy = self.figma_toggle_plate(
                c,
                (x, y, w, h),
                17.,
                Action::Setting(16, 1),
                state.data.settings.show_lyrics,
            );
            asset(c, "media-lyrics", (x, y + dy, w, h));
            self.figma_asset_button(c, "media-previous", button(1.), Action::Media(1));
            if track.playing {
                self.figma_asset_button(c, "media-pause", button(2.), Action::Media(0));
            } else {
                let (x, y, w, h) = button(2.);
                let dy = self.figma_plate(c, (x, y, w, h), 17., Action::Media(0));
                c.icon_centered("play", (x, y + dy, w, h), 24.);
            }
            self.figma_asset_button(c, "media-next", button(3.), Action::Media(2));
            let favorite = state
                .data
                .favorites
                .contains(&format!("{}|{}", track.title, track.artist));
            let (x, y, w, h) = button(4.);
            let dy = self.figma_toggle_plate(c, (x, y, w, h), 17., Action::Favorite, favorite);
            if favorite {
                c.icon_filled("heart", x + 12., y + 12. + dy, 26., 0xffffff);
            } else {
                c.icon_centered("heart", (x, y + dy, w, h), 26.);
            }
            c.set_icon_scale(panel_icon_scale);
            let volume = state
                .spotify_volume
                .as_ref()
                .and_then(|v| v.snapshot.lock().ok().map(|s| s.clone()))
                .unwrap_or_default();
            let enabled = spotify && volume.level.is_some();
            let ink = if enabled { 0xffffff } else { 0x858991 };
            c.icon(
                if volume.muted || volume.level == Some(0.) {
                    "speaker-muted"
                } else {
                    "speaker"
                },
                76.,
                249.,
                18.,
                ink,
            );
            let level = if volume.muted {
                0.
            } else {
                volume.level.unwrap_or(0.)
            };
            c.rect(101., 257., rail_width, 3., 1.5, 0x565960);
            if enabled {
                c.rect(101., 257., rail_width * level, 3., 1.5, 0xd7dce4);
                c.ellipse(101. + rail_width * level - 5., 253.5, 10., 10., 0xffffff);
                self.figma_hit((73., 246., 24., 25.), Action::SpotifyMute);
                self.figma_hit((96., 246., rail_width + 10., 25.), Action::SpotifyVolume);
            }
            label(
                c,
                &volume
                    .level
                    .map_or_else(|| "—".to_string(), |_| format!("{:.0}%", level * 100.)),
                (318. + extra, 249., 36.),
                (10., 500, ink, 4),
            );
            let search_button = (panel_width - 52., 25., 32., 28.);
            label(
                c,
                &Local::now().format("%H:%M").to_string(),
                (panel_width - 64., 7., 44.),
                (10., 600, 0xa8b0bc, 4),
            );
            let animated = state.data.settings.motion;
            let search = self.spotify_search_progress(animated);
            let dy = self.figma_plate(c, search_button, 8., Action::SpotifySearchPanel);
            for (icon, opacity, size) in [("search", 1. - search, 18.), ("x", search, 15.)] {
                if opacity > 0. {
                    if opacity < 1. {
                        c.push_design_opacity((search_button.0, 22., 32., 35.), opacity);
                    }
                    c.icon_centered(icon, (search_button.0, 25. + dy, 32., 28.), size);
                    if opacity < 1. {
                        c.pop_design_opacity();
                    }
                }
            }
            let opacity = ((lyrics_reveal - 0.35) / 0.65).clamp(0., 1.);
            let divider = 365. + extra;
            if opacity > 0. {
                if opacity < 1. {
                    c.push_design_opacity((divider, 25., 2., 230.), opacity);
                }
                c.rect(divider, 25., 1., 230., 0.5, 0x343a43);
                if opacity < 1. {
                    c.pop_design_opacity();
                }
            }
            let left = divider + 5.;
            let clip = (left, 23., panel_width - left, 249.);
            c.push_design_clip(clip);
            let lyric_opacity = opacity * (1. - search);
            if lyric_opacity > 0. {
                let bounds = (382., 55. - 8. * search, panel_width - 402., 200.);
                if lyric_opacity < 1. {
                    c.push_design_opacity(clip, lyric_opacity);
                }
                self.figma_music_lyrics(c, state, track, bounds);
                if lyric_opacity < 1. {
                    c.pop_design_opacity();
                }
            }
            if search > 0. {
                let (scale, offset) = self.figma_transform();
                if search < 1. {
                    c.push_design_opacity(clip, search);
                }
                c.design_transform_y(scale, offset, 10. * (1. - search));
                let first_hit = self.hits.len();
                self.figma_spotify_search(c, panel_width);
                if !self.spotify_search_input_ready(animated) {
                    self.hits.truncate(first_hit);
                }
                c.design_transform(scale, offset);
                if search < 1. {
                    c.pop_design_opacity();
                }
            }
            c.pop_design_clip();
        }
    }
    unsafe fn figma_music_lyrics(
        &self,
        c: &Canvas,
        state: &Shared,
        track: &media::Track,
        bounds: (f32, f32, f32, f32),
    ) {
        if !track.title.is_empty() && !track.lyrics.is_empty() {
            let position = track.position_now();
            let frame = track.lyric_frame(position, state.data.settings.motion);
            let current = frame.latest.unwrap_or(0);
            let start = current.saturating_sub(14);
            let end = (current + 15).min(track.lyrics.len());
            let mut active_size = 15.;
            if c.design_measure(&track.lyrics[current].text, bounds.2, (active_size, 700, 2))
                .1
                > 140.
            {
                active_size = 13.5;
            }
            let heights: Vec<f32> = (start..end)
                .map(|index| {
                    let featured = frame.latest == Some(index);
                    let size = if featured { active_size } else { 11.5 };
                    c.design_measure(
                        &track.lyrics[index].text,
                        bounds.2,
                        (size, if featured { 700 } else { 400 }, 2),
                    )
                    .1
                    .ceil()
                    .min(if featured { bounds.3 } else { size * 4.5 })
                })
                .collect();
            let rows = layout::lyric_rows(&heights, current - start, bounds.1, bounds.3, 10.);
            let mut offset = 0.;
            if frame.entry < 1.
                && let Some(previous) = rows.iter().rev().find(|row| {
                    start + row.index < current && !track.lyrics[start + row.index].text.is_empty()
                })
            {
                let previous_rows =
                    layout::lyric_rows(&heights, previous.index, bounds.1, bounds.3, 10.);
                if let Some(old) = previous_rows.iter().find(|row| row.index == previous.index) {
                    offset = (old.y - previous.y).clamp(0., 26.) * (1. - frame.entry);
                }
            }
            c.push_design_clip(bounds);
            for row in rows {
                let index = start + row.index;
                let distance = index.abs_diff(current).saturating_sub(1) as f32;
                let featured = frame.latest == Some(index);
                let style = if featured {
                    (active_size, 700, 2)
                } else {
                    (11.5, 400, 2)
                };
                let row_bounds = (bounds.0, row.y + offset, bounds.2, row.height);
                let line = &track.lyrics[index];
                let next = track.next_lyric_time(index);
                if frame.active == Some(index) {
                    c.design_karaoke(line, (position, next), row_bounds, style, false, 1.);
                } else {
                    let muted = motion::mix(0xa8b0bc, 0x68717e, distance / 5.);
                    if let Some((_, fade)) = frame.outgoing.filter(|(old, _)| *old == index) {
                        c.design_text(
                            &line.text,
                            row_bounds,
                            style,
                            motion::mix(0xffffff, muted, fade),
                        );
                    } else {
                        c.design_text(&line.text, row_bounds, style, muted);
                    }
                }
            }
            c.pop_design_clip();
        } else if !track.title.is_empty() {
            label(
                c,
                track.lyrics_message(),
                (bounds.0, bounds.1 + 73., bounds.2),
                (12., 600, 0x777777, 1),
            );
        }
    }
    unsafe fn figma_spotify_search(&mut self, c: &Canvas, panel_width: f32) {
        let width = panel_width - 402.;
        let free = self.spotify_search.mode == spotify::SearchMode::Free;
        asset(c, "spotify", (383., 32., 17., 17.));
        let setup = self.spotify_search.mode == spotify::SearchMode::Setup;
        label(
            c,
            i18n::tr(if setup {
                "Sambungkan Spotify"
            } else {
                "Cari lagu"
            }),
            (406., 33., width - 64.),
            (13., 700, 0xffffff, 0),
        );
        let field_width = width - if free { 0. } else { 40. };
        let focused =
            !self.spotify_search.edit.is_invalid() && GetFocus() == self.spotify_search.edit;
        c.rect(382., 63., field_width, 36., 9., input::FIELD);
        c.outline(
            382.,
            63.,
            field_width,
            36.,
            9.,
            if focused { 0x6e7b8b } else { 0x343a43 },
        );
        if self.spotify_search.edit.is_invalid()
            || !IsWindowVisible(self.spotify_search.edit).as_bool()
        {
            let text = if setup {
                i18n::tr("Client ID Spotify")
            } else if self.spotify_search.query.is_empty() {
                i18n::tr("Judul atau artis")
            } else {
                &self.spotify_search.query
            };
            label(
                c,
                text,
                (394., 71., field_width - 24.),
                (12., 500, 0xa8b0bc, 0),
            );
        }
        if free {
            let submit = (382., 112., width, 38.);
            let dy = self.figma_plate(c, submit, 9., Action::SpotifySearchSubmit);
            c.design_text(
                i18n::tr("Cari di Spotify desktop"),
                (submit.0, submit.1 + dy, submit.2, submit.3),
                (12., 650, render::DESIGN_CENTERED),
                0xffffff,
            );
            if !self.spotify_search.notice.is_empty() {
                c.design_text(
                    i18n::tr(self.spotify_search.notice),
                    (386., 160., width - 8., 40.),
                    (10.5, 500, 3),
                    0xa8b0bc,
                );
            }
            c.rect(382., 198., width, 1., 0.5, 0x303035);
            let half = (width - 10.) / 2.;
            for (x, text, action) in [
                (382., "Buka di browser", Action::SpotifySearchWeb),
                (392. + half, "Atur Premium", Action::SpotifySearchSetup),
            ] {
                let button = (x, 207., half, 28.);
                let dy = self.figma_plate(c, button, 8., action);
                c.design_text(
                    i18n::tr(text),
                    (x + 4., button.1 + dy, half - 8., button.3),
                    (10., 600, render::DESIGN_CENTERED),
                    0xc9cbd1,
                );
            }
            return;
        }
        let busy = self.spotify_search.busy();
        if !busy {
            let button = (panel_width - 54., 63., 34., 36.);
            let dy = self.figma_plate(c, button, 9., Action::SpotifySearchSubmit);
            c.icon_centered(
                if setup { "external-link" } else { "search" },
                (button.0, button.1 + dy, button.2, button.3),
                18.,
            );
        }
        let notice = i18n::tr(self.spotify_search.notice);
        if setup || self.spotify_search.songs.is_empty() {
            let description = if !notice.is_empty() {
                notice
            } else if setup {
                i18n::tr(
                    "Tempel Client ID dari Spotify Developer Dashboard. Gunakan redirect URI ini.",
                )
            } else {
                i18n::tr("Cari judul atau artis, lalu pilih lagu untuk diputar.")
            };
            c.design_text(
                description,
                (382., 111., width, 52.),
                (11., 500, 2),
                0xa8b0bc,
            );
            if setup {
                c.design_text(
                    spotify::REDIRECT,
                    (382., 160., width, 38.),
                    (10., 500, 2),
                    0xffffff,
                );
                let copy = (382., 200., width, 25.);
                let dy = self.figma_plate(c, copy, 7., Action::SpotifySearchCopyRedirect);
                c.design_text(
                    i18n::tr("Salin redirect URI"),
                    (382., 200. + dy, width, 25.),
                    (10., 600, render::DESIGN_CENTERED),
                    0xffffff,
                );
            }
        } else {
            let offset = self.spotify_search.offset;
            for slot in 0..spotify_ui::ROWS {
                let index = offset + slot;
                let Some(song) = self.spotify_search.songs.get(index) else {
                    break;
                };
                let y = 103. + slot as f32 * 29.;
                let play = (panel_width - 51., y, 31., 27.);
                let link = (382., y, width - 37., 27.);
                let hovered = self.figma_hover(link);
                c.rect(
                    link.0,
                    y,
                    link.2,
                    link.3,
                    7.,
                    if hovered { 0x2a2a2f } else { 0x1a1a1e },
                );
                label(
                    c,
                    &song.title,
                    (389., y + 1., width - 52.),
                    (10.5, 650, 0xffffff, 0),
                );
                label(
                    c,
                    &song.artist,
                    (389., y + 15., width - 52.),
                    (9., 400, 0xa8b0bc, 0),
                );
                self.figma_hit(link, Action::SpotifySearchLink(index));
                if !busy {
                    let dy = self.figma_plate(c, play, 8., Action::SpotifySearchPlay(index));
                    c.icon_centered("play", (play.0, play.1 + dy, play.2, play.3), 16.);
                }
            }
        }
        if setup {
            if !busy {
                let half = (width - 8.) / 2.;
                for (x, label, action) in [
                    (382., "Kembali", Action::SpotifySearchSetup),
                    (390. + half, "Dashboard", Action::SpotifySearchHelp),
                ] {
                    let b = (x, 229., half, 28.);
                    let dy = self.figma_plate(c, b, 8., action);
                    c.design_text(
                        i18n::tr(label),
                        (x, b.1 + dy, half, b.3),
                        (10., 600, render::DESIGN_CENTERED),
                        0xffffff,
                    );
                }
            }
        } else if !notice.is_empty() && !self.spotify_search.songs.is_empty() {
            // Keep playback errors visible while the result list remains intact.
            c.rect(382., 222., width, 38., 8., 0x25252a);
            c.design_text(
                notice,
                (388., 223., width - 12., 36.),
                (10., 600, 2),
                0xffffff,
            );
        } else if !busy {
            let offset = self.spotify_search.offset;
            let total = self.spotify_search.songs.len();
            for (delta, x, icon, enabled) in [
                (-1, 382., "chevron-up", offset > 0),
                (
                    1,
                    panel_width - 44.,
                    "chevron-down",
                    offset + spotify_ui::ROWS < total,
                ),
            ] {
                if enabled {
                    let b = (x, 239., 24., 20.);
                    let dy = self.figma_plate(c, b, 6., Action::SpotifySearchScroll(delta));
                    c.icon_centered(icon, (x, b.1 + dy, b.2, b.3), 13.);
                }
            }
            let b = (412., 231., width - 60., 28.);
            let dy = self.figma_plate(c, b, 7., Action::SpotifySearchDisconnect);
            c.design_text(
                i18n::tr("Putuskan Spotify"),
                (b.0, b.1 + dy, b.2, b.3),
                (10., 600, render::DESIGN_CENTERED),
                0xffffff,
            );
        }
    }
    unsafe fn figma_focus(&mut self, c: &Canvas, state: &Shared) {
        let width = (state.data.settings.width as f32).max(591.);
        let timer = (73., 20., 220., 197.);
        let right = (305., 20., width - 324., 197.);
        let left = remaining(state);
        let phase = state.data.focus.phase;
        let done = focus::finished(phase);
        let is_break = matches!(phase, focus::Phase::Break | focus::Phase::BreakFinished);
        let active_task = state.data.focus.task.and_then(|i| state.data.tasks.get(i));
        let editable = focus::can_configure(state);
        let status = match phase {
            focus::Phase::Finished => "Sesi selesai",
            focus::Phase::Break => "Jeda",
            focus::Phase::BreakFinished => "Jeda selesai",
            focus::Phase::Extension => "Fokus tambahan",
            focus::Phase::Work if state.running.is_some() => "Sedang fokus",
            focus::Phase::Work if editable => "Siap mulai",
            focus::Phase::Work => "Dijeda",
        };
        unsafe {
            let fraction = c.animate_value(
                timer,
                4,
                left as f32 / focus::duration(state).max(1) as f32,
                motion::STATE,
            );
            let time = state
                .running
                .filter(|_| state.data.settings.motion)
                .map_or(0., |start| start.elapsed().as_secs_f32());
            c.rect(timer.0, timer.1, timer.2, timer.3, 30., 0x191919);
            c.liquid_level(
                timer,
                fraction,
                time,
                if is_break { 0x474749 } else { FOCUS_ACCENT },
            );
            c.glass_edge(timer.0, timer.1, timer.2, timer.3, 30., 0.);
            c.rect(timer.0 + 48., 37., 124., 23., 8., 0xe8242020);
            label(
                c,
                i18n::tr(status),
                (timer.0 + 48., 40., 124.),
                (11., 600, 0xffffff, 1),
            );
            c.rect(timer.0 + 19., 69., 182., 76., 18., 0xe8242020);
            c.design_text(
                &format!("{:02}:{:02}", left / 60, left % 60),
                (timer.0 + 19., 71., 182., 72.),
                (45., 700, 1),
                0xffffff,
            );
            let primary = (timer.0 + 22., 166., 122., 33.);
            let dy = self.figma_plate(c, primary, 10., Action::Focus);
            label(
                c,
                i18n::tr(if state.running.is_some() {
                    "Jeda"
                } else if phase == focus::Phase::BreakFinished || phase == focus::Phase::Finished {
                    "Mulai lagi"
                } else if editable {
                    "Mulai"
                } else {
                    "Lanjut"
                }),
                (primary.0, primary.1 + 7. + dy, primary.2),
                (12., 700, 0xffffff, 1),
            );
            let reset = (timer.0 + 152., 166., 46., 33.);
            let dy = self.figma_plate(c, reset, 10., Action::Reset);
            asset(c, "repeat", (reset.0 + 12., reset.1 + 5. + dy, 22., 23.));

            c.glass_panel(right, 15., 0x1d1d1f, 0.);
            let x = right.0 + 13.;
            let w = right.2 - 26.;
            if self.focus_task_menu && editable {
                self.figma_focus_tasks(c, state, x, w);
            } else {
                label(
                    c,
                    i18n::tr("Sesi ini"),
                    (x, 31., w),
                    (13., 700, 0xffffff, 0),
                );
                let task = (x, 55., w, 30.);
                let task_name =
                    active_task.map_or(i18n::tr("Sesi bebas"), |task| task.text.as_str());
                if editable {
                    let dy = self.figma_plate(c, task, 8., Action::FocusTaskMenu);
                    c.design_text(
                        task_name,
                        (x + 10., 61. + dy, w - 34., 20.),
                        (11.5, 600, 0),
                        0xffffff,
                    );
                    c.icon_centered("chevron-down", (x + w - 27., 55. + dy, 23., 30.), 14.);
                } else {
                    c.rect(x, 55., w, 30., 8., 0x28282a);
                    c.design_text(
                        task_name,
                        (x + 10., 61., w - 20., 20.),
                        (11.5, 600, 0),
                        0xffffff,
                    );
                }
                if phase == focus::Phase::Finished {
                    let half = (w - 7.) / 2.;
                    for (name, b, action) in [
                        ("Mulai jeda", (x, 99., half, 35.), Action::StartFocusBreak),
                        (
                            "Lanjut 5 menit",
                            (x + half + 7., 99., half, 35.),
                            Action::ExtendFocus,
                        ),
                    ] {
                        let dy = self.figma_plate(c, b, 8., action);
                        label(
                            c,
                            i18n::tr(name),
                            (b.0, b.1 + 8. + dy, b.2),
                            (11., 700, 0xffffff, 1),
                        );
                    }
                    if active_task.is_some_and(|task| !task.done) {
                        let b = (x, 145., w, 34.);
                        let dy = self.figma_plate(c, b, 8., Action::FinishFocusTask);
                        label(
                            c,
                            i18n::tr("Tandai tugas selesai"),
                            (x, 153. + dy, w),
                            (11., 600, 0xffffff, 1),
                        );
                    } else if active_task.is_some() {
                        label(
                            c,
                            i18n::tr("Tugas selesai"),
                            (x, 149., w),
                            (11., 600, 0x97cda7, 0),
                        );
                    }
                } else if editable {
                    for (title, y, selected, options, make_action) in [
                        (
                            "Durasi fokus",
                            96.,
                            state.data.settings.focus_minutes,
                            [15, 25, 45],
                            Action::SetFocusMinutes as fn(u32) -> Action,
                        ),
                        (
                            "Jeda setelah sesi",
                            151.,
                            state.data.settings.focus_break_minutes,
                            [3, 5, 10],
                            Action::SetBreakMinutes as fn(u32) -> Action,
                        ),
                    ] {
                        label(c, i18n::tr(title), (x, y, w), (10.5, 600, 0xbcbcc0, 0));
                        let button = (w - 12.) / 3.;
                        for (i, minutes) in options.into_iter().enumerate() {
                            let b = (x + i as f32 * (button + 6.), y + 18., button, 28.);
                            let dy = self.figma_toggle_plate(
                                c,
                                b,
                                8.,
                                make_action(minutes),
                                selected == minutes,
                            );
                            label(
                                c,
                                &i18n::format!("{} mnt", "{} min", minutes),
                                (b.0, b.1 + 4. + dy, b.2),
                                (10.5, 700, 0xffffff, 1),
                            );
                        }
                    }
                } else if !done {
                    let subtitle = if is_break {
                        i18n::format!(
                            "Jeda {} menit",
                            "{} min break",
                            state.data.settings.focus_break_minutes
                        )
                    } else {
                        i18n::format!(
                            "Fokus {} menit",
                            "{} min focus",
                            focus::duration(state) / 60
                        )
                    };
                    label(c, &subtitle, (x, 105., w), (11., 600, 0xbcbcc0, 0));
                }
            }
            let today = Local::now().format("%Y-%m-%d").to_string();
            let seconds = state.data.focus_history.get(&today).copied().unwrap_or(0);
            let count = state.data.focus.completed.get(&today).copied().unwrap_or(0);
            c.rect(73., 228., width - 92., 35., 10., 0x1c1c1e);
            label(
                c,
                i18n::tr("Hari ini"),
                (85., 237., 68.),
                (11., 700, 0xffffff, 0),
            );
            let summary = if seconds >= 3600 {
                i18n::format!(
                    "{} sesi · {} j {} mnt fokus",
                    "{} sessions · {} h {} min focused",
                    count,
                    seconds / 3600,
                    (seconds / 60) % 60
                )
            } else {
                i18n::format!(
                    "{} sesi · {} mnt fokus",
                    "{} sessions · {} min focused",
                    count,
                    seconds / 60
                )
            };
            label(
                c,
                &summary,
                (164., 237., width - 195.),
                (11., 600, 0xc9c9cd, 4),
            );
        }
    }

    unsafe fn figma_focus_tasks(&mut self, c: &Canvas, state: &Shared, x: f32, w: f32) {
        unsafe {
            label(
                c,
                i18n::tr("Pilih tugas"),
                (x, 31., w - 85.),
                (13., 700, 0xffffff, 0),
            );
            for (icon, dx, action) in [
                ("chevron-up", 84., Action::FocusTaskScroll(-1)),
                ("chevron-down", 56., Action::FocusTaskScroll(1)),
                ("x", 28., Action::FocusTaskMenu),
            ] {
                let b = (x + w - dx, 27., 25., 24.);
                let dy = self.figma_plate(c, b, 7., action);
                c.icon_centered(icon, (b.0, b.1 + dy, b.2, b.3), 13.);
            }
            let b = (x, 55., w, 30.);
            let dy = self.figma_toggle_plate(
                c,
                b,
                8.,
                Action::FocusTask(None),
                state.data.focus.task.is_none(),
            );
            label(
                c,
                i18n::tr("Sesi bebas"),
                (x + 9., 61. + dy, w - 18.),
                (11., 600, 0xffffff, 0),
            );
            let mut displayed = false;
            for (row, (index, task)) in state
                .data
                .tasks
                .iter()
                .enumerate()
                .filter(|(_, task)| !task.done)
                .skip(self.focus_task_offset)
                .take(3)
                .enumerate()
            {
                displayed = true;
                let b = (x, 92. + row as f32 * 36., w, 30.);
                let dy = self.figma_toggle_plate(
                    c,
                    b,
                    8.,
                    Action::FocusTask(Some(index)),
                    state.data.focus.task == Some(index),
                );
                c.design_text(
                    &task.text,
                    (x + 9., b.1 + 5. + dy, w - 18., 21.),
                    (11., 600, 0),
                    0xffffff,
                );
            }
            if !displayed {
                label(
                    c,
                    i18n::tr("Belum ada tugas aktif"),
                    (x + 9., 99., w - 18.),
                    (11., 500, 0xbcbcc0, 0),
                );
            }
        }
    }
    unsafe fn figma_calendar(&mut self, c: &Canvas, state: &Shared) {
        unsafe {
            let extra = (state.data.settings.width as f32 - 591.).max(0.);
            let agenda_w = 171. + extra;
            c.rect(72., 20., 293., 244., 21., 0x191919);
            c.glass_edge(72., 20., 293., 244., 21., 0.);
            c.rect(375., 20., 197. + extra, 244., 21., 0xffc2a6f3);
            c.glass_edge(375., 20., 197. + extra, 244., 21., 0.);
            label(
                c,
                i18n::tr(calendar::MONTHS[self.calendar.month.month0() as usize]),
                (96., 29., 112.),
                (14.5, 600, 0xffffff, 0),
            );
            label(
                c,
                &self.calendar.month.year().to_string(),
                (96., 49., 112.),
                (10.5, 400, 0xa8a8ae, 0),
            );
            c.rect(96., 69., 251., 0.8, 0., 0xaaaaaa);
            self.figma_asset_button(c, "calendar-back", (294., 39., 16., 16.), Action::Month(-1));
            self.figma_asset_button(c, "calendar-next", (326., 39., 16., 16.), Action::Month(1));
            for (i, day) in calendar::DAYS_SHORT.into_iter().enumerate() {
                label(
                    c,
                    &i18n::tr(day).to_uppercase(),
                    (96. + i as f32 * 35., 81., 32.),
                    (10., 400, 0x7e818c, 1),
                );
            }
            for (i, date) in self.calendar.days().into_iter().enumerate() {
                let x = 101. + (i % 7) as f32 * 35.;
                let y = 102. + (i / 7) as f32 * 23.;
                let selected = date == self.calendar.selected;
                let today = date == Local::now().date_naive();
                let light = c.hover_amount((x, y, 22., 22.), self.figma_hover((x, y, 22., 22.)));
                c.calendar_marker(x, y, selected, today, light);
                label(
                    c,
                    &date.day().to_string(),
                    (x, y + 0.5, 22.),
                    (
                        14.,
                        if selected || today { 600 } else { 400 },
                        if date.month() == self.calendar.month.month() {
                            0xffffff
                        } else {
                            0x555555
                        },
                        1,
                    ),
                );
                if state.data.events.iter().any(|event| {
                    Local
                        .timestamp_opt(event.at, 0)
                        .single()
                        .is_some_and(|d| d.date_naive() == date)
                }) {
                    c.ellipse(
                        x + 10.,
                        y + 19.,
                        2.,
                        2.,
                        if selected { 0xffffff } else { 0xc878fe },
                    );
                }
                self.figma_hit((x - 4., y, 32., 22.), Action::Date(date.num_days_from_ce()));
            }
            let dy = self.figma_plate(c, (218., 35., 64., 24.), 8., Action::Today);
            label(
                c,
                i18n::tr("Hari ini"),
                (218., 39. + dy, 64.),
                (10., 600, 0xffffff, 1),
            );
            if self.calendar.form {
                self.figma_event_editor(c);
                return;
            }
            label(
                c,
                &format!(
                    "{}, {}",
                    i18n::tr(
                        calendar::DAYS_SHORT
                            [self.calendar.selected.weekday().num_days_from_monday() as usize]
                    ),
                    self.calendar.selected.day()
                ),
                (388., 37., 94. + extra),
                (15., 600, AGENDA_INK, 0),
            );
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
                .min(events.len().saturating_sub(1) / AGENDA_PAGE_SIZE * AGENDA_PAGE_SIZE);
            for (row, (index, event)) in events
                .iter()
                .skip(self.calendar.agenda_offset)
                .take(AGENDA_PAGE_SIZE)
                .enumerate()
            {
                let y = 82. + row as f32 * 68.;
                let time = Local
                    .timestamp_opt(event.at, 0)
                    .single()
                    .map(|d| d.format("%H.%M").to_string())
                    .unwrap_or_default();
                label(c, &time, (388., y + 3., 36.), (10., 600, 0x44216e, 0));
                let title_y = y + 20.;
                let title_h = c
                    .design_measure(&event.title, agenda_w, (10.5, 400, 3))
                    .1
                    .ceil()
                    .clamp(16., 32.);
                c.design_text(
                    &event.title,
                    (388., title_y, agenda_w, title_h),
                    (10.5, 400, 3),
                    AGENDA_INK,
                );
                label(
                    c,
                    &format!(
                        "{}m before",
                        event
                            .lead_minutes
                            .unwrap_or(state.data.settings.event_lead_minutes)
                    ),
                    (388., title_y + title_h + 5., agenda_w),
                    (8.5, 400, AGENDA_MUTED, 0),
                );
                self.figma_asset_button(
                    c,
                    "edit-event",
                    (517. + extra, y + 1., 13., 12.),
                    Action::EditEvent(*index),
                );
                self.figma_asset_button(
                    c,
                    "delete-event",
                    (541. + extra, y + 1., 13., 13.),
                    Action::DeleteEvent(*index),
                );
                if !event.url.is_empty() {
                    self.figma_hit(
                        (388., title_y, agenda_w, title_h),
                        Action::EventLink(*index),
                    );
                }
            }
            if events.is_empty() {
                label(
                    c,
                    i18n::tr("Belum ada acara"),
                    (388., 87., agenda_w),
                    (10., 400, AGENDA_MUTED, 0),
                );
            }
            if events.len() > AGENDA_PAGE_SIZE {
                let pager_x = 415.5 + extra / 2.;
                c.rect(pager_x, 228., 116., 24., 8., 0x281e30);
                c.glass_edge(pager_x, 228., 116., 24., 8., 0.);
                self.figma_asset_button(
                    c,
                    "calendar-back",
                    (pager_x + 8., 232., 16., 16.),
                    Action::AgendaPage(-1),
                );
                self.figma_asset_button(
                    c,
                    "calendar-next",
                    (pager_x + 92., 232., 16., 16.),
                    Action::AgendaPage(1),
                );
                label(
                    c,
                    &format!(
                        "{}/{}",
                        self.calendar.agenda_offset / AGENDA_PAGE_SIZE + 1,
                        events.len().div_ceil(AGENDA_PAGE_SIZE)
                    ),
                    (pager_x + 32., 232., 52.),
                    (9.5, 600, 0xd8d1de, 1),
                );
            }
            let dy = self.figma_plate(c, (492. + extra, 32., 67., 24.), 8., Action::EventForm);
            label(
                c,
                i18n::tr("+ Acara"),
                (492. + extra, 36. + dy, 67.),
                (10., 600, 0xffffff, 1),
            );
            self.calendar.form_y = 26.;
            self.calendar.content_height = host_height() as f32;
            self.calendar.scroll = 0.;
            self.calendar.target_scroll = 0.;
        }
    }
    unsafe fn figma_event_editor(&mut self, c: &Canvas) {
        unsafe {
            let editing = self.calendar.editing.is_some();
            let extra = (self.shared.borrow().data.settings.width as f32 - 591.).max(0.);
            let editor_w = 171. + extra;
            label(
                c,
                if editing {
                    i18n::tr("Ubah acara")
                } else {
                    i18n::tr("Tambah acara")
                },
                (388., 29., editor_w),
                (14., 600, AGENDA_INK, 0),
            );
            for (text, x, y, w) in [
                (i18n::tr("Nama acara"), 388., 56., editor_w),
                (i18n::tr("Jam"), 388., 108., 81. + extra / 2.),
                (
                    i18n::tr("Pengingat (mnt)"),
                    478. + extra / 2.,
                    108.,
                    81. + extra / 2.,
                ),
                (i18n::tr("Link (opsional)"), 388., 160., editor_w),
            ] {
                label(c, text, (x, y, w), (9.5, 400, AGENDA_MUTED, 0));
            }
            let dy = self.figma_plate(c, (388., 226., 108. + extra, 30.), 9., Action::AddEvent);
            label(
                c,
                if editing {
                    i18n::tr("Perbarui")
                } else {
                    i18n::tr("Simpan")
                },
                (388., 232. + dy, 108. + extra),
                (11., 600, 0xffffff, 1),
            );
            let dy = self.figma_plate(
                c,
                (504. + extra, 226., 55., 30.),
                9.,
                Action::CancelEditEvent,
            );
            label(
                c,
                i18n::tr("Batal"),
                (504. + extra, 233. + dy, 55.),
                (10., 400, 0xffffff, 1),
            );
            if !self.calendar.status.is_empty() {
                c.design_text(
                    &i18n::message(&self.calendar.status),
                    (388., 209., editor_w, 16.),
                    (9.5, 400, 3),
                    AGENDA_MUTED,
                );
            }
            self.calendar.form_y = 26.;
            self.calendar.content_height = host_height() as f32;
            self.calendar.scroll = 0.;
            self.calendar.target_scroll = 0.;
        }
    }
    unsafe fn figma_dashboard(&mut self, c: &Canvas, state: &Shared, track: &media::Track) {
        unsafe {
            mono_badge(c, "clock", (30., 12., 31., 31.));
            mono_badge(c, "calendar-dots", (30., 50., 32., 32.));
            c.design_glass_clock(
                &Local::now().format("%H.%M").to_string(),
                (71., 2., 147., 57.6),
            );
            let date = i18n::date(Local::now().date_naive());
            let date_width = c.design_measure(&date, 2000., (15., 600, 0)).0;
            let date_size = (15. * 184. / date_width.max(1.)).clamp(12., 15.);
            label(c, &date, (71., 54., 184.), (date_size, 600, 0xffffff, 0));
            c.rect(276., 18., 1., 53., 0., 0x434343);
            let done = state.data.tasks.iter().filter(|t| t.done).count();
            for (name, y, text, ratio, ink) in [
                (
                    "nav-tasks",
                    16.,
                    format!("Task {done}/{}", state.data.tasks.len()),
                    done as f32 / state.data.tasks.len().max(1) as f32,
                    0x58ff5e,
                ),
                (
                    "nav-water",
                    41.,
                    format!(
                        "Water {}/{}",
                        state.data.water, state.data.settings.water_goal
                    ),
                    state.data.water as f32 / state.data.settings.water_goal.max(1) as f32,
                    0x00d9ff,
                ),
            ] {
                mono_badge(
                    c,
                    if name == "nav-tasks" {
                        "list-checks"
                    } else {
                        "drop"
                    },
                    (292., y, 20., 20.),
                );
                label(c, &text, (317., y + 1., 84.), (8., 600, 0xffffff, 0));
                c.rect(317., y + 14., 65., 3., 1.5, 0xd9d9d9);
                c.rect(317., y + 14., 65. * ratio.clamp(0., 1.), 3., 1.5, ink);
            }
            mono_badge(c, "music-notes", (292., 66., 20., 20.));
            label(c, &track.title, (317., 65., 78.), (7.5, 600, 0xffffff, 0));
            self.figma_idle_wave(c, state, track, (317., 75., 65., 9.));
            c.rect(400., 16., 98., 68., 11., 0x191919);
            c.liquid_level(
                (401., 17., 96., 66.),
                remaining(state) as f32 / focus::duration(state).max(1) as f32,
                0.,
                if matches!(
                    state.data.focus.phase,
                    focus::Phase::Break | focus::Phase::BreakFinished
                ) {
                    0x474749
                } else {
                    FOCUS_ACCENT
                },
            );
            c.glass_edge(400., 16., 98., 68., 11., 0.);
            label(
                c,
                &format!("{:02}.{:02}", remaining(state) / 60, remaining(state) % 60),
                (410., 23., 78.),
                (24., 600, 0xffffff, 1),
            );
            label(
                c,
                &i18n::format!("{} mnt", "{} min", focus::duration(state) / 60),
                (410., 56., 78.),
                (5., 600, 0xffffff, 1),
            );
            c.rect(426., 66., 46., 13., 6.5, 0x1affffff);
            c.outline(426., 66., 46., 13., 6.5, 0x35ffffff);
            label(
                c,
                if state.running.is_some() {
                    i18n::tr("Jeda")
                } else {
                    i18n::tr("Mulai")
                },
                (426., 66.5, 46.),
                (6.5, 600, 0xffffff, 1),
            );
            self.figma_hit((400., 16., 98., 68.), Action::Focus);
            self.figma_peek_playback(c, track);
            self.figma_hit((20., 8., 260., 80.), Action::OpenAlert);
        }
    }
}
#[cfg(test)]
mod compact_context_tests {
    #[test]
    fn passive_indicator_follows_running_activity_and_never_offers_resume() {
        use super::{CompactActivity, CompactIndicator};
        let mut activity = CompactActivity::default();
        let mut track = crate::media::Track {
            title: "Fixture".into(),
            playing: true,
            ..Default::default()
        };
        activity.observe(true, false);
        assert_eq!(
            activity.indicator(false, &track, None, 2),
            CompactIndicator::Music
        );
        activity.observe(true, true);
        assert_eq!(
            activity.indicator(true, &track, None, 2),
            CompactIndicator::Focus
        );
        activity.observe(false, false);
        track.playing = false;
        assert_eq!(
            activity.indicator(false, &track, None, 2),
            CompactIndicator::None
        );
        activity.reset_focus();
        assert_eq!(
            activity.indicator(false, &track, None, 2),
            CompactIndicator::None
        );
        let alert = crate::Alert {
            text: "Fixture".into(),
            is_water: true,
            page: 3,
            url: String::new(),
        };
        activity.observe_alert(Some(&alert));
        assert_eq!(
            activity.indicator(false, &track, Some(3), 2),
            CompactIndicator::Reminder
        );
    }
    #[test]
    fn a_short_metadata_gap_during_focus_is_not_a_music_resume() {
        let mut state = super::CompactActivity::default();
        let mut track = crate::media::Track {
            title: "First track".into(),
            playing: true,
            updated: Some(std::time::Instant::now()),
            ..Default::default()
        };
        state.observe_track(&track, false);
        state.observe_track(&track, true);
        state.observe_track(&crate::media::Track::default(), true);
        track.title = "Automatically queued track".into();
        state.observe_track(&track, true);
        assert_eq!(state.page(None, true, true, 0), 1);
        track.playing = false;
        state.observe_track(&track, true);
        track.playing = true;
        state.observe_track(&track, true);
        assert_eq!(state.page(None, true, true, 0), 0);
    }
    #[test]
    fn last_activated_activity_owns_the_icon_and_pauses_fall_back() {
        let mut state = super::CompactActivity::default();
        state.observe(true, false);
        assert_eq!(state.page(None, false, true, 2), 0);
        state.observe(true, true);
        assert_eq!(state.page(None, true, true, 2), 1);
        state.observe(true, true); // automatic song/metadata changes keep focus
        assert_eq!(state.page(None, true, true, 2), 1);
        state.observe(false, true);
        assert_eq!(state.page(None, true, false, 2), 1);
        state.observe(true, true);
        assert_eq!(state.page(None, true, true, 2), 0);
        state.observe(true, true); // metadata/next-track refresh is not activation
        assert_eq!(state.page(None, true, true, 2), 0);
        state.observe(true, false);
        state.observe(true, true);
        assert_eq!(state.page(None, true, true, 2), 1);
        let alert = crate::Alert {
            text: "Reminder fixture".into(),
            is_water: true,
            page: 3,
            url: String::new(),
        };
        state.observe_alert(Some(&alert));
        assert_eq!(state.page(Some(3), true, true, 2), 3);
        state.observe(false, true);
        state.observe(true, true);
        state.observe_alert(Some(&alert)); // pending reminders do not lock ownership
        assert_eq!(state.page(Some(3), true, true, 2), 0);
        state.observe(false, false);
        assert_eq!(state.page(None, false, false, 2), 2);
    }
}
