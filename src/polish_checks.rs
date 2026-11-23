//! Native UI fixtures, enabled only by --offscreen --verify.
use crate::*;
use chrono::Datelike;

unsafe fn capture(w: &Window, path: &str) -> Result<()> {
    let layouts = std::env::args().any(|arg| arg == "--capture-layouts")
        && (path.contains("polish-591-page-")
            || path.contains("polish-680-page-")
            || path.contains("layout-20261102-")
            || path.ends_with("polish-task-priority-540.bmp"));
    if layouts || std::env::args().any(|arg| arg == "--capture-fixtures") {
        unsafe {
            w.canvas
                .as_ref()
                .unwrap()
                .save_bmp(std::path::Path::new(path))?;
        }
    }
    Ok(())
}

unsafe fn paint(w: &mut Window, width: i32) -> Result<()> {
    unsafe {
        w.overlay = None;
        let height = host_height();
        w.canvas = Some(Canvas::image(width as u32, height as u32)?);
        w.paint()
    }
}
pub(super) unsafe fn run(hwnd: HWND, w: &mut Window) -> Result<()> {
    unsafe {
        let saved_calendar = std::mem::take(&mut w.calendar);
        let saved_page = w.page;
        let saved_focus = (
            w.shared.borrow().focus_left,
            w.shared.borrow().data.focus_remaining,
        );
        let (settings, tasks, events, files, water, media) = {
            let mut s = w.shared.borrow_mut();
            (
                s.data.settings.clone(),
                std::mem::take(&mut s.data.tasks),
                std::mem::take(&mut s.data.events),
                std::mem::take(&mut s.data.shelf_files),
                s.data.water,
                s.media.take(),
            )
        };
        let tomorrow = Local::now().date_naive() + chrono::Duration::days(1);
        w.calendar.select(tomorrow);
        {
            let mut s = w.shared.borrow_mut();
            s.data.settings.show_lyrics = true;
            s.data.settings.album_colors = true;
            s.data.settings.focus_minutes = 25;
            s.data.settings.water_goal = 2000;
            s.focus_left = 1500;
            for (text, priority, done) in [
                ("Rapikan proposal", 1, false),
                ("Kirim revisi desain", 0, false),
                ("Review catatan", 2, true),
            ] {
                s.data.tasks.push(Task {
                    text: text.into(),
                    done,
                    due: None,
                    notified: false,
                    priority,
                });
            }
            let at = Local
                .from_local_datetime(&tomorrow.and_hms_opt(12, 0, 0).unwrap())
                .single()
                .unwrap()
                .timestamp();
            s.data.tasks[0].due = Some(at);
            for i in 0..7 {
                s.data.events.push(model::Event {
                    title: format!("Agenda {}", i + 1),
                    at: at + i * 300,
                    notified: false,
                    lead_minutes: Some(5),
                    url: String::new(),
                });
            }
            s.data.water = 750;
            s.data.add_shelf_file(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("assets/fixtures/cover.png")
                    .to_string_lossy()
                    .into_owned(),
            );
            s.media = Some(media::Media::fixture(media::Track {
                title: "Night Drive".into(),
                artist: "Aster · Midnight Sessions".into(),
                position: 108.,
                duration: 251.,
                can_seek: true,
                playing: true,
                palette: artwork::palette(include_bytes!("../assets/fixtures/cover.png")),
                cover: Some(std::sync::Arc::new(
                    include_bytes!("../assets/fixtures/cover.png").to_vec(),
                )),
                lyrics: std::sync::Arc::new(lyrics::parse(
                    "[00:00]Tarik napas, pelan saja\n[00:30]Lampu kota mulai menyala\n[01:00]Esok kita mulai lagi\n[01:30]Masih ada waktu untuk bermimpi\n[02:00]Langkah kecil tetap berarti\n[02:30]Biar malam menemani\n[03:00]Kita temukan arah pulang",
                )),
                ..Default::default()
            }));
        }
        w.shared.borrow_mut().data.settings.width = 540;
        for mode in [shell_policy::Mode::Compact, shell_policy::Mode::Peek] {
            w.expanded = false;
            w.expansion = 0.;
            w.peek = if mode == shell_policy::Mode::Peek {
                1.
            } else {
                0.
            };
            w.mode = mode;
            paint(w, 540)?;
            let (left, right, height) = if mode == shell_policy::Mode::Compact {
                (115, 425, layout::COMPACT_HEIGHT as i32)
            } else {
                (0, 540, 101)
            };
            if w.hits
                .iter()
                .any(|h| h.x < left || h.x + h.w > right || h.y < 0 || h.y + h.h > height)
            {
                return Err(Error::new(E_FAIL, "Idle controls cross their mode bounds"));
            }
        }
        w.expanded = true;
        w.expansion = 1.;
        w.peek = 1.;
        w.mode = shell_policy::Mode::Expanded;
        let mut rendered = 0;
        for width in [360, 480, 540, 560, 591, 680] {
            w.shared.borrow_mut().data.settings.width = width;
            for page in 0..=monitor::PAGE {
                w.page = page;
                w.priority_open = false;
                w.shelf_detail = false;
                w.calendar.scroll = 0.;
                w.calendar.target_scroll = 0.;
                paint(w, width)?;
                if page == 5
                    && w.hits
                        .iter()
                        .any(|h| matches!(h.action, Action::ClearShelfFiles) && h.y < 31)
                {
                    return Err(Error::new(E_FAIL, "Shelf clear button crowds the clock"));
                }
                for hit in w.hits.iter().filter(|hit| hit.x >= 62) {
                    if hit.x + hit.w > width
                        || hit.y < 0
                        || hit.y + hit.h
                            > if page <= 2 {
                                host_height()
                            } else {
                                panel_height(&w.shared.borrow().data.settings)
                            }
                        || hit.w <= 0
                        || hit.h <= 0
                    {
                        return Err(Error::new(
                            E_FAIL,
                            format!(
                                "Control outside panel: page={page}, width={width}, bounds=({},{},{},{})",
                                hit.x, hit.y, hit.w, hit.h
                            ),
                        ));
                    }
                }
                capture(
                    w,
                    &format!("artifacts/previews/polish-{width}-page-{page}.bmp"),
                )?;
                rendered += 1;
            }
        }
        w.shared.borrow_mut().data.settings.width = 680;
        w.page = 0;
        paint(w, 680)?;
        let sidebar: Vec<_> = w
            .hits
            .iter()
            .filter(|hit| matches!(hit.action, Action::Page(_) | Action::Settings))
            .collect();
        if sidebar.len() != 7
            || ![monitor::PAGE, 0, 1, 2, 3, 4]
                .into_iter()
                .enumerate()
                .all(|(i, page)| sidebar[i].action == Action::Page(page))
            || sidebar[6].action != Action::Settings
            || sidebar
                .iter()
                .any(|hit| hit.w != 32 || hit.h != 32 || hit.y + hit.h > host_height())
            || sidebar
                .windows(2)
                .any(|pair| pair[0].y + pair[0].h >= pair[1].y)
        {
            return Err(Error::new(
                E_FAIL,
                "Sidebar must start with Monitor above Music and keep Settings clear",
            ));
        }
        capture(w, "artifacts/previews/layout-20261102-sidebar.bmp")?;
        action(hwnd, w, Action::Page(monitor::PAGE));
        paint(w, 680)?;
        if !w
            .hits
            .iter()
            .any(|hit| hit.action == Action::ToggleMonitorCompact)
        {
            return Err(Error::new(E_FAIL, "Monitor toggle must be reachable"));
        }
        action(hwnd, w, Action::ToggleMonitorCompact);
        let persisted = {
            let state = w.shared.borrow();
            Data::load(&state.store).is_ok_and(|data| data.settings.monitor_in_compact)
        };
        if !persisted {
            return Err(Error::new(E_FAIL, "Monitor toggle did not persist"));
        }
        w.expanded = false;
        w.expansion = 0.;
        w.mode = shell_policy::Mode::Compact;
        w.idle_transition = None;
        w.shared.borrow_mut().data.settings.width = 360;
        let outline = shell_outline(w);
        let badges = monitor::Badges::new(outline);
        let previous_dpi = w.dpi;
        for dpi in [96, 120, 144].map(dpi::Scale::new) {
            w.dpi = dpi;
            apply_frame(hwnd, w);
            let mut client = RECT::default();
            GetClientRect(hwnd, &mut client)?;
            let badge_region = CreateRectRgn(0, 0, 0, 0);
            let result = GetWindowRgn(hwnd, badge_region);
            let physical = |dips| dpi.physical(dips).round() as i32;
            let y = physical(outline.height / 2.);
            let hit = |x| PtInRegion(badge_region, physical(x), y).as_bool();
            let separate = hit(badges.cpu.0 + 14.)
                && hit(badges.memory.0 + 54.)
                && !hit(outline.offset + outline.width + 3.);
            let _ = DeleteObject(badge_region.into());
            if result.0 == ERROR
                || !separate
                || !w.show_monitor_badges()
                || client.right != dpi.px(monitor::COMPACT_CLIENT_WIDTH)
            {
                return Err(Error::new(E_FAIL, "Scaled monitor badges must be separate"));
            }
            w.canvas = Some(Canvas::new(
                hwnd,
                client.right as u32,
                client.bottom as u32,
                w.alpha_shell,
                dpi,
            )?);
            w.paint()?;
        }
        w.dpi = previous_dpi;
        w.expanded = true;
        w.expansion = 1.;
        w.mode = shell_policy::Mode::Expanded;
        action(hwnd, w, Action::ToggleMonitorCompact);
        w.shared.borrow_mut().data.settings.width = 680;
        w.sidebar_scroll = sidebar_max_scroll(figma_ui::SIDEBAR_ICONS.len());
        w.sidebar_target_scroll = w.sidebar_scroll;
        w.page = assistant::PAGE;
        paint(w, 680)?;
        let assistant_hit = w
            .hits
            .iter()
            .find(|hit| hit.action == Action::Page(assistant::PAGE));
        let settings_hit = w.hits.iter().find(|hit| hit.action == Action::Settings);
        if !matches!((assistant_hit, settings_hit), (Some(page), Some(settings))
            if page.h == SIDEBAR_SIZE as i32
                && settings.y - (page.y + page.h) == SIDEBAR_SETTINGS_GAP as i32)
            || !w
                .hits
                .iter()
                .any(|hit| hit.action == Action::SettingsAt(assistant::SETTINGS_PAGE))
        {
            return Err(Error::new(
                E_FAIL,
                "Scrolling did not reveal Assistant and keep Settings available",
            ));
        }
        w.assistant
            .attach([assistant::Attachment::File("Notes.txt".into())]);
        w.assistant
            .attach_clipboard_image(include_bytes!("../assets/fixtures/cover.png").to_vec());
        for width in [360, 680] {
            w.shared.borrow_mut().data.settings.width = width;
            paint(w, width)?;
            let mut items = w
                .hits
                .iter()
                .filter(|hit| matches!(hit.action, Action::AiDetach(_)));
            if !matches!((items.next(), items.next()), (Some(first), Some(second))
                if first.action == Action::AiDetach(0) && second.action == Action::AiDetach(1)
                    && first.y == second.y && first.x + first.w < second.x)
                || items.next().is_some()
            {
                return Err(Error::new(
                    E_FAIL,
                    "Two Assistant attachments must appear side by side",
                ));
            }
        }
        action(hwnd, w, Action::AiDetach(0));
        if !matches!(
            w.assistant.attachments.as_slice(),
            [assistant::Attachment::Image(_)]
        ) {
            return Err(Error::new(
                E_FAIL,
                "Removing one attachment affected its neighbor",
            ));
        }
        w.assistant.clear_chat();
        w.sidebar_scroll = 0.;
        w.sidebar_target_scroll = 0.;
        w.page = 0;
        let track_before = w
            .shared
            .borrow()
            .media
            .as_ref()
            .unwrap()
            .snapshot
            .lock()
            .unwrap()
            .clone();
        let events_before = w.shared.borrow().data.events.clone();
        w.shared.borrow_mut().data.settings.width = 591;
        {
            let s = w.shared.borrow();
            let mut t = s.media.as_ref().unwrap().snapshot.lock().unwrap();
            t.title = "Tak Ingin Usai — Live Acoustic Session".into();
            t.artist = "Keisya Levronka · Kolaborasi Spesial".into();
            t.duration = 180.;
            t.lyrics=std::sync::Arc::new((0..18).map(|i|lyrics::Line {
                at:i as f64*10.,
                text:if i%3==0 {"Kita masih punya waktu untuk kembali menemukan arah, berjalan perlahan dan menyelesaikan cerita ini bersama".into()}else{format!("Langkah {} membawa kita pulang",i+1)},
                ..Default::default()
            }).collect());
        }
        w.page = 0;
        for (name, position) in [("first", 0.), ("middle", 65.), ("last", 175.)] {
            w.shared
                .borrow()
                .media
                .as_ref()
                .unwrap()
                .snapshot
                .lock()
                .unwrap()
                .position = position;
            paint(w, 591)?;
            capture(
                w,
                &format!("artifacts/previews/layout-20261102-music-{name}.bmp"),
            )?;
        }
        w.expanded = false;
        w.expansion = 0.;
        w.peek = 0.;
        w.mode = shell_policy::Mode::Compact;
        paint(w, 591)?;
        capture(w, "artifacts/previews/layout-20261102-compact.bmp")?;
        {
            let state = w.shared.borrow();
            let mut track = state.media.as_ref().unwrap().snapshot.lock().unwrap();
            // A distinct title catches accidentally rendering metadata instead
            // of the synchronized compact lyric. A short line exposes centering.
            track.title = "This song title must not be the compact caption".into();
            track.lyrics = std::sync::Arc::new(vec![lyrics::Line {
                at: 0.,
                text: "I loved You".into(),
                ..Default::default()
            }]);
            track.position = 86.4;
        }
        paint(w, 591)?;
        capture(
            w,
            "artifacts/previews/layout-20261102-compact-reference.bmp",
        )?;
        w.peek = 1.;
        w.mode = shell_policy::Mode::Peek;
        for (name, playing) in [("playing", true), ("paused", false), ("resumed", true)] {
            {
                let state = w.shared.borrow();
                let mut track = state.media.as_ref().unwrap().snapshot.lock().unwrap();
                track.playing = playing;
                track.title = "Night Drive".into();
                track.artist = "Aster".into();
                track.lyrics = std::sync::Arc::new(vec![lyrics::Line {
                    at: 0.,
                    text: "But for u i leave anything behind".into(),
                    ..Default::default()
                }]);
            }
            paint(w, 591)?;
            // Frame 2 is the playing view; Frame 3 is the paused dashboard.
            // Both must retain the transport button and their song timeline.
            let (seek_x, seek_w) = if playing { (292, 196) } else { (317, 65) };
            if !w
                .hits
                .iter()
                .any(|h| matches!(h.action, Action::Seek) && h.x == seek_x && h.w == seek_w)
                || !w.hits.iter().any(|h| {
                    matches!(h.action, Action::Media(0))
                        && h.x == 518
                        && h.y == 29
                        && h.w == 40
                        && h.h == 39
                })
            {
                return Err(Error::new(
                    E_FAIL,
                    "Peek play/pause controls or dashboard timeline missing",
                ));
            }
            capture(
                w,
                &format!("artifacts/previews/layout-20261102-peek-{name}.bmp"),
            )?;
        }
        w.expanded = true;
        w.expansion = 1.;
        w.peek = 1.;
        w.mode = shell_policy::Mode::Expanded;
        w.page = 2;
        for event in &mut w.shared.borrow_mut().data.events {
            event.title = "Rapat koordinasi dan review desain aplikasi Horizon".into();
        }
        paint(w, 591)?;
        capture(w, "artifacts/previews/layout-20261102-calendar.bmp")?;
        for edit in w
            .hits
            .iter()
            .filter(|h| matches!(h.action, Action::EditEvent(_)))
        {
            for delete in w
                .hits
                .iter()
                .filter(|h| matches!(h.action, Action::DeleteEvent(_)))
            {
                let overlaps = edit.x < delete.x + delete.w
                    && delete.x < edit.x + edit.w
                    && edit.y < delete.y + delete.h
                    && delete.y < edit.y + edit.h;
                if overlaps {
                    return Err(Error::new(
                        E_FAIL,
                        "Calendar edit/delete hit targets overlap",
                    ));
                }
            }
        }
        w.calendar.form = true;
        paint(w, 591)?;
        capture(w, "artifacts/previews/layout-20261102-calendar-editor.bmp")?;
        w.calendar.form = false;
        *w.shared
            .borrow()
            .media
            .as_ref()
            .unwrap()
            .snapshot
            .lock()
            .unwrap() = track_before;
        w.shared.borrow_mut().data.events = events_before;
        w.shared.borrow_mut().data.settings.width = 540;
        w.page = 1;
        action(hwnd, w, Action::Reset);
        action(hwnd, w, Action::Focus);
        if w.shared.borrow().running.is_none() {
            return Err(Error::new(E_FAIL, "Focus failed to start"));
        }
        w.shared.borrow_mut().running = Some(Instant::now() - std::time::Duration::from_secs(750));
        action(hwnd, w, Action::Focus);
        if w.shared.borrow().running.is_some() || remaining(&w.shared.borrow()) != 750 {
            return Err(Error::new(E_FAIL, "Focus pause lost elapsed time"));
        }
        paint(w, 540)?;
        if w.hits
            .iter()
            .filter(|h| matches!(h.action, Action::SetFocusMinutes(_)))
            .count()
            != 0
        {
            return Err(Error::new(E_FAIL, "Paused focus exposed duration controls"));
        }
        action(hwnd, w, Action::Focus);
        if w.shared.borrow().focus_left != 750 {
            return Err(Error::new(E_FAIL, "Focus resume reset the timer"));
        }
        paint(w, 540)?;
        action(hwnd, w, Action::Reset);
        paint(w, 540)?;
        if w.hits
            .iter()
            .filter(|h| matches!(h.action, Action::SetFocusMinutes(_)))
            .count()
            != 3
            || w.hits
                .iter()
                .filter(|h| matches!(h.action, Action::SetBreakMinutes(_)))
                .count()
                != 3
        {
            return Err(Error::new(E_FAIL, "Focus and break presets are missing"));
        }
        action(hwnd, w, Action::FocusTaskMenu);
        paint(w, 540)?;
        if !w.hits.iter().any(|h| h.action == Action::FocusTask(None))
            || !w
                .hits
                .iter()
                .any(|h| h.action == Action::FocusTask(Some(0)))
            || w.hits
                .iter()
                .any(|h| h.action == Action::FocusTask(Some(2)))
        {
            return Err(Error::new(
                E_FAIL,
                "Focus task picker did not filter completed tasks",
            ));
        }
        action(hwnd, w, Action::FocusTask(None));
        w.shared.borrow_mut().focus_left = 0;
        paint(w, 540)?;
        action(hwnd, w, Action::Focus);
        if w.shared.borrow().focus_left != 1500 {
            return Err(Error::new(E_FAIL, "Completed focus failed to restart"));
        }
        action(hwnd, w, Action::Reset);
        w.page = 4;
        paint(w, 540)?;
        apply_frame(hwnd, w);
        let mut alpha = 0;
        GetLayeredWindowAttributes(hwnd, None, Some(&mut alpha), None)?;
        if alpha != 255 {
            return Err(Error::new(E_FAIL, "Tasks surface is not opaque"));
        }
        let field = input::task_bounds(540);
        let (list, summary) = input::task_columns(540);
        if field.width != list.width || input::priority_pill(540).x != summary.x {
            return Err(Error::new(E_FAIL, "Task composer columns do not align"));
        }
        let mut edit = RECT::default();
        GetWindowRect(w.input, &mut edit)?;
        if edit.right - edit.left != field.edit().width {
            return Err(Error::new(
                E_FAIL,
                "Native task edit width does not match its field",
            ));
        }
        w.priority_open = true;
        paint(w, 540)?;
        capture(w, "artifacts/previews/polish-task-priority-540.bmp")?;
        let canvas = w.canvas.as_ref().unwrap();
        let pill = input::priority_pill(540);
        let size = tasks_ui::priority_label_size(canvas, pill.width);
        for label in ["Penting", "Sedang", "Ringan"] {
            if canvas.measure_text(label, size, false) > (pill.width - 37) as f32 {
                return Err(Error::new(E_FAIL, "Priority label is clipped"));
            }
        }
        {
            use windows::Win32::Graphics::Direct2D::*;
            let factory: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let _ = icons::outline(&factory, "chevron-up")?;
        }
        let menu = input::priority_popup(540);
        if menu.y + menu.height >= field.y {
            return Err(Error::new(
                E_FAIL,
                "Task priority menu does not open upward",
            ));
        }
        w.focused = Some(0);
        SendMessageW(hwnd, WM_KEYDOWN, Some(WPARAM(40)), Some(LPARAM(0)));
        if w.focused != Some(1) {
            return Err(Error::new(E_FAIL, "Priority keyboard navigation failed"));
        }
        SendMessageW(hwnd, WM_KEYDOWN, Some(WPARAM(13)), Some(LPARAM(0)));
        if w.priority_open || w.task_priority != 0 {
            return Err(Error::new(E_FAIL, "Priority keyboard selection failed"));
        }
        paint(w, 540)?;
        for index in 0..3 {
            let toggle = w
                .hits
                .iter()
                .find(|h| matches!(h.action, Action::ToggleTask(i) if i == index))
                .unwrap();
            let delete = w
                .hits
                .iter()
                .find(|h| matches!(h.action, Action::DeleteTask(i) if i == index))
                .unwrap();
            if toggle.x + toggle.w > delete.x || toggle.y + toggle.h > input::task_bounds(560).y {
                return Err(Error::new(E_FAIL, "Task row actions overlap"));
            }
        }
        let tasks_with_items = std::mem::take(&mut w.shared.borrow_mut().data.tasks);
        paint(w, 540)?;
        w.shared.borrow_mut().data.tasks = tasks_with_items;
        w.page = 3;
        apply_frame(hwnd, w);
        GetLayeredWindowAttributes(hwnd, None, Some(&mut alpha), None)?;
        if alpha != w.shared.borrow().data.settings.opacity {
            return Err(Error::new(E_FAIL, "Tasks opacity leaked into another page"));
        }
        w.shared.borrow_mut().data.settings.width = 560;
        w.page = 4;
        w.priority_open = false;
        action(hwnd, w, Action::TaskPriorityMenu);
        paint(w, 560)?;
        if w.hits
            .iter()
            .filter(|hit| matches!(hit.action, Action::SetTaskPriority(_)))
            .count()
            != 3
        {
            return Err(Error::new(E_FAIL, "Priority menu is incomplete"));
        }
        capture(w, "artifacts/previews/polish-priority-menu.bmp")?;
        action(hwnd, w, Action::SetTaskPriority(1));
        let count = w.shared.borrow().data.tasks.len();
        SetWindowTextW(w.input, w!("@14:30"))?;
        action(hwnd, w, Action::AddTask);
        if w.shared.borrow().data.tasks.len() != count {
            return Err(Error::new(E_FAIL, "Empty task title was accepted"));
        }
        SetWindowTextW(w.input, w!("Priority fixture"))?;
        action(hwnd, w, Action::AddTask);
        if !w
            .shared
            .borrow()
            .data
            .tasks
            .last()
            .is_some_and(|task| task.priority == 1)
        {
            return Err(Error::new(E_FAIL, "Priority did not reach the new task"));
        }
        action(hwnd, w, Action::DeleteTask(count));
        w.page = 2;
        w.calendar.scroll = 0.;
        w.calendar.target_scroll = 0.;
        w.calendar.selected = tomorrow;
        w.calendar.month = tomorrow.with_day(1).unwrap();
        w.calendar.agenda_offset = 0;
        action(hwnd, w, Action::AgendaPage(1));
        paint(w, 560)?;
        if !w
            .hits
            .iter()
            .any(|hit| matches!(hit.action, Action::EditEvent(3)))
        {
            return Err(Error::new(E_FAIL, "Fourth agenda is inaccessible"));
        }
        action(hwnd, w, Action::AgendaPage(1));
        action(hwnd, w, Action::AgendaPage(1));
        paint(w, 560)?;
        if !w
            .hits
            .iter()
            .any(|hit| matches!(hit.action, Action::EditEvent(6)))
        {
            return Err(Error::new(E_FAIL, "Last agenda is inaccessible"));
        }
        w.shared.borrow_mut().data.settings.width = 360;
        w.page = 5;
        w.shelf_detail = true;
        paint(w, 360)?;
        if w.hits
            .iter()
            .filter(|hit| {
                matches!(
                    hit.action,
                    Action::OpenShelfFile(_)
                        | Action::OpenFileFolder(_)
                        | Action::CopyFilePath(_)
                        | Action::RemoveShelfFile(_)
                )
            })
            .count()
            != 4
        {
            return Err(Error::new(E_FAIL, "Narrow shelf actions are missing"));
        }
        let toggle = Action::ToggleShelfPreview(0);
        let path = w.shared.borrow().data.shelf_files[0].path.clone();
        if !w.hits.iter().any(|hit| hit.action == toggle)
            || w.shelf_preview_open
            || w.preview_worker
                .as_ref()
                .and_then(|worker| worker.preview(&path))
                .is_some()
        {
            return Err(Error::new(
                E_FAIL,
                "Shelf preview must be available on demand",
            ));
        }
        action(hwnd, w, toggle);
        let deadline = Instant::now() + std::time::Duration::from_secs(2);
        loop {
            paint(w, 360)?;
            if matches!(
                w.preview_worker
                    .as_ref()
                    .and_then(|worker| worker.preview(&path)),
                Some(shelf::Preview::Image(_))
            ) {
                break;
            }
            if Instant::now() >= deadline {
                return Err(Error::new(
                    E_FAIL,
                    "Shelf preview did not load after opening",
                ));
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        action(hwnd, w, toggle);
        paint(w, 360)?;
        if w.shelf_preview_open || !w.hits.iter().any(|hit| hit.action == toggle) {
            return Err(Error::new(
                E_FAIL,
                "Shelf preview cannot be closed and reopened",
            ));
        }
        capture(w, "artifacts/previews/polish-shelf-detail-360.bmp")?;
        {
            let mut s = w.shared.borrow_mut();
            s.data.settings = settings;
            s.focus_left = saved_focus.0;
            s.data.focus_remaining = saved_focus.1;
            s.data.tasks = tasks;
            s.data.events = events;
            s.data.shelf_files = files;
            s.data.water = water;
            s.media = media;
        }
        w.calendar = saved_calendar;
        w.page = saved_page;
        w.priority_open = false;
        w.shelf_detail = false;
        w.feedback = None;
        w.water_fill = -1.;
        w.overlay = None;
        w.canvas = None;
        apply_frame(hwnd, w);
        let _ = UpdateWindow(hwnd);
        let report = serde_json::json!({"offscreen":true,"desktopInputUsed":false,"widths":[360,480,540,560,591,680],"expandedPanelSize":[680,280],"figmaPeekSize":[591,101],"figmaCompactSize":[310,42],"commonExpandedHeight":280,"expandedBottomRadius":32,"pausedPeekDashboard":true,"current540CompactAndPeekInBounds":true,"shelfHeaderClearsClock":true,"pagesRendered":rendered,"controlsInBounds":true,"priorityMenuAndTaskCreation":true,"taskRowActionsDoNotOverlap":true,"taskComposerColumnsAligned":true,"tasksSurfaceOpaque":true,"otherPagesKeepConfiguredOpacity":true,"taskPriorityOpensUpward":true,"taskPriorityKeyboardNavigation":true,"focusStartPauseResumeResetRestart":true,"blankTaskRejected":true,"allSevenAgendaItemsAccessible":true,"narrowShelfActionsAccessible":true});
        std::fs::write(
            "artifacts/polish-checks-0.3.json",
            serde_json::to_vec_pretty(&report).map_err(|e| Error::new(E_FAIL, e.to_string()))?,
        )
        .map_err(|e| Error::new(E_FAIL, e.to_string()))?;
        Ok(())
    }
}
