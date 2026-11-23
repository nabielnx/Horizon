use crate::*;
use spotify::SearchMode;

pub(super) const ROWS: usize = 4;

pub(super) fn edit_bounds(width: i32, mode: SearchMode) -> input::Bounds {
    let scale = (width as f32 / 591.).min(1.);
    let inset = if mode == SearchMode::Free { 402 } else { 442 };
    input::Bounds {
        x: (382. * scale).round() as i32,
        y: 63,
        width: ((width.max(591) - inset) as f32 * scale).round() as i32,
        height: 36,
    }
}

pub(super) unsafe fn sync_edit(hwnd: HWND, w: &mut Window, width: i32, show: bool) {
    let visible =
        show && w.page == 0 && w.spotify_search_input_ready(w.shared.borrow().data.settings.motion);
    if visible && w.spotify_search.edit.is_invalid() {
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
        w.spotify_search.edit = edit;
        SetWindowLongPtrW(edit, GWLP_WNDPROC, edit_proc as *const () as isize);
    }
    let edit = w.spotify_search.edit;
    if edit.is_invalid() {
        return;
    }
    let was_visible = IsWindowVisible(edit).as_bool();
    if visible {
        let client = &w.shared.borrow().data.settings.spotify_client_id;
        let (text, cue, limit) = if w.spotify_search.mode == SearchMode::Setup {
            (client.as_str(), "Client ID Spotify", 128)
        } else {
            (w.spotify_search.query.as_str(), "Judul atau artis", 200)
        };
        input::configure(edit, w.input_font, i18n::tr(cue), limit);
        if text_input(edit) != text && !was_visible {
            let _ = SetWindowTextW(edit, &HSTRING::from(text));
        }
    } else if was_visible && w.spotify_search.mode != SearchMode::Setup {
        w.spotify_search.query = text_input(edit);
    }
    input::place(
        edit,
        edit_bounds(width, w.spotify_search.mode),
        visible,
        w.dpi,
    );
    if visible && !was_visible && GetForegroundWindow() == hwnd {
        let _ = SetFocus(Some(edit));
    }
}

impl Window {
    pub(super) fn set_spotify_search_open(&mut self, open: bool) {
        self.spotify_search.open = open;
        self.focused = None;
        self.spotify_search_reveal.retarget(
            f32::from(open),
            motion::PAGE,
            self.shared.borrow().data.settings.motion,
            Instant::now(),
        );
    }

    pub(super) fn spotify_search_progress(&self, animated: bool) -> f32 {
        if animated {
            self.spotify_search_reveal.sample(Instant::now())
        } else {
            f32::from(self.spotify_search.open)
        }
    }

    pub(super) fn spotify_search_input_ready(&self, animated: bool) -> bool {
        self.spotify_search.open && self.spotify_search_progress(animated) >= 0.985
    }

    pub(super) fn spotify_search_contains(&self, x: i32, y: i32) -> bool {
        if self.settings || !self.expanded || self.page != 0 || !self.spotify_search.open {
            return false;
        }
        let (scale, offset) = self.figma_transform();
        let width = self.shared.borrow().data.settings.width.max(591) as f32;
        let x = (x as f32 - offset) / scale;
        x >= 365. && x < width && (20..host_height()).contains(&y)
    }
}

pub(super) unsafe fn spotify_search_action(hwnd: HWND, w: &mut Window, action: Action) {
    let client_id = w.shared.borrow().data.settings.spotify_client_id.clone();
    let search = &mut w.spotify_search;
    match action {
        Action::SpotifySearchSubmit | Action::SpotifySearchWeb
            if search.mode == SearchMode::Free && !search.edit.is_invalid() =>
        {
            let text = text_input(search.edit);
            let text = text.trim();
            if text.is_empty() {
                search.notice = "Masukkan judul lagu atau nama artis.";
                return;
            }
            search.query = text.to_owned();
            let browser = action == Action::SpotifySearchWeb;
            let opened = if browser {
                spotify::search_url(text, true).is_some_and(|url| calendar::open_url(hwnd, &url))
            } else {
                spotify::open_desktop_search(hwnd, text)
            };
            if opened {
                search.notice = "";
                w.set_spotify_search_open(false);
                sync_inputs(hwnd, w);
            } else {
                search.notice = if browser {
                    "Pencarian Spotify gagal dibuka."
                } else {
                    "Spotify desktop gagal dibuka. Coba Buka di browser."
                };
            }
        }
        Action::SpotifySearchSubmit if !search.busy() && !search.edit.is_invalid() => {
            let text = text_input(search.edit);
            let text = text.trim();
            if search.mode == SearchMode::Setup {
                if spotify::valid_id(text) {
                    search.start(hwnd, text.to_ascii_lowercase(), spotify::Job::Connect);
                } else {
                    search.notice = "Masukkan Client ID dari Spotify Developer Dashboard.";
                }
            } else if text.is_empty() {
                search.songs.clear();
                search.offset = 0;
                search.notice = "";
            } else {
                search.query = text.to_owned();
                search.songs.clear();
                search.offset = 0;
                search.start(hwnd, client_id, spotify::Job::Search(text.to_owned()));
            }
        }
        Action::SpotifySearchSetup if !search.busy() && !search.edit.is_invalid() => {
            let next = if search.mode == SearchMode::Free {
                search.query = text_input(search.edit);
                SearchMode::Setup
            } else {
                SearchMode::Free
            };
            search.mode = next;
            search.notice = "";
            let text = if next == SearchMode::Setup {
                &client_id
            } else {
                &search.query
            };
            let _ = SetWindowTextW(search.edit, &HSTRING::from(text));
            sync_inputs(hwnd, w);
            let _ = SetFocus(Some(w.spotify_search.edit));
        }
        Action::SpotifySearchPlay(index) if !search.busy() => {
            if let Some(song) = search.songs.get(index) {
                search.start(hwnd, client_id, spotify::Job::Play(song.id.clone()));
            }
        }
        Action::SpotifySearchLink(index) => {
            if let Some(song) = search.songs.get(index) {
                let _ = calendar::open_url(
                    hwnd,
                    &format!("https://open.spotify.com/track/{}", song.id),
                );
            }
        }
        Action::SpotifySearchScroll(delta) => {
            search.offset = search
                .offset
                .saturating_add_signed(delta as isize)
                .min(search.songs.len().saturating_sub(ROWS));
            w.focused = None;
        }
        Action::SpotifySearchDisconnect if !search.busy() => {
            if search.disconnect(&client_id) {
                let _ = SetWindowTextW(search.edit, &HSTRING::from(&search.query));
                sync_inputs(hwnd, w);
                let _ = SetFocus(Some(w.spotify_search.edit));
            }
        }
        Action::SpotifySearchHelp => {
            let _ = calendar::open_url(hwnd, "https://developer.spotify.com/dashboard");
        }
        Action::SpotifySearchCopyRedirect => {
            copy_to_clipboard(spotify::REDIRECT);
        }
        _ => {}
    }
}

pub(super) unsafe fn spotify_search_update(hwnd: HWND, w: &mut Window) {
    let Some(result) = w.spotify_search.finish() else {
        return;
    };
    match result {
        Ok(spotify::Update::Connected(client_id)) => {
            let previous = {
                let mut state = w.shared.borrow_mut();
                std::mem::replace(
                    &mut state.data.settings.spotify_client_id,
                    client_id.clone(),
                )
            };
            if persist(w) {
                if !previous.is_empty() && previous != client_id {
                    let _ = assistant::delete_key(&HSTRING::from(format!(
                        "Horizon:Spotify:{previous}"
                    )));
                }
                w.spotify_search.mode = SearchMode::Connected;
                let _ = SetWindowTextW(
                    w.spotify_search.edit,
                    &HSTRING::from(&w.spotify_search.query),
                );
            } else {
                w.shared.borrow_mut().data.settings.spotify_client_id = previous;
                let _ = w.spotify_search.disconnect(&client_id);
                w.spotify_search.mode = SearchMode::Setup;
                w.spotify_search.notice = "Pengaturan Spotify gagal disimpan.";
            }
        }
        Ok(spotify::Update::Songs(songs)) => {
            if songs.is_empty() {
                w.spotify_search.notice = "Lagu tidak ditemukan.";
            }
            w.spotify_search.songs = songs;
        }
        Ok(spotify::Update::Playing) => {
            w.set_spotify_search_open(false);
            if let Some(media) = &w.shared.borrow().media {
                let _ = media.send.send(media::Command::Refresh);
            }
        }
        Err(_) if w.spotify_search.mode == SearchMode::Setup => {
            let client_id = &w.shared.borrow().data.settings.spotify_client_id;
            if !client_id.is_empty() {
                let _ = SetWindowTextW(w.spotify_search.edit, &HSTRING::from(client_id));
            }
        }
        Err(_) => {}
    }
    sync_inputs(hwnd, w);
    let _ = InvalidateRect(Some(hwnd), None, false);
}
