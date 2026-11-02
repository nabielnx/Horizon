//! A native EDIT inside a painted rounded field. Windows retains keyboard,
//! selection and IME behavior; only its stock non-client frame is removed.
use crate::*;
use windows::Win32::UI::Controls::{EM_SETCUEBANNER, EM_SETLIMITTEXT, EM_SETMARGINS};

pub(super) const FIELD: u32 = 0x12151a;
pub(super) const FIELD_COLORREF: COLORREF = COLORREF(0x001a1512);
pub(super) const TASK_FIELD: u32 = 0x141d17;
pub(super) const TASK_FIELD_COLORREF: COLORREF = COLORREF(0x00171d14);
pub(super) const TEXT_COLORREF: COLORREF = COLORREF(0x00f6f4f2);

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Bounds {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}
impl Bounds {
    pub fn edit(self) -> Self {
        Self {
            x: self.x + 12,
            y: self.y + 8,
            width: (self.width - 24).max(1),
            height: (self.height - 16).max(1),
        }
    }
    pub fn contains(self, x: i32, y: i32) -> bool {
        x >= self.x && x < self.x + self.width && y >= self.y && y < self.y + self.height
    }
}
pub(super) fn style() -> WINDOW_STYLE {
    WS_CHILD | WS_TABSTOP | WINDOW_STYLE(0x80)
}

pub(super) unsafe fn configure(edit: HWND, font: HFONT, cue: &str, max_length: usize) {
    unsafe {
        // Also sanitizes the task edit made by the root constructor.
        let style = GetWindowLongPtrW(edit, GWL_STYLE) as u32 & !WS_BORDER.0;
        let extended = GetWindowLongPtrW(edit, GWL_EXSTYLE) as u32 & !WS_EX_CLIENTEDGE.0;
        SetWindowLongPtrW(edit, GWL_STYLE, style as isize);
        SetWindowLongPtrW(edit, GWL_EXSTYLE, extended as isize);
        let _ = SetWindowPos(
            edit,
            None,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
        SendMessageW(
            edit,
            WM_SETFONT,
            Some(WPARAM(font.0 as usize)),
            Some(LPARAM(1)),
        );
        SendMessageW(edit, EM_SETLIMITTEXT, Some(WPARAM(max_length)), None);
        // The parent provides the full field padding; Windows adds no extra edge.
        SendMessageW(
            edit,
            EM_SETMARGINS,
            Some(WPARAM((EC_LEFTMARGIN | EC_RIGHTMARGIN) as usize)),
            Some(LPARAM(0)),
        );
        let text = HSTRING::from(cue);
        SendMessageW(
            edit,
            EM_SETCUEBANNER,
            Some(WPARAM(0)),
            Some(LPARAM(text.as_ptr() as isize)),
        );
    }
}

pub(super) fn task_columns(width: i32) -> (Bounds, Bounds) {
    let content = width - 88;
    let (summary, gap) = if content < 352 { (92, 8) } else { (118, 14) };
    (
        Bounds {
            x: 70,
            y: 64,
            width: content - summary - gap,
            height: 156,
        },
        Bounds {
            x: width - 18 - summary,
            y: 64,
            width: summary,
            height: 156,
        },
    )
}
pub(super) fn task_bounds(width: i32) -> Bounds {
    let (list, _) = task_columns(width);
    Bounds {
        x: list.x,
        y: 230,
        width: list.width,
        height: 38,
    }
}
pub(super) fn priority_pill(width: i32) -> Bounds {
    let (_, summary) = task_columns(width);
    Bounds {
        x: summary.x,
        y: 230,
        width: summary.width - 46,
        height: 38,
    }
}
pub(super) fn priority_popup(width: i32) -> Bounds {
    let pill = priority_pill(width);
    Bounds {
        x: pill.x + pill.width - 132,
        y: pill.y - 116,
        width: 132,
        height: 108,
    }
}
pub(super) fn figma_event_bounds(width: i32, index: usize) -> Bounds {
    let scale = (width as f32 / 591.).min(1.);
    let extra = (width as f32 - 591.).max(0.);
    let (x, y, w) = match index {
        0 => (388., 74, 171. + extra),
        1 => (388., 126, 81. + extra / 2.),
        2 => (478. + extra / 2., 126, 81. + extra / 2.),
        _ => (388., 178, 171. + extra),
    };
    Bounds {
        x: (x * scale).round() as i32,
        y,
        width: (w * scale).round() as i32,
        height: 30,
    }
}
pub(super) fn in_view(bounds: Bounds, panel_height: i32) -> bool {
    bounds.y >= 22 && bounds.y + bounds.height <= panel_height - 12
}

pub(super) unsafe fn place(edit: HWND, bounds: Bounds, visible: bool) {
    unsafe {
        // Move before showing so a new control never flashes at its old position.
        if visible {
            let b = bounds.edit();
            let _ = MoveWindow(edit, b.x, b.y, b.width, b.height, true);
            let _ = ShowWindow(edit, SW_SHOW);
            let _ = InvalidateRect(Some(edit), None, true);
        } else {
            let _ = ShowWindow(edit, SW_HIDE);
            let _ = MoveWindow(edit, -2000, -2000, 10, 10, false);
        }
    }
}

fn fields(w: &Window) -> [Option<(HWND, Bounds)>; 4] {
    let mut fields = [None; 4];
    if w.settings || !w.expanded || w.expansion < 0.95 {
        return fields;
    }
    let settings = &w.shared.borrow().data.settings;
    if w.page == 4 {
        fields[0] = Some((w.input, task_bounds(settings.width)));
        return fields;
    }
    if w.page != 2 || !w.calendar.form {
        return fields;
    }
    for (i, &edit) in w.event_inputs.iter().enumerate() {
        let bounds = figma_event_bounds(settings.width, i);
        fields[i] = in_view(bounds, host_height()).then_some((edit, bounds));
    }
    fields
}

pub(super) unsafe fn draw_fields(c: &Canvas, w: &Window) {
    if !w.settings && w.page == 4 {
        return;
    }
    unsafe {
        let focused = GetFocus();
        for (edit, b) in fields(w).into_iter().flatten() {
            c.rect(
                b.x as f32,
                b.y as f32,
                b.width as f32,
                b.height as f32,
                9.,
                FIELD,
            );
            c.outline(
                b.x as f32,
                b.y as f32,
                b.width as f32,
                b.height as f32,
                9.,
                if edit == focused { 0x6e7b8b } else { 0x242a34 },
            );
        }
    }
}

/// Padding is part of the input's click target, rather than an empty shell area.
pub(super) unsafe fn focus_at(w: &Window, x: i32, y: i32) -> bool {
    for (edit, bounds) in fields(w).into_iter().flatten() {
        if bounds.contains(x, y) {
            unsafe {
                let _ = SetFocus(Some(edit));
            }
            return true;
        }
    }
    false
}

/// Call from edit_proc on WM_SETFOCUS / WM_KILLFOCUS. This repaints only the
/// rounded field edge; it never rebuilds layout or changes the edit contents.
pub(super) unsafe fn focus_changed(edit: HWND) {
    unsafe {
        if let Ok(parent) = GetParent(edit) {
            let _ = InvalidateRect(Some(parent), None, false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn task_input_and_priority_follow_the_two_columns() {
        for width in [360, 480, 540, 560, 680] {
            let (list, summary) = task_columns(width);
            let field = task_bounds(width);
            let pill = priority_pill(width);
            let menu = priority_popup(width);
            assert!(list.width > summary.width);
            assert_eq!(field.x + field.width, list.x + list.width);
            assert_eq!(pill.x, summary.x);
            assert_eq!(pill.x + pill.width + 8 + 38, summary.x + summary.width);
            assert_eq!(menu.y + menu.height + 8, pill.y);
            assert_eq!(menu.x + menu.width, pill.x + pill.width);
        }
        assert_eq!(task_bounds(540).width, 320);
        assert_eq!(priority_pill(540).width, 72);
    }
    #[test]
    fn text_rect_stays_inside_round_corners_at_narrow_and_wide_sizes() {
        for width in [360, 560, 680] {
            for outer in [
                task_bounds(width),
                figma_event_bounds(width, 0),
                figma_event_bounds(width, 1),
                figma_event_bounds(width, 2),
            ] {
                let inner = outer.edit();
                assert!(inner.x >= outer.x + 9 && inner.y >= outer.y + 8);
                assert!(inner.x + inner.width <= outer.x + outer.width - 9);
                assert!(inner.y + inner.height <= outer.y + outer.height - 8);
                assert!(inner.width > 20 && inner.height == outer.height - 16);
            }
            let time = figma_event_bounds(width, 1);
            let lead = figma_event_bounds(width, 2);
            assert!(time.x + time.width + 4 <= lead.x);
        }
    }
    #[test]
    fn calendar_editor_fields_fit_card_without_scrolling() {
        for width in [360, 480, 591, 680] {
            for index in 0..4 {
                let field = figma_event_bounds(width, index);
                let scale = (width as f32 / 591.).min(1.);
                let extra = (width as f32 - 591.).max(0.);
                assert!(in_view(field, host_height()));
                assert!(field.x >= (375. * scale) as i32);
                assert!(field.x + field.width <= ((572. + extra) * scale).ceil() as i32);
                assert!(field.y + field.height < 226);
            }
        }
    }
}
