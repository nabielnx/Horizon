//! Native implementation of the approved Figma frames. Positions are relative
//! to the 591px notch, excluding the grey artboard surrounding the design.
use crate::*;
use chrono::Datelike;

fn cover_wave_ink(track: &media::Track) -> u32 {
    track.palette.map_or(0xeb55c0, |palette| {
        let channels = [16, 8, 0]
            .map(|shift| ((((palette[0] >> shift) & 255) as f32 - 8.) / 0.34).clamp(0., 255.));
        let gain = (150. / channels.into_iter().fold(1_f32, f32::max)).max(1.);
        channels
            .into_iter()
            .zip([16, 8, 0])
            .map(|(v, shift)| ((v * gain).min(255.) as u32) << shift)
            .sum()
    })
}

fn idle_lyric(track: &media::Track, enabled: bool) -> &str {
    if !enabled {
        "Lirik nonaktif"
    } else if let Some(line) = track.lyric_index().and_then(|i| track.lyrics.get(i)) {
        line.text.as_str()
    } else if track.title.is_empty() {
        "Horizon"
    } else if !track.lyrics.is_empty() {
        ""
    } else if !track.lyrics_loaded {
        "Menunggu lirik"
    } else {
        "Lirik belum tersedia"
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
impl CompactActivity {
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
        "shell" => include_bytes!("../assets/figma/shell.png"),
        "compact-shell" => include_bytes!("../assets/figma/compact-shell.png"),
        "nav-music" => include_bytes!("../assets/figma/nav-music.png"),
        "nav-focus" => include_bytes!("../assets/figma/nav-focus.png"),
        "nav-calendar" => include_bytes!("../assets/figma/nav-calendar.png"),
        "nav-water" => include_bytes!("../assets/figma/nav-water.png"),
        "nav-tasks" => include_bytes!("../assets/figma/nav-tasks.png"),
        "nav-files" => include_bytes!("../assets/figma/nav-files.png"),
        "nav-settings" => include_bytes!("../assets/figma/nav-settings.png"),
        "media-lyrics" => include_bytes!("../assets/figma/media-lyrics-glyph.png"),
        "media-previous" => include_bytes!("../assets/figma/media-previous.png"),
        "media-pause" => include_bytes!("../assets/figma/media-pause-glyph.png"),
        "media-next" => include_bytes!("../assets/figma/media-next.png"),
        "media-heart" => include_bytes!("../assets/figma/media-heart.png"),
        "peek-pause" => include_bytes!("../assets/figma/peek-pause-glyph.png"),
        "spotify" => include_bytes!("../assets/figma/spotify.png"),
        "repeat" => include_bytes!("../assets/figma/repeat.png"),
        "fire" => include_bytes!("../assets/figma/fire.png"),
        "sidebar-focus" => include_bytes!("../assets/figma/sidebar-focus.png"),
        "sidebar-calendar" => include_bytes!("../assets/figma/sidebar-calendar.png"),
        "calendar-back" => include_bytes!("../assets/figma/calendar-back.png"),
        "calendar-next" => include_bytes!("../assets/figma/calendar-next.png"),
        "edit-event" => include_bytes!("../assets/figma/edit-event.png"),
        "delete-event" => include_bytes!("../assets/figma/delete-event.png"),
        _ => return,
    };
    unsafe {
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
    unsafe fn figma_lyric(
        &self,
        c: &Canvas,
        track: &media::Track,
        enabled: bool,
        b: (f32, f32, f32, f32),
        style: (f32, u16, u8),
    ) {
        unsafe {
            if enabled && let Some(index) = track.lyric_index() {
                let line = &track.lyrics[index];
                let end = track.lyrics.get(index + 1).map_or(track.duration, |l| l.at);
                c.design_karaoke(line, (track.position_now(), end), b, style, true);
            } else {
                c.design_text(idle_lyric(track, enabled), b, style, 0xffffff);
            }
        }
    }
    pub(super) fn figma_transform(&self) -> (f32, f32) {
        let width = self.shared.borrow().data.settings.width as f32;
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
            let progress = if track.duration > 0. {
                (track.position_now() / track.duration).clamp(0., 1.) as f32
            } else {
                0.
            };
            let motion = state.data.settings.motion;
            let rail_h = if !self.expanded && self.mode == shell_policy::Mode::Compact {
                2.5
            } else {
                4.
            };
            let rail_center = b.1 + b.3 - 3.5;
            c.mini_wave_tinted(
                (
                    b.0 - 2.,
                    rail_center - 5. - rail_h / 2.,
                    b.2 + 4.,
                    (b.3 - 6.).clamp(2.2, 14.),
                ),
                progress,
                if motion {
                    track.position_now() * 1.8
                } else {
                    0.
                },
                motion && track.playing,
                Some(cover_wave_ink(track)),
                rail_h,
            );
            let thumb = if rail_h > 2.5 { 11. } else { 7. };
            c.ellipse(
                b.0 + b.2 * progress - thumb / 2.,
                rail_center - thumb / 2.,
                thumb,
                thumb,
                0xffffff,
            );
            let hit_top = b.1.min(rail_center - thumb / 2.);
            let hit_bottom = (b.1 + b.3).max(rail_center + thumb / 2.);
            self.figma_hit((b.0, hit_top, b.2, hit_bottom - hit_top), Action::Seek);
        }
    }
    unsafe fn figma_peek_playback(&mut self, c: &Canvas, track: &media::Track) {
        unsafe {
            let b = (518., 29., 40., 39.);
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
    fn figma_hit(&mut self, b: (f32, f32, f32, f32), action: Action) {
        let (scale, offset) = self.figma_transform();
        self.hit(
            (b.0 * scale + offset).round() as i32,
            b.1.round() as i32,
            (b.2 * scale).round() as i32,
            b.3.round() as i32,
            action,
        );
    }
    fn figma_hover(&self, b: (f32, f32, f32, f32)) -> bool {
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
                if name == "media-heart" {
                    c.icon_filled("heart", b.0 + 12., b.1 + 12. + dy, 26., 0xffffff);
                } else {
                    asset(c, name, (b.0, b.1 + dy, b.2, b.3));
                }
                return;
            }
            let hit = if b.2 <= 18. && b.3 <= 18. {
                (b.0 - (24. - b.2) / 2., b.1 - (24. - b.3) / 2., 24., 24.)
            } else {
                b
            };
            let hover = self.figma_hover(hit);
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
    pub(super) unsafe fn draw_figma(&mut self, c: &Canvas, state: &Shared) -> bool {
        unsafe {
            let s = &state.data.settings;
            let width = s.width as f32;
            let track = state
                .media
                .as_ref()
                .and_then(|m| m.snapshot.lock().ok().map(|t| t.clone()))
                .unwrap_or_default();
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
                    let left = (591. - 310.) / 2.;
                    if s.album_colors && track.palette.is_some() {
                        c.design_glow_at(
                            (left, 0., 310., 50.),
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
                    let icon_bounds = (left + 24., 10., 29., 29.);
                    let mut cover_drawn = false;
                    let dy = c.glass_content(icon_bounds, 8., self.figma_hover(icon_bounds), |b| {
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
                    });
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
                            22.,
                        );
                    }
                    if let Some(notice) = &self.power_notice {
                        label(
                            c,
                            &notice.text(),
                            (left + 70., 11., 175.),
                            (10.5, 600, 0xffffff, 1),
                        );
                    } else if let Some(alert) = &state.alert {
                        label(
                            c,
                            &alert.text,
                            (left + 70., 11., 175.),
                            (12., 700, 0xffffff, 1),
                        );
                    } else {
                        self.figma_lyric(
                            c,
                            &track,
                            s.show_lyrics,
                            (left + 70., 11., 175., 21.),
                            (12., 700, 1),
                        );
                    }
                    self.figma_idle_wave(c, state, &track, (left + 70., 29., 175., 12.));
                    let b = (left + 257., 9., 32., 30.);
                    let hover = c.hover_amount(b, self.figma_hover(b));
                    c.compact_status(
                        b,
                        hover,
                        self.battery,
                        if s.motion {
                            self.battery_phase.elapsed().as_secs_f32()
                        } else {
                            0.
                        },
                    );
                    self.figma_hit(b, Action::OpenAlert);
                    self.figma_hit((left, 0., 310., 50.), Action::OpenAlert);
                } else if track.playing {
                    if s.album_colors {
                        c.design_glow(101., self.palette.sample(Instant::now())[0]);
                    }
                    self.figma_cover(c, &track, (28., 14., 64., 64.));
                    label(c, &track.title, (108., 22., 160.), (16., 600, 0xffffff, 0));
                    label(c, &track.artist, (108., 50., 160.), (12., 600, 0xffffff, 0));
                    c.rect(276., 30., 1., 41., 0., 0x434343);
                    self.figma_lyric(
                        c,
                        &track,
                        s.show_lyrics,
                        (292., 27., 196., 20.),
                        (11.5, 600, 0),
                    );
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
            if self.page == 0 && s.album_colors && track.palette.is_some() {
                c.design_glow_at(
                    (0., 0., width.max(591.), 270.),
                    self.palette.sample(Instant::now())[0],
                );
                if s.show_lyrics {
                    c.lyric_glow(
                        (382., 0., width.max(591.) - 402., host_height() as f32),
                        self.palette.sample(Instant::now()),
                        self.lyric_glow_phase,
                        self.lyric_glow_seed,
                    );
                }
            }
            self.figma_sidebar(c);
            match self.page {
                0 => self.figma_music(c, state, &track),
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
    unsafe fn figma_cover(&self, c: &Canvas, track: &media::Track, b: (f32, f32, f32, f32)) {
        unsafe {
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
    pub(super) unsafe fn figma_sidebar(&mut self, c: &Canvas) {
        unsafe {
            let top = 16.;
            let bottom = (host_height() - 64) as f32;
            let scroll = self.sidebar_scroll.clamp(
                0.,
                sidebar_max_scroll(self.page, &self.shared.borrow().data.settings),
            );
            c.push_design_clip((8., top, 54., bottom - top));
            for (i, icon) in [
                "music-notes",
                "clock",
                "calendar-dots",
                "drop",
                "list-checks",
                "files",
            ]
            .into_iter()
            .enumerate()
            {
                let y = 20. + i as f32 * 48. - scroll;
                let visible_top = y.max(top);
                let visible_bottom = (y + 40.).min(bottom);
                if visible_bottom <= visible_top {
                    continue;
                }
                let hit = (10., visible_top, 40., visible_bottom - visible_top);
                let hover = c.hover_amount((10., y, 40., 40.), self.figma_hover(hit));
                // Key selection to the page, not its scrolling pixel position.
                let selection = c.animate_value(
                    (i as f32, -48., 40., 40.),
                    2,
                    f32::from(i == self.page),
                    motion::STATE,
                );
                let press = c.animate_value(
                    (10., y, 40., 40.),
                    1,
                    f32::from(self.pressed && self.figma_hover(hit)),
                    motion::PRESS,
                );
                let dy = if self.shared.borrow().data.settings.motion {
                    -hover + press * 1.5
                } else {
                    0.
                };
                c.glass_panel(
                    (10., y + dy, 40., 40.),
                    13.,
                    motion::mix(
                        motion::mix(0x242424, 0x414141, selection),
                        0x4a4a4a,
                        hover * 0.65,
                    ),
                    hover,
                );
                c.icon(icon, 16., y + 6. + dy, 28., 0xffffff);
                self.figma_hit(hit, Action::Page(i));
            }
            c.pop_design_clip();
            let settings_y = (host_height() - 52) as f32;
            c.rect(12., settings_y - 8., 36., 0.7, 0., 0x555555);
            let settings_bounds = (10., settings_y, 40., 40.);
            c.liquid_glass(
                10.,
                settings_y,
                40.,
                40.,
                13.,
                None,
                self.figma_hover(settings_bounds),
                false,
            );
            c.icon("sliders-horizontal", 17., settings_y + 7., 26., 0xffffff);
            self.figma_hit(settings_bounds, Action::Settings);
            let viewport = bottom - 20.;
            let track_h = bottom - top;
            let thumb_h = (track_h * viewport / 280.).clamp(24., track_h);
            let max_scroll =
                sidebar_max_scroll(self.page, &self.shared.borrow().data.settings).max(1.);
            c.rect(58., top, 1., track_h, 0.5, 0x434343);
            c.rect(
                58.,
                top + (track_h - thumb_h) * scroll / max_scroll,
                1.,
                thumb_h,
                0.5,
                0xb8b8b8,
            );
        }
    }
    unsafe fn figma_music(&mut self, c: &Canvas, state: &Shared, track: &media::Track) {
        unsafe {
            self.figma_cover(c, track, (73., 20., 91., 89.));
            let title = if track.title.is_empty() {
                "Tidak ada musik"
            } else {
                &track.title
            };
            let title_h = c
                .design_measure(title, 175., (18., 700, 3))
                .1
                .ceil()
                .clamp(27., 54.);
            c.design_text(title, (176., 22., 175., title_h), (18., 700, 3), 0xffffff);
            let artist_y = 22. + title_h + 6.;
            label(
                c,
                &track.artist,
                (176., artist_y, 175.),
                (12., 400, 0xb7bec8, 0),
            );
            let source_y = artist_y + 25.;
            let spotify = track.source_app.to_lowercase().contains("spotify");
            if track.cover_source_url.is_some() {
                c.icon("external-link", 176., source_y, 18., 0xffffff);
            } else if spotify {
                asset(c, "spotify", (176., source_y, 18., 18.));
            } else {
                c.icon("headphones", 176., source_y, 18., 0xffffff);
            }
            label(
                c,
                if track.cover_source_url.is_some() {
                    "Cover Apple Music ↗"
                } else if spotify {
                    "Play on Spotify"
                } else {
                    "Media Windows"
                },
                (200., source_y + 1., 151.),
                (10., 600, 0xffffff, 0),
            );
            self.figma_hit(
                (176., source_y, 175., 20.),
                if track.cover_source_url.is_some() {
                    Action::CoverSource
                } else {
                    Action::Media(0)
                },
            );
            self.figma_idle_wave(c, state, track, (73., 141., 278., 25.));
            label(
                c,
                &crate::ui::clock(track.position_now()),
                (73., 173., 80.),
                (11., 600, 0xffffff, 0),
            );
            label(
                c,
                &crate::ui::clock(track.duration),
                (308., 173., 43.),
                (11., 600, 0xffffff, 1),
            );
            self.figma_asset_button(
                c,
                "media-lyrics",
                (73., 204., 50., 50.),
                Action::Setting(16, 1),
            );
            self.figma_asset_button(
                c,
                "media-previous",
                (130., 204., 50., 50.),
                Action::Media(1),
            );
            if track.playing {
                self.figma_asset_button(c, "media-pause", (187., 204., 50., 50.), Action::Media(0));
            } else {
                let dy = self.figma_plate(c, (187., 204., 50., 50.), 17., Action::Media(0));
                c.icon_centered("play", (187., 204. + dy, 50., 50.), 24.);
            }
            self.figma_asset_button(c, "media-next", (244., 204., 50., 50.), Action::Media(2));
            let favorite = state
                .data
                .favorites
                .contains(&format!("{}|{}", track.title, track.artist));
            if favorite {
                self.figma_asset_button(c, "media-heart", (301., 204., 50., 50.), Action::Favorite);
            } else {
                let dy = self.figma_plate(c, (301., 204., 50., 50.), 17., Action::Favorite);
                c.icon("heart", 313., 216. + dy, 26., 0xffffff);
            }
            c.rect(365., 25., 1., 230., 0.5, 0x343a43);
            if state.data.settings.show_lyrics && !track.lyrics.is_empty() {
                let active_index = track.lyric_index();
                let current = track
                    .lyrics
                    .partition_point(|line| line.at <= track.position_now())
                    .saturating_sub(1);
                let start = current.saturating_sub(14);
                let end = (current + 15).min(track.lyrics.len());
                let bounds = (
                    382.,
                    25.,
                    (state.data.settings.width as f32).max(591.) - 402.,
                    230.,
                );
                let mut active_size = 15.;
                if c.design_measure(&track.lyrics[current].text, bounds.2, (active_size, 700, 2))
                    .1
                    > 140.
                {
                    active_size = 13.5;
                }
                let heights: Vec<f32> = (start..end)
                    .map(|index| {
                        let active = active_index == Some(index);
                        let size = if active { active_size } else { 11.5 };
                        c.design_measure(
                            &track.lyrics[index].text,
                            bounds.2,
                            (size, if active { 700 } else { 400 }, 2),
                        )
                        .1
                        .ceil()
                        .min(if active { bounds.3 } else { size * 4.5 })
                    })
                    .collect();
                let rows = layout::lyric_rows(&heights, current - start, bounds.1, bounds.3, 10.);
                c.push_design_clip(bounds);
                for row in rows {
                    let index = start + row.index;
                    let active = active_index == Some(index);
                    let distance = index.abs_diff(current).saturating_sub(1) as f32;
                    if active {
                        let end = track
                            .lyrics
                            .get(index + 1)
                            .map_or(track.duration, |line| line.at);
                        c.design_karaoke(
                            &track.lyrics[index],
                            (track.position_now(), end),
                            (bounds.0, row.y, bounds.2, row.height),
                            (active_size, 700, 2),
                            false,
                        );
                    } else {
                        c.design_text(
                            &track.lyrics[index].text,
                            (bounds.0, row.y, bounds.2, row.height),
                            (
                                if active { active_size } else { 11.5 },
                                if active { 700 } else { 400 },
                                2,
                            ),
                            if active {
                                0xffffff
                            } else {
                                motion::mix(0xa8b0bc, 0x68717e, distance / 5.)
                            },
                        );
                    }
                }
                c.pop_design_clip();
            } else {
                label(
                    c,
                    if !state.data.settings.show_lyrics {
                        "Lirik nonaktif"
                    } else if !track.lyrics_loaded {
                        "Mencari lirik"
                    } else {
                        "Lirik belum tersedia"
                    },
                    (
                        382.,
                        128.,
                        (state.data.settings.width as f32).max(591.) - 402.,
                    ),
                    (12., 600, 0x777777, 1),
                );
            }
        }
    }
    unsafe fn figma_focus(&mut self, c: &Canvas, state: &Shared) {
        unsafe {
            let s = &state.data.settings;
            let extra = (s.width as f32 - 591.).max(0.);
            let center = extra / 2.;
            let left = remaining(state);
            let fraction = left as f32 / (s.focus_minutes * 60).max(1) as f32;
            let fraction =
                c.animate_value((187. + center, 20., 249., 169.), 4, fraction, motion::STATE);
            c.rect(187. + center, 20., 249., 169., 30., 0x191919);
            let time = state
                .running
                .filter(|_| s.motion)
                .map_or(0., |start| start.elapsed().as_secs_f32());
            c.liquid_level((187. + center, 20., 249., 169.), fraction, time, 0xff9317);
            c.glass_edge(187. + center, 20., 249., 169., 30., 0.);
            label(
                c,
                &format!("{:02}.{:02}", left / 60, left % 60),
                (223. + center, 22., 178.),
                (64., 600, 0xffffff, 1),
            );
            label(
                c,
                &format!("{} minutes", s.focus_minutes),
                (250. + center, 125., 128.),
                (13., 600, 0xffffff, 1),
            );
            for (minutes, x, y, label_text) in [
                (15, 73., 28., "15 minutes"),
                (20, 73., 80., "20 minutes"),
                (45, 73., 132., "45 minutes"),
                (60, 454. + extra, 29., "1 Hours"),
                (90, 454. + extra, 80., "1.5 Hours"),
                (120, 454. + extra, 132., "2 Hours"),
            ] {
                let dy =
                    self.figma_plate(c, (x, y, 100., 40.), 14., Action::SetFocusMinutes(minutes));
                label(
                    c,
                    label_text,
                    (x, y + 10. + dy, 100.),
                    (13., 600, 0xffffff, 1),
                );
            }
            let dy = self.figma_plate(c, (251. + center, 147., 88., 32.), 9., Action::Focus);
            label(
                c,
                if state.running.is_some() {
                    "Pause"
                } else if left == 0 {
                    "Play"
                } else if left < (s.focus_minutes * 60) as u64 {
                    "Lanjut"
                } else {
                    "Play"
                },
                (251. + center, 153. + dy, 88.),
                (14., 600, 0xffffff, 1),
            );
            let dy = self.figma_plate(c, (343. + center, 147., 36., 32.), 9., Action::Reset);
            asset(c, "repeat", (350. + center, 152. + dy, 22., 23.));
            let today = Local::now().date_naive();
            let seconds = state
                .data
                .focus_history
                .get(&today.to_string())
                .copied()
                .unwrap_or(0);
            let mut streak = 0;
            let mut day = today;
            if seconds == 0 {
                day -= chrono::Duration::days(1);
            }
            while state
                .data
                .focus_history
                .get(&day.to_string())
                .copied()
                .unwrap_or(0)
                > 0
            {
                streak += 1;
                day -= chrono::Duration::days(1);
            }
            label(
                c,
                &format!("Today: {}h {}m", seconds / 3600, (seconds % 3600) / 60),
                (184. + center, 207., 139.),
                (15., 600, 0xffffff, 0),
            );
            c.ellipse(304. + center, 219., 4., 4., 0xffffff);
            asset(c, "fire", (324. + center, 207., 24., 22.));
            label(
                c,
                &format!("Streak : {streak}d"),
                (351. + center, 207., 170.),
                (15., 600, 0xffffff, 0),
            );
            c.rect(76., 238., 480. + extra, 26., 10., 0x191919);
            let week: u64 = (0..7)
                .map(|i| {
                    state
                        .data
                        .focus_history
                        .get(&(today - chrono::Duration::days(i)).to_string())
                        .copied()
                        .unwrap_or(0)
                })
                .sum();
            let progress = (week as f32 / (s.focus_minutes * 60 * 7).max(1) as f32).clamp(0., 1.);
            if progress > 0. {
                c.rect(76., 238., (480. + extra) * progress, 26., 10., 0xff9317);
            }
            c.glass_edge(76., 238., 480. + extra, 26., 10., 0.);
            label(
                c,
                "1 week",
                (76., 242., 480. + extra),
                (12., 600, 0xffffff, 1),
            );
        }
    }
    unsafe fn figma_calendar(&mut self, c: &Canvas, state: &Shared) {
        unsafe {
            let extra = (state.data.settings.width as f32 - 591.).max(0.);
            let agenda_w = 171. + extra;
            c.rect(72., 20., 293., 244., 21., 0x191919);
            c.glass_edge(72., 20., 293., 244., 21., 0.);
            c.rect(375., 20., 197. + extra, 244., 21., 0x1e1226);
            c.glass_edge(375., 20., 197. + extra, 244., 21., 0.);
            label(
                c,
                calendar::MONTHS[self.calendar.month.month0() as usize],
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
                    &day.to_uppercase(),
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
                "Hari ini",
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
                    calendar::DAYS_SHORT
                        [self.calendar.selected.weekday().num_days_from_monday() as usize],
                    self.calendar.selected.day()
                ),
                (388., 37., 94. + extra),
                (15., 600, 0xffffff, 0),
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
                label(c, &time, (388., y + 3., 36.), (10., 600, 0xc878fe, 0));
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
                    0xffffff,
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
                    (8.5, 400, 0xa299ad, 0),
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
                    "Belum ada acara",
                    (388., 87., agenda_w),
                    (10., 400, 0x888888, 0),
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
                "+ Acara",
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
                    "Ubah acara"
                } else {
                    "Tambah acara"
                },
                (388., 29., editor_w),
                (14., 600, 0xffffff, 0),
            );
            for (text, x, y, w) in [
                ("Nama acara", 388., 56., editor_w),
                ("Jam", 388., 108., 81. + extra / 2.),
                ("Pengingat (mnt)", 478. + extra / 2., 108., 81. + extra / 2.),
                ("Link (opsional)", 388., 160., editor_w),
            ] {
                label(c, text, (x, y, w), (9.5, 400, 0xb5aebe, 0));
            }
            let dy = self.figma_plate(c, (388., 226., 108. + extra, 30.), 9., Action::AddEvent);
            label(
                c,
                if editing { "Perbarui" } else { "Simpan" },
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
                "Batal",
                (504. + extra, 233. + dy, 55.),
                (10., 400, 0xffffff, 1),
            );
            if !self.calendar.status.is_empty() {
                c.design_text(
                    &self.calendar.status,
                    (388., 209., editor_w, 16.),
                    (9.5, 400, 3),
                    0xb9afc5,
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
            let date = Local::now().format("%A, %-d %B %Y").to_string();
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
                remaining(state) as f32 / (state.data.settings.focus_minutes * 60).max(1) as f32,
                0.,
                0xff9317,
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
                &format!("{} minutes", state.data.settings.focus_minutes),
                (410., 56., 78.),
                (5., 600, 0xffffff, 1),
            );
            c.rect(426., 66., 46., 13., 6.5, 0x1affffff);
            c.outline(426., 66., 46., 13., 6.5, 0x35ffffff);
            label(
                c,
                if state.running.is_some() {
                    "Stop"
                } else {
                    "Play"
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
