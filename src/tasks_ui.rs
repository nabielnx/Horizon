use crate::*;

impl Window {
    pub(super) unsafe fn draw_tasks(&mut self, c: &Canvas, state: &Shared) {
        unsafe {
            let width = state.data.settings.width;
            let (list, summary) = input::task_columns(width);
            let x = list.x as f32;
            let lw = list.width as f32;
            let done = state.data.tasks.iter().filter(|task| task.done).count();
            let total = state.data.tasks.len();
            c.rect(
                62.,
                24.,
                (width - 62) as f32,
                (panel_height(&state.data.settings) - 24) as f32,
                0.,
                0x090e0b,
            );
            c.text("Tugas", x, 32., 100., 16., true, 0xf1f5f2);
            c.ellipse(
                x + c.measure_text("Tugas", 16., true) + 7.,
                40.,
                4.,
                4.,
                render::TASK_ACCENT,
            );
            if total == 0 {
                c.rect(x, 64., lw, list.height as f32, 10., 0x17271d);
                c.center("Belum ada tugas", x, 133., lw, 11., false, 0x99afa1);
            }
            for (row, (index, task)) in state
                .data
                .tasks
                .iter()
                .enumerate()
                .skip(self.list_offset)
                .take(4)
                .enumerate()
            {
                let y = 64. + row as f32 * 40.;
                let hover = self.pointer.0 >= list.x
                    && self.pointer.0 < list.x + list.width
                    && self.pointer.1 >= y as i32
                    && self.pointer.1 < y as i32 + 36;
                let light = c.hover_amount((x, y, lw, 36.), hover);
                c.rect(x, y, lw, 36., 10., motion::mix(0x18231e, 0x2b4033, light));
                c.glass_edge(x, y, lw, 36., 10., light);
                let check_x = x + 11.;
                let completion =
                    c.animate_value((x, y, lw, 36.), 3, f32::from(task.done), motion::STATE);
                c.outline(check_x, y + 9.5, 17., 17., 8.5, 0x60776a);
                if completion > 0.001 {
                    let size = 17. * completion;
                    let inset = (17. - size) / 2.;
                    c.ellipse(
                        check_x + inset,
                        y + 9.5 + inset,
                        size,
                        size,
                        render::TASK_ACCENT,
                    );
                    let reveal = ((completion - 0.45) / 0.55).clamp(0., 1.);
                    let first = (reveal / 0.35).min(1.);
                    if reveal > 0.001 {
                        c.line(
                            (check_x + 4.5, y + 18.),
                            (check_x + 4.5 + 3. * first, y + 18. + 3. * first),
                            2.5,
                            0xffffff,
                        );
                    }
                    if reveal > 0.35 {
                        let last = (reveal - 0.35) / 0.65;
                        c.line(
                            (check_x + 7.5, y + 21.),
                            (check_x + 7.5 + 5. * last, y + 21. - 6. * last),
                            2.5,
                            0xffffff,
                        );
                    }
                }
                self.hit(
                    list.x,
                    y as i32,
                    list.width - 26,
                    36,
                    Action::ToggleTask(index),
                );
                let due = task
                    .due
                    .and_then(|at| Local.timestamp_opt(at, 0).single())
                    .filter(|_| list.width >= 220);
                let text_x = x + 38.;
                let text_w = lw - 38. - if due.is_some() { 98. } else { 50. };
                c.text(
                    &task.text,
                    text_x,
                    y + 9.,
                    text_w,
                    11.5,
                    !task.done,
                    motion::mix(0xf1f5f2, 0x899a92, completion),
                );
                if completion > 0.001 {
                    c.line(
                        (text_x, y + 18.),
                        (
                            text_x
                                + c.measure_text(&task.text, 11.5, false).min(text_w) * completion,
                            y + 18.,
                        ),
                        1.,
                        0x71877b,
                    );
                }
                if let Some(due) = due {
                    c.text(
                        &due.format("%H:%M").to_string(),
                        x + lw - 92.,
                        y + 11.,
                        37.,
                        9.5,
                        false,
                        0x91a89b,
                    );
                }
                let (_, ink) = task_priority(task.priority);
                c.ellipse(x + lw - 42., y + 12.5, 11., 11., 0x0b1d12);
                c.ellipse(x + lw - 40., y + 14.5, 7., 7., ink);
                let remove_x = list.x + list.width - 26;
                let delete_hover = hover && self.pointer.0 >= remove_x;
                let delete_light =
                    c.hover_amount((remove_x as f32, y + 4., 23., 28.), delete_hover);
                if delete_light > 0.001 {
                    c.rect(
                        remove_x as f32,
                        y + 4.,
                        23.,
                        28.,
                        6.,
                        motion::mix(
                            motion::mix(0x18231e, 0x2b4033, light),
                            0x2b4935,
                            delete_light,
                        ),
                    );
                }
                c.icon(
                    "x",
                    remove_x as f32 + 6.,
                    y + 12.5,
                    11.,
                    if delete_hover { 0xe5f0e8 } else { 0x245e2d },
                );
                self.hit(remove_x, y as i32, 26, 36, Action::DeleteTask(index));
            }
            self.draw_task_progress(c, summary, done, total);
            let field = input::task_bounds(width);
            c.rect(
                field.x as f32,
                field.y as f32,
                field.width as f32,
                field.height as f32,
                10.,
                input::TASK_FIELD,
            );
            c.outline(
                field.x as f32,
                field.y as f32,
                field.width as f32,
                field.height as f32,
                10.,
                if windows::Win32::UI::Input::KeyboardAndMouse::GetFocus() == self.input {
                    0x658a70
                } else {
                    0x3d5344
                },
            );
            let pill = input::priority_pill(width);
            let (label, ink) = task_priority(self.task_priority);
            let hover = pill.contains(self.pointer.0, self.pointer.1);
            let py = pill.y as f32;
            let light = c.hover_amount((pill.x as f32, py, pill.width as f32, 38.), hover);
            c.rect(
                pill.x as f32,
                py,
                pill.width as f32,
                38.,
                10.,
                motion::mix(
                    0x213328,
                    0x2b4233,
                    if self.priority_open { 1. } else { light },
                ),
            );
            c.outline(
                pill.x as f32,
                py,
                pill.width as f32,
                38.,
                10.,
                if self.priority_open {
                    0x658a70
                } else {
                    0x3d5344
                },
            );
            c.glass_edge(pill.x as f32, py, pill.width as f32, 38., 10., light);
            c.ellipse(pill.x as f32 + 8., py + 17., 4., 4., ink);
            if pill.width >= 64 {
                c.text(
                    label,
                    pill.x as f32 + 17.,
                    py + 12.,
                    (pill.width - 37) as f32,
                    priority_label_size(c, pill.width),
                    false,
                    0xd3e1d8,
                );
            }
            c.icon(
                "chevron-up",
                (pill.x + pill.width - 16) as f32,
                py + 15.,
                8.,
                0xb5cdbe,
            );
            self.hit(
                pill.x,
                pill.y,
                pill.width,
                pill.height,
                Action::TaskPriorityMenu,
            );
            let add_x = width - 56;
            let hover = self.pointer.0 >= add_x
                && self.pointer.0 < add_x + 38
                && (pill.y..pill.y + pill.height).contains(&self.pointer.1);
            let add_y = pill.y as f32;
            let offset = c.control_glass(
                add_x as f32,
                add_y,
                38.,
                38.,
                10.,
                Some(render::TASK_ACCENT),
                hover,
                false,
            );
            let light = c.hover_amount((add_x as f32, add_y, 38., 38.), hover);
            if light > 0.001 {
                c.outline(
                    add_x as f32,
                    add_y + offset,
                    38.,
                    38.,
                    10.,
                    motion::mix(0x31523e, render::TASK_ACCENT, light),
                );
            }
            c.icon(
                "plus",
                add_x as f32 + 10.,
                add_y + 10. + offset,
                18.,
                0xffffff,
            );
            self.hit(add_x, pill.y, 38, 38, Action::AddTask);
            let menu = input::priority_popup(width);
            let reveal = c.animate_value(
                (
                    menu.x as f32,
                    menu.y as f32,
                    menu.width as f32,
                    menu.height as f32,
                ),
                8,
                f32::from(self.priority_open),
                motion::STATE,
            );
            if self.priority_open {
                c.clip_scroll(
                    menu.x as f32 - 2.,
                    (menu.y + menu.height) as f32 - menu.height as f32 * reveal,
                    menu.width as f32 + 4.,
                    menu.height as f32 * reveal + 4.,
                    0.,
                );
                c.rect(
                    menu.x as f32,
                    menu.y as f32 + 3.,
                    menu.width as f32,
                    menu.height as f32,
                    11.,
                    0x70000000,
                );
                c.rect(
                    menu.x as f32,
                    menu.y as f32,
                    menu.width as f32,
                    menu.height as f32,
                    11.,
                    0x192a20,
                );
                c.outline(
                    menu.x as f32,
                    menu.y as f32,
                    menu.width as f32,
                    menu.height as f32,
                    11.,
                    0x4a6853,
                );
                for (i, p) in [1, 0, 2].into_iter().enumerate() {
                    let (label, ink) = task_priority(p);
                    let yy = menu.y + 5 + i as i32 * 32;
                    let hover = (menu.x..menu.x + menu.width).contains(&self.pointer.0)
                        && (yy..yy + 32).contains(&self.pointer.1);
                    let bounds = (menu.x as f32 + 5., yy as f32, menu.width as f32 - 10., 32.);
                    let light = c.hover_amount(bounds, hover);
                    let selection = c.animate_value(
                        bounds,
                        2,
                        f32::from(self.task_priority == p),
                        motion::STATE,
                    );
                    if selection.max(light) > 0.001 {
                        c.rect(
                            menu.x as f32 + 5.,
                            yy as f32,
                            menu.width as f32 - 10.,
                            32.,
                            6.,
                            motion::mix(0x192a20, 0x2d4937, selection.max(light)),
                        );
                    }
                    c.ellipse(menu.x as f32 + 13., yy as f32 + 12.5, 7., 7., ink);
                    c.text(
                        label,
                        menu.x as f32 + 29.,
                        yy as f32 + 9.,
                        66.,
                        10.5,
                        true,
                        0xdce9e1,
                    );
                    if self.task_priority == p {
                        c.icon(
                            "check",
                            (menu.x + menu.width - 25) as f32,
                            yy as f32 + 10.,
                            12.,
                            render::TASK_ACCENT,
                        );
                    }
                    self.hits.insert(
                        i,
                        Hit {
                            x: menu.x + 5,
                            y: yy,
                            w: menu.width - 10,
                            h: 32,
                            action: Action::SetTaskPriority(p),
                        },
                    );
                }
                c.end_clip();
            }
        }
    }

    unsafe fn draw_task_progress(
        &self,
        c: &Canvas,
        bounds: input::Bounds,
        done: usize,
        total: usize,
    ) {
        unsafe {
            let x = bounds.x as f32;
            let w = bounds.width as f32;
            c.rect(x, 64., w, bounds.height as f32, 13., render::TASK_FILL);
            c.outline(x, 64., w, bounds.height as f32, 13., 0x31523e);
            c.glass_edge(x, 64., w, bounds.height as f32, 13., 0.);
            c.text("Progres", x + 12., 76., w - 24., 10.5, false, 0xa0b4a8);
            let numerator = done.to_string();
            let denominator = format!("/{}", total);
            let measured = c.measure_text(&numerator, 40., true)
                + c.measure_text(&denominator, 26., false)
                + 2.;
            let scale = ((w - 24.) / measured).min(1.);
            let size = 40. * scale;
            c.text(
                &numerator,
                x + 12.,
                109. + (40. - size),
                w - 24.,
                size,
                true,
                0xf2faf5,
            );
            let num_w = c.measure_text(&numerator, size, true);
            c.text(
                &denominator,
                x + 14. + num_w,
                127. + (26. - 26. * scale),
                w - 26. - num_w,
                26. * scale,
                false,
                0x93aa9c,
            );
            let segments = total.clamp(1, 6);
            let segment_w = (w - 24. - (segments - 1) as f32 * 4.) / segments as f32;
            let portion = if total == 0 {
                0.
            } else {
                done as f32 / total as f32
            };
            let portion = c.animate_value((x, 186., w, 4.), 4, portion, 0.3);
            for i in 0..segments {
                let xx = x + 12. + i as f32 * (segment_w + 4.);
                c.rect(xx, 186., segment_w, 4., 2., 0x35443d);
                let fill = (portion * segments as f32 - i as f32).clamp(0., 1.);
                if fill > 0. {
                    c.rect(xx, 186., segment_w * fill, 4., 2., render::TASK_ACCENT);
                }
            }
            let caption = if total == 0 {
                "Belum ada".into()
            } else if done == total {
                "Semua selesai".into()
            } else {
                format!("{} tersisa", total - done)
            };
            c.text(
                &caption,
                x + 12.,
                198.,
                w - 24.,
                if w < 100. { 9. } else { 10.5 },
                false,
                0xa0b4a8,
            );
        }
    }

    pub(super) unsafe fn draw_compact_player(&mut self, c: &Canvas, state: &Shared) {
        unsafe {
            let track = state
                .media
                .as_ref()
                .and_then(|m| m.snapshot.lock().ok().map(|s| s.clone()))
                .unwrap_or_default();
            let x = 70.;
            let cw = (state.data.settings.width - 88) as f32;
            if let Some(bytes) = &track.cover {
                let _ = c.cover(
                    &format!("{}|{}|{}", track.title, track.artist, track.cover_revision),
                    bytes,
                    x,
                    26.,
                    48.,
                );
            } else {
                c.rect(x, 26., 48., 48., 10., 0x273642);
                c.icon("music-2", x + 15., 41., 18., 0x9cb8cd);
            }
            c.text(
                if track.title.is_empty() {
                    "Siap diputar"
                } else {
                    &track.title
                },
                x + 60.,
                29.,
                cw - 60.,
                14.,
                true,
                INK,
            );
            c.text(
                if track.artist.is_empty() {
                    "Media Windows"
                } else {
                    &track.artist
                },
                x + 60.,
                52.,
                cw - 60.,
                11.,
                false,
                MUTED,
            );
            let lyric = if state.data.settings.show_lyrics {
                track.lyric_index()
            } else {
                None
            };
            let caption = lyric
                .and_then(|i| track.lyrics.get(i))
                .map(|l| l.text.as_str())
                .unwrap_or(if !track.lyrics_loaded && track.lyrics_retry.is_some() {
                    "Menunggu layanan lirik"
                } else if track.lyrics_loaded {
                    "Lirik tidak tersedia"
                } else {
                    ""
                });
            c.text(caption, x, 88., cw, 15., true, INK);
            if let Some(next) = lyric.and_then(|i| track.lyrics.get(i + 1)) {
                c.text(&next.text, x, 115., cw, 11., false, 0x87939f);
            }
            let fraction = if track.duration > 0. {
                (track.position_now() / track.duration).clamp(0., 1.) as f32
            } else {
                0.
            };
            c.wave(
                x,
                131.,
                cw,
                fraction,
                track.position_now(),
                state.data.settings.motion,
            );
            if track.duration > 0. {
                self.hit(x as i32 + 7, 143, cw as i32 - 14, 22, Action::Seek);
            }
            c.text(
                &crate::ui::clock(track.position_now()),
                x,
                169.,
                60.,
                9.5,
                false,
                MUTED,
            );
            c.text(
                &crate::ui::clock(track.duration),
                x + cw - 36.,
                169.,
                36.,
                9.5,
                false,
                MUTED,
            );
            let start = x + (cw - 5. * 38. - 4. * 8.) / 2.;
            let key = format!("{}|{}", track.title, track.artist);
            for (i, (icon, action, active)) in [
                (
                    "mic-2",
                    Action::Setting(16, 1),
                    state.data.settings.show_lyrics,
                ),
                ("skip-back", Action::Media(1), false),
                (
                    if track.playing { "pause" } else { "play" },
                    Action::Media(0),
                    track.playing,
                ),
                ("skip-forward", Action::Media(2), false),
                (
                    "heart",
                    Action::Favorite,
                    state.data.favorites.contains(&key),
                ),
            ]
            .into_iter()
            .enumerate()
            {
                self.icon_button(
                    c,
                    icon,
                    (start + i as f32 * 46.).round() as i32,
                    190,
                    action,
                    active,
                );
            }
        }
    }
}

fn task_priority(priority: u8) -> (&'static str, u32) {
    match priority {
        1 => ("Penting", 0xff8198),
        2 => ("Ringan", 0x74dfb1),
        _ => ("Sedang", 0xf1bf61),
    }
}

pub(super) unsafe fn priority_label_size(c: &Canvas, width: i32) -> f32 {
    let measured = ["Penting", "Sedang", "Ringan"]
        .iter()
        .map(|text| unsafe { c.measure_text(text, 9.5, false) })
        .fold(0_f32, f32::max);
    9.5 * (((width - 38) as f32) / measured).min(1.)
}
