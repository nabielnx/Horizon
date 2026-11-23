use crate::*;
use windows::Win32::UI::Controls::Dialogs::*;

pub fn prompt_bounds(width: i32) -> input::Bounds {
    input::Bounds {
        x: 72,
        y: 227,
        width: width - 199,
        height: 37,
    }
}

pub const KEY_BOUNDS: input::Bounds = input::Bounds {
    x: 220,
    y: 215,
    width: 450,
    height: 34,
};
pub const MODEL_BOUNDS: input::Bounds = input::Bounds {
    x: 220,
    y: 279,
    width: 450,
    height: 34,
};
pub const URL_BOUNDS: input::Bounds = input::Bounds {
    x: 220,
    y: 343,
    width: 450,
    height: 34,
};

pub unsafe fn sync_edit(hwnd: HWND, w: &mut Window, width: i32, show: bool) {
    let visible = if w.settings {
        w.page == assistant::SETTINGS_PAGE
    } else {
        show && w.page == assistant::PAGE
    };
    if w.settings && visible == w.ai_input_visible {
        return;
    }
    if visible && w.ai_input.is_invalid() {
        let Ok(instance) = (unsafe { GetModuleHandleW(None) }) else {
            return;
        };
        let style = input::style()
            | if w.settings {
                WINDOW_STYLE(0x20)
            } else {
                WINDOW_STYLE(0)
            };
        let Ok(edit) = (unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("EDIT"),
                w!(""),
                style,
                0,
                0,
                100,
                36,
                Some(hwnd),
                None,
                Some(instance.into()),
                None,
            )
        }) else {
            return;
        };
        w.ai_input = edit;
        if w.input_brush.is_invalid() {
            w.input_brush = unsafe { CreateSolidBrush(input::FIELD_COLORREF) };
        }
        let font = if w.input_font.is_invalid() {
            HFONT(unsafe { GetStockObject(DEFAULT_GUI_FONT) }.0)
        } else {
            w.input_font
        };
        unsafe {
            let old = SetWindowLongPtrW(edit, GWLP_WNDPROC, edit_proc as *const () as isize);
            if w.settings {
                w.input_proc = std::mem::transmute::<isize, WNDPROC>(old);
            }
            input::configure(
                edit,
                font,
                i18n::tr(if w.settings {
                    "Tempel API key"
                } else {
                    "Tanya Asisten…"
                }),
                if w.settings { 512 } else { 1500 },
            );
        }
    }
    if w.settings && visible {
        for (slot, cue, limit) in [
            (&mut w.ai_model_input, "ID model", 160),
            (&mut w.ai_url_input, "https://api.example.com/v1", 420),
        ] {
            if slot.is_invalid() {
                let Ok(instance) = GetModuleHandleW(None) else {
                    return;
                };
                let Ok(edit) = CreateWindowExW(
                    WINDOW_EX_STYLE(0),
                    w!("EDIT"),
                    w!(""),
                    input::style(),
                    0,
                    0,
                    100,
                    36,
                    Some(hwnd),
                    None,
                    Some(instance.into()),
                    None,
                ) else {
                    return;
                };
                *slot = edit;
                input::configure(
                    edit,
                    HFONT(GetStockObject(DEFAULT_GUI_FONT).0),
                    i18n::tr(cue),
                    limit,
                );
                SetWindowLongPtrW(edit, GWLP_WNDPROC, edit_proc as *const () as isize);
            }
        }
        refresh_provider(w);
    }
    if !w.ai_input.is_invalid() {
        let bounds = if w.settings {
            KEY_BOUNDS
        } else {
            prompt_bounds(width)
        };
        unsafe { input::place(w.ai_input, bounds, visible, w.dpi) };
        if !visible && w.settings {
            let _ = unsafe { SetWindowTextW(w.ai_input, w!("")) };
            for (edit, bounds) in [
                (w.ai_model_input, MODEL_BOUNDS),
                (w.ai_url_input, URL_BOUNDS),
            ] {
                if !edit.is_invalid() {
                    input::place(edit, bounds, false, w.dpi);
                }
            }
        }
        w.ai_input_visible = visible;
    }
}

pub unsafe fn refresh_provider(w: &mut Window) {
    if w.ai_model_input.is_invalid() || w.ai_url_input.is_invalid() {
        return;
    }
    let settings = &w.shared.borrow().data.settings;
    let _ = SetWindowTextW(w.ai_input, w!(""));
    let _ = SetWindowTextW(
        w.ai_model_input,
        &HSTRING::from(assistant::model(settings, settings.ai_provider)),
    );
    let _ = SetWindowTextW(w.ai_url_input, &HSTRING::from(&settings.ai_endpoint));
    input::place(w.ai_model_input, MODEL_BOUNDS, true, w.dpi);
    input::place(
        w.ai_url_input,
        URL_BOUNDS,
        settings.ai_provider == assistant::CUSTOM,
        w.dpi,
    );
}

fn key_state(w: &mut Window) -> bool {
    *w.assistant.configured.get_or_insert_with(|| {
        !w.offscreen
            && assistant::Connection::credential(&w.shared.borrow().data.settings)
                .and_then(|target| assistant::load_key(&target).ok().flatten())
                .is_some()
    })
}

pub unsafe fn choose_files(hwnd: HWND) -> Vec<std::path::PathBuf> {
    let mut path = vec![0u16; 32768];
    let filter: Vec<u16> =
        "PDF, TXT, Markdown, PNG, JPEG, WebP\0*.pdf;*.txt;*.md;*.png;*.jpg;*.jpeg;*.webp\0\0"
            .encode_utf16()
            .collect();
    let mut dialog = OPENFILENAMEW {
        lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: hwnd,
        lpstrFilter: PCWSTR(filter.as_ptr()),
        lpstrFile: PWSTR(path.as_mut_ptr()),
        nMaxFile: path.len() as u32,
        Flags: OFN_FILEMUSTEXIST
            | OFN_PATHMUSTEXIST
            | OFN_NOCHANGEDIR
            | OFN_NODEREFERENCELINKS
            | OFN_EXPLORER
            | OFN_ALLOWMULTISELECT,
        ..Default::default()
    };
    if !(unsafe { GetOpenFileNameW(&mut dialog) }).as_bool() {
        return Vec::new();
    }
    use std::os::windows::ffi::OsStringExt;
    let mut paths = path
        .split(|unit| *unit == 0)
        .take_while(|part| !part.is_empty());
    let Some(first) = paths.next() else {
        return Vec::new();
    };
    let first = std::path::PathBuf::from(std::ffi::OsString::from_wide(first));
    if let Some(name) = paths.next() {
        std::iter::once(name)
            .chain(paths)
            .take(assistant::MAX_ATTACHMENTS + 1)
            .map(|name| first.join(std::ffi::OsString::from_wide(name)))
            .collect()
    } else {
        vec![first]
    }
}

impl Window {
    pub unsafe fn draw_assistant(&mut self, c: &Canvas, width: i32) {
        if !self.offscreen {
            let path = self
                .shared
                .borrow()
                .store
                .with_file_name("assistant-chats.json");
            self.assistant.load_history(&path);
        }
        let ready = key_state(self);
        let panel = (72., 57., width as f32 - 88., 126.);
        let waiting = self.assistant.waiting();
        unsafe {
            c.text(i18n::tr("Asisten"), 72., 25., 100., 16., true, INK);
            for (name, x, w, action) in [
                ("Riwayat", width - 198, 58, Action::AiHistory),
                ("Chat baru", width - 135, 63, Action::AiClear),
                (
                    "AI",
                    width - 67,
                    51,
                    Action::SettingsAt(assistant::SETTINGS_PAGE),
                ),
            ] {
                self.mini_button(
                    c,
                    i18n::tr(name),
                    x,
                    22,
                    w,
                    26,
                    action,
                    action == Action::AiHistory && self.assistant.history_open,
                    None,
                );
            }
            c.glass_panel(panel, 12., 0x1c1c1e, 0.);
            if self.assistant.history_open {
                let count = self.assistant.chats.as_ref().map_or(0, Vec::len);
                self.assistant.history_offset =
                    self.assistant.history_offset.min(count.saturating_sub(3));
                if count == 0 {
                    c.text(
                        i18n::tr("Belum ada riwayat chat."),
                        85.,
                        74.,
                        panel.2 - 26.,
                        12.,
                        false,
                        0xbcbcc0,
                    );
                }
                for row in 0..3 {
                    let Some(index) = count.checked_sub(1 + row + self.assistant.history_offset)
                    else {
                        break;
                    };
                    let name: String = self.assistant.chats.as_ref().unwrap()[index]
                        .turns
                        .first()
                        .map_or("", |turn| &turn.question)
                        .chars()
                        .take(64)
                        .collect();
                    let y = 65 + row as i32 * 37;
                    self.mini_button(
                        c,
                        &name,
                        81,
                        y,
                        width - 138,
                        30,
                        Action::AiOpenChat(index),
                        self.assistant.selected_chat == Some(index),
                        None,
                    );
                    self.mini_button(
                        c,
                        "×",
                        width - 52,
                        y,
                        28,
                        30,
                        Action::AiDeleteChat(index),
                        false,
                        None,
                    );
                }
            } else {
                self.draw_transcript(c, panel, ready);
            }
            if !waiting && !self.assistant.history_open {
                self.mini_button(
                    c,
                    i18n::tr("Lampirkan"),
                    72,
                    189,
                    82,
                    28,
                    Action::AiAttach,
                    false,
                    None,
                );
            }
            if !self.assistant.attachments.is_empty() {
                self.draw_attachment_row(c, width, waiting);
            } else if !waiting && !self.assistant.history_open {
                if self.assistant.latest.is_some() {
                    self.mini_button(
                        c,
                        i18n::tr("Salin"),
                        160,
                        189,
                        52,
                        28,
                        Action::AiCopy,
                        false,
                        None,
                    );
                    if let Some(task) = self
                        .assistant
                        .latest
                        .as_ref()
                        .and_then(|answer| answer.task.as_deref())
                    {
                        let title_width = (width - 356).max(0) as f32;
                        if title_width > 50. {
                            c.design_text(
                                task,
                                (220., 194., title_width, 20.),
                                (10., 500, 0),
                                0xbcbcc0,
                            );
                        }
                        self.mini_button(
                            c,
                            i18n::tr("Tambah tugas"),
                            width - 133,
                            189,
                            116,
                            28,
                            Action::AiAddTask,
                            true,
                            None,
                        );
                    }
                }
            }
            self.mini_button(
                c,
                i18n::tr(if waiting { "Menunggu" } else { "Kirim" }),
                width - 80,
                227,
                64,
                37,
                Action::AiSend,
                ready && !waiting,
                None,
            );
            let mic = (width - 121, 227, 36, 37);
            self.mini_button(
                c,
                if self.assistant.dictation.is_some() {
                    "…"
                } else {
                    ""
                },
                mic.0,
                mic.1,
                mic.2,
                mic.3,
                Action::AiVoice,
                self.assistant.dictation.is_some(),
                None,
            );
            if self.assistant.dictation.is_none() {
                c.icon_centered("mic-2", (mic.0 as f32, mic.1 as f32, 36., 37.), 17.);
            }
        }
    }

    unsafe fn draw_attachment_row(&mut self, c: &Canvas, width: i32, waiting: bool) {
        let count = self.assistant.attachments.len();
        let available = width - 176;
        let fits = ((available + 5) / 93).max(1) as usize;
        let overflow = count > fits;
        let space = available - if overflow { 54 } else { 0 };
        let visible = (((space + 5) / 93).max(1) as usize).min(count);
        let last = count - visible;
        let offset = self.assistant.attachment_offset.min(last);
        self.assistant.attachment_offset = offset;
        let tile_width = ((space - (visible as i32 - 1) * 5) / visible as i32).min(155);
        let start = if overflow { 187 } else { 160 };
        unsafe {
            if offset > 0 {
                self.mini_button(
                    c,
                    "‹",
                    160,
                    189,
                    22,
                    28,
                    Action::AiScrollAttachments(-1),
                    false,
                    None,
                );
            }
            if offset < last {
                self.mini_button(
                    c,
                    "›",
                    width - 38,
                    189,
                    22,
                    28,
                    Action::AiScrollAttachments(1),
                    false,
                    None,
                );
            }
            for index in offset..offset + visible {
                let x = start + (index - offset) as i32 * (tile_width + 5);
                let item = &self.assistant.attachments[index];
                c.rect(x as f32, 189., tile_width as f32, 28., 7., 0x28282b);
                if let assistant::Attachment::Image(bytes) = item {
                    let _ = c.attachment_thumbnail(
                        &format!(
                            "assistant-image-{}-{index}",
                            self.assistant.attachment_revision
                        ),
                        bytes,
                        (x as f32 + 4., 192., 22.),
                    );
                    c.design_text(
                        i18n::tr("Gambar"),
                        (x as f32 + 30., 195., (tile_width - 54) as f32, 18.),
                        (10., 500, 0),
                        INK,
                    );
                } else {
                    c.design_text(
                        item.name(),
                        (x as f32 + 7., 195., (tile_width - 31) as f32, 18.),
                        (10., 500, 0),
                        INK,
                    );
                }
                if !waiting {
                    self.mini_button(
                        c,
                        "×",
                        x + tile_width - 22,
                        191,
                        20,
                        24,
                        Action::AiDetach(index),
                        false,
                        None,
                    );
                }
            }
        }
    }

    unsafe fn draw_transcript(&mut self, c: &Canvas, panel: (f32, f32, f32, f32), ready: bool) {
        let clip = (panel.0 + 8., panel.1 + 7., panel.2 - 16., panel.3 - 14.);
        let to_end = std::mem::take(&mut self.assistant.scroll_to_end);
        let mut messages = Vec::new();
        for turn in self.assistant.turns() {
            messages.push((true, turn.question.as_str(), turn.attachment.as_deref()));
            messages.push((false, turn.answer.reply.as_str(), None));
        }
        let attachment = self.assistant.attachment_name();
        if self.assistant.waiting() {
            messages.push((true, &self.assistant.prompt, attachment.as_deref()));
            messages.push((false, i18n::tr("Asisten sedang berpikir…"), None));
        } else if self.assistant.dictation.is_some() {
            messages.push((false, i18n::tr("Dikte aktif. Silakan bicara…"), None));
        } else if !self.assistant.notice.is_empty() {
            messages.push((false, i18n::tr(self.assistant.notice), None));
        }
        if messages.is_empty() {
            messages.push((
                false,
                i18n::tr(if ready {
                    "Tanya Asisten, tempel gambar dengan Ctrl+V, atau klik mikrofon."
                } else {
                    "Pilih penyedia dan simpan API key di Pengaturan AI."
                }),
                None,
            ));
        }
        let mut y = 0.;
        let mut rows = Vec::with_capacity(messages.len());
        for (user, text, file) in messages {
            let w = if user { clip.2 * 0.83 } else { clip.2 };
            let h = unsafe { c.design_measure(text, w - 18., (11.5, 500, 3)).1.ceil() };
            let total = h + if file.is_some() { 31. } else { 14. };
            rows.push((user, text, file, w, h, total, y));
            y += total + 6.;
        }
        let scroll_max = (y - 6. - clip.3).max(0.);
        let scroll = if to_end {
            scroll_max
        } else {
            self.assistant.scroll.clamp(0., scroll_max)
        };
        unsafe {
            c.push_design_clip(clip);
            for (user, text, file, w, h, total, start) in rows {
                let y = clip.1 + start - scroll;
                if y + total <= clip.1 || y >= clip.1 + clip.3 {
                    continue;
                }
                let x = if user { clip.0 + clip.2 - w } else { clip.0 };
                c.rect(x, y, w, total, 8., if user { 0x35323e } else { 0x242426 });
                c.design_text(text, (x + 9., y + 6., w - 18., h + 2.), (11.5, 500, 3), INK);
                if let Some(file) = file {
                    c.design_text(
                        file,
                        (x + 9., y + h + 9., w - 18., 17.),
                        (9., 500, 0),
                        0xbab4ce,
                    );
                }
            }
            c.pop_design_clip();
        }
        self.assistant.scroll_max = scroll_max;
        self.assistant.scroll = scroll;
    }

    pub unsafe fn draw_assistant_settings(&mut self, c: &Canvas, settings: &Settings) {
        let ready = key_state(self);
        unsafe {
            c.rect(204., 104., 482., 324., 12., 0x1e1e1e);
            for (i, name) in assistant::PROVIDERS.into_iter().enumerate() {
                self.mini_button(
                    c,
                    name,
                    220 + (i % 3) as i32 * 152,
                    114 + (i / 3) as i32 * 39,
                    144,
                    33,
                    Action::AiProvider(i),
                    settings.ai_provider == i,
                    None,
                );
            }
            let custom = settings.ai_provider == assistant::CUSTOM;
            for (bounds, label) in [
                (KEY_BOUNDS, "API key"),
                (MODEL_BOUNDS, "ID model"),
                (URL_BOUNDS, "URL dasar HTTPS"),
            ] {
                if bounds == URL_BOUNDS && !custom {
                    continue;
                }
                let b = (
                    bounds.x as f32,
                    bounds.y as f32,
                    bounds.width as f32,
                    bounds.height as f32,
                );
                c.text(i18n::tr(label), b.0, b.1 - 20., b.2, 11., true, INK);
                c.rect(b.0, b.1, b.2, b.3, 8., input::FIELD);
                c.outline(b.0, b.1, b.2, b.3, 8., 0x393939);
            }
            let buttons_y = if custom { 385 } else { 343 };
            self.mini_button(
                c,
                i18n::tr("Simpan"),
                220,
                buttons_y,
                108,
                33,
                Action::AiSaveKey,
                true,
                None,
            );
            if ready {
                self.mini_button(
                    c,
                    i18n::tr("Hapus key"),
                    337,
                    buttons_y,
                    104,
                    33,
                    Action::AiDeleteKey,
                    false,
                    None,
                );
            }
            let status = if !self.assistant.notice.is_empty() {
                self.assistant.notice
            } else if ready {
                "API key tersimpan di Windows."
            } else {
                "Belum ada API key."
            };
            c.text(
                i18n::tr(status),
                220.,
                buttons_y as f32 + 38.,
                440.,
                11.,
                false,
                0xbcbcc0,
            );
            c.design_text(
                i18n::tr("Saat dikirim, pesan dan lampiran diteruskan ke penyedia yang dipilih."),
                (220., 453., 440., 36.),
                (11., 400, 3),
                0xbcbcc0,
            );
        }
    }
}
