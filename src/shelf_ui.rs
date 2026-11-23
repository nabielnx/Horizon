use crate::*;

impl Window {
    pub(super) unsafe fn draw_shelf(&mut self, c: &Canvas, state: &Shared) {
        unsafe {
            let width = state.data.settings.width;
            let x = 70.;
            let cw = (width - 88) as f32;
            let count = state.data.shelf_files.len();
            let narrow = cw < 400.;
            if narrow && self.shelf_detail && count > 0 {
                self.icon_button(c, "chevron-left", 70, 25, Action::ShelfList, false);
            }
            c.text(
                if state.status == "Berkas tidak tersedia" {
                    ""
                } else {
                    i18n::tr("Berkas")
                },
                if narrow && self.shelf_detail && count > 0 {
                    116.
                } else {
                    x
                },
                36.,
                112.,
                14.,
                true,
                INK,
            );
            let error = state.status == "Berkas tidak tersedia";
            let badge = if error {
                i18n::tr("Berkas tidak tersedia").into()
            } else if count == 0 {
                "Staging Ready".into()
            } else {
                i18n::format!("{} berkas", "{} files", count)
            };
            let badge_w = if error {
                122.
            } else if count == 0 {
                104.
            } else {
                76.
            };
            let badge_x = x + cw - badge_w - if count > 0 { 84. } else { 0. };
            c.rect(badge_x, 33., badge_w, 24., 6., 0x191f26);
            c.center(
                &badge,
                badge_x,
                39.,
                badge_w,
                9.5,
                true,
                if error { 0xf39bb0 } else { 0xeec16c },
            );
            if count > 0 {
                let clear_x = width - 96;
                let hover = self.pointer.0 >= clear_x
                    && self.pointer.0 < clear_x + 78
                    && (33..57).contains(&self.pointer.1);
                let light = c.hover_amount((clear_x as f32, 33., 78., 24.), hover);
                c.rect(
                    clear_x as f32,
                    33.,
                    78.,
                    24.,
                    6.,
                    motion::mix(0x1b222a, 0x333a43, light),
                );
                c.center(
                    i18n::tr("Kosongkan"),
                    clear_x as f32,
                    39.,
                    78.,
                    9.5,
                    true,
                    0xbac4ce,
                );
                self.hit(clear_x, 33, 78, 24, Action::ClearShelfFiles);
            }
            if count == 0 {
                c.rect(x, 68., cw, 196., 12., 0x13191f);
                c.outline(x, 68., cw, 196., 12., 0x303841);
                let mid = x + cw / 2.;
                c.polygon(
                    &[
                        (mid - 25., 112.),
                        (mid - 8., 112.),
                        (mid - 1., 119.),
                        (mid + 25., 119.),
                        (mid + 25., 144.),
                        (mid - 25., 144.),
                    ],
                    0xa47734,
                );
                c.rect(mid - 25., 122., 50., 27., 6., 0xe9b959);
                c.line((mid - 18., 125.), (mid + 17., 125.), 1., 0xffdf9f);
                c.center(i18n::tr("Drop berkas"), x, 167., cw, 13., true, INK);
                let chips = ["IMG", "DOC", "PDF", "ZIP", "CODE"];
                let start = x + (cw - 230.) / 2.;
                for (i, label) in chips.iter().enumerate() {
                    let xx = start + i as f32 * 47.;
                    c.rect(xx, 207., 42., 18., 5., 0x1e252b);
                    c.center(label, xx, 211., 42., 8.5, false, 0xa4afba);
                }
                return;
            }
            let show_list = !narrow || !self.shelf_detail;
            let left_w = if narrow { cw } else { (cw * 0.44).min(206.) };
            let files = &state.data.shelf_files;
            let index = self.shelf_selected.min(count - 1);
            let file = &files[index];
            let detail = (!narrow || self.shelf_detail).then_some(file.path.as_str());
            let can_preview = detail.is_some_and(shelf::supports_preview);
            let preview_open = can_preview && self.shelf_preview_open;
            let preview_path = detail.filter(|_| preview_open);
            let rows = files
                .iter()
                .enumerate()
                .skip(self.list_offset)
                .take(4)
                .filter(|_| show_list);
            self.preview_worker
                .get_or_insert_with(|| shelf::Worker::new(self.preview_notify.clone()))
                .request(
                    preview_path,
                    detail.into_iter().chain(
                        rows.clone()
                            .map(|(_, file)| file.path.as_str())
                            .filter(|path| Some(*path) != detail),
                    ),
                );
            if show_list {
                for (row, (i, file)) in rows.enumerate() {
                    let entrance = if state.data.settings.motion {
                        self.feedback
                            .filter(|(p, _)| *p == 5)
                            .map(|(_, start)| {
                                8. * (1.
                                    - motion::ease(
                                        (start.elapsed().as_secs_f32() / 0.35).clamp(0., 1.),
                                    ))
                            })
                            .unwrap_or(0.)
                    } else {
                        0.
                    };
                    let y = 68. + row as f32 * 45. + entrance;
                    let selected = self.shelf_selected == i;
                    let hover = self.pointer.0 >= x as i32
                        && self.pointer.0 < (x + left_w) as i32
                        && self.pointer.1 >= y as i32
                        && self.pointer.1 < y as i32 + 40;
                    let light = c.hover_amount((x, y, left_w, 40.), hover);
                    let selected =
                        c.animate_value((x, y, left_w, 40.), 2, f32::from(selected), motion::STATE);
                    c.rect(
                        x,
                        y,
                        left_w,
                        40.,
                        8.,
                        motion::mix(motion::mix(0x29241c, 0x352d22, light), 0x453722, selected),
                    );
                    if selected > 0.001 {
                        c.outline(x, y, left_w, 40., 8., 0x45515e);
                        c.rect(
                            x + 4.,
                            y + 20. - 8. * selected,
                            2.,
                            16. * selected,
                            1.,
                            0xeec16c,
                        );
                    }
                    c.glass_edge(x, y, left_w, 40., 8., light);
                    let native = self
                        .preview_worker
                        .as_ref()
                        .and_then(|worker| worker.icon(&file.path))
                        .is_some_and(|icon| {
                            c.file_icon(&file.path, icon, (x + 10., y + 10., 20.))
                                .is_ok()
                        });
                    if !native {
                        let ext = std::path::Path::new(&file.name)
                            .extension()
                            .and_then(|s| s.to_str())
                            .unwrap_or("")
                            .to_lowercase();
                        let (icon, ink, _) = crate::ui::file_category(&ext);
                        c.icon(icon, x + 12., y + 12., 16., ink);
                    }
                    c.text(&file.name, x + 36., y + 6., left_w - 48., 11., true, INK);
                    c.text(
                        &crate::ui::format_file_size(file.size),
                        x + 36.,
                        y + 23.,
                        left_w - 48.,
                        9.,
                        false,
                        MUTED,
                    );
                    self.hit(
                        x as i32,
                        y as i32,
                        left_w as i32,
                        40,
                        Action::SelectShelfFile(i),
                    );
                }
                if count > 4 {
                    c.center(
                        &format!(
                            "{}–{} / {}",
                            self.list_offset + 1,
                            (self.list_offset + 4).min(count),
                            count
                        ),
                        x,
                        250.,
                        left_w,
                        9.,
                        false,
                        MUTED,
                    );
                }
            }
            if narrow && !self.shelf_detail {
                return;
            }
            let right_x = if narrow { x } else { x + left_w + 14. };
            let right_w = if narrow { cw } else { cw - left_w - 14. };
            c.rect(right_x, 68., right_w, 196., 11., 0x25221c);
            c.outline(right_x, 68., right_w, 196., 11., 0x303b46);
            c.glass_edge(right_x, 68., right_w, 196., 11., 0.);
            let side = (right_w - 112.).min(176.);
            let p_x = right_x + 10.;
            let p_y = 68. + (196. - side) / 2.;
            c.rect(p_x, p_y, side, side, 8., 0x0b1015);
            let preview = preview_path.and_then(|path| {
                self.preview_worker
                    .as_ref()
                    .and_then(|worker| worker.preview(path))
            });
            let mut drawn = false;
            match preview {
                Some(shelf::Preview::Image(bytes)) => {
                    drawn = c
                        .image_preview(
                            &file.path,
                            bytes,
                            p_x + 5.,
                            p_y + 5.,
                            side - 10.,
                            side - 10.,
                            0.,
                        )
                        .is_ok();
                }
                Some(shelf::Preview::Text(text)) => {
                    for (i, line) in text.lines().take(6).enumerate() {
                        c.text(
                            line,
                            p_x + 8.,
                            p_y + 6. + i as f32 * 15.,
                            side - 16.,
                            9.,
                            false,
                            0xaebdc9,
                        );
                    }
                    drawn = !text.is_empty();
                }
                _ => {}
            }
            if !drawn {
                let ext = std::path::Path::new(&file.name)
                    .extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                let (icon, _, label) = crate::ui::file_category(&ext);
                let size = (side * 0.45).min(72.);
                let icon_x = p_x + (side - size) / 2.;
                let icon_y = p_y + (side - size - 18.) / 2.;
                let native = self
                    .preview_worker
                    .as_ref()
                    .and_then(|worker| worker.icon(&file.path))
                    .is_some_and(|image| {
                        c.file_icon(&file.path, image, (icon_x, icon_y, size))
                            .is_ok()
                    });
                if !native {
                    c.icon_centered(icon, (icon_x, icon_y, size, size), 29.);
                }
                c.center(
                    if !preview_open {
                        label
                    } else if preview.is_none() {
                        i18n::tr("Memuat")
                    } else {
                        i18n::tr("Preview gagal")
                    },
                    p_x,
                    icon_y + size + 7.,
                    side,
                    9.,
                    false,
                    MUTED,
                );
            }
            let info_x = p_x + side + 10.;
            let info_w = right_x + right_w - 10. - info_x;
            if assistant::supports_file(std::path::Path::new(&file.path))
                && !self.assistant.waiting()
            {
                self.mini_button(
                    c,
                    i18n::tr("Tanya AI"),
                    (p_x + side - 76.) as i32,
                    (p_y + side - 30.) as i32,
                    70,
                    24,
                    Action::AiFromShelf(index),
                    false,
                    None,
                );
            }
            c.design_text(&file.name, (info_x, 84., info_w, 38.), (12., 600, 3), INK);
            c.text(
                &crate::ui::format_file_size(file.size),
                info_x,
                126.,
                info_w,
                9.5,
                false,
                MUTED,
            );
            if can_preview {
                self.mini_button(
                    c,
                    i18n::tr(if preview_open {
                        "Tutup preview"
                    } else {
                        "Lihat preview"
                    }),
                    info_x as i32,
                    142,
                    info_w.min(116.) as i32,
                    22,
                    Action::ToggleShelfPreview(index),
                    !preview_open,
                    None,
                );
            } else {
                let badge_w = info_w.min(116.);
                c.rect(info_x, 142., badge_w, 22., 7., 0x191f26);
                c.center(
                    i18n::tr("Ikon saja"),
                    info_x,
                    147.,
                    badge_w,
                    10.,
                    false,
                    MUTED,
                );
            }
            for (i, (icon, action)) in [
                ("external-link", Action::OpenShelfFile(index)),
                ("folder", Action::OpenFileFolder(index)),
                ("copy", Action::CopyFilePath(index)),
                ("x", Action::RemoveShelfFile(index)),
            ]
            .into_iter()
            .enumerate()
            {
                self.icon_button(
                    c,
                    icon,
                    (info_x + (i % 2) as f32 * 44.) as i32,
                    170 + (i / 2) as i32 * 44,
                    action,
                    i == 0,
                );
            }
        }
    }
}
