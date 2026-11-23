use crate::{Alert, Shared};
use chrono::Local;
use serde::{Deserialize, Serialize};
use std::time::Instant;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    #[default]
    Work,
    Finished,
    Break,
    BreakFinished,
    Extension,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Session {
    pub phase: Phase,
    pub task: Option<usize>,
    pub completed: std::collections::BTreeMap<String, u32>,
}

pub fn duration(s: &Shared) -> u64 {
    match s.data.focus.phase {
        Phase::Break | Phase::BreakFinished => u64::from(s.data.settings.focus_break_minutes) * 60,
        Phase::Extension => 300,
        _ => u64::from(s.data.settings.focus_minutes) * 60,
    }
}

pub fn finished(phase: Phase) -> bool {
    matches!(phase, Phase::Finished | Phase::BreakFinished)
}

pub fn can_configure(s: &Shared) -> bool {
    s.running.is_none() && s.data.focus.phase == Phase::Work && s.focus_left == duration(s)
}

pub fn reset(s: &mut Shared) {
    s.running = None;
    s.focus_accounted = None;
    s.data.focus.phase = Phase::Work;
    s.focus_left = duration(s);
}

pub fn begin(s: &mut Shared, phase: Phase) {
    s.data.focus.phase = phase;
    s.focus_left = duration(s);
    s.focus_accounted = None;
    s.running = Some(Instant::now());
}

pub fn complete_if_due(s: &mut Shared) -> Option<&'static str> {
    if s.running.is_none() || crate::remaining(s) > 0 {
        return None;
    }
    crate::record_focus(s);
    s.running = None;
    s.focus_accounted = None;
    s.focus_left = 0;
    s.data.focus_remaining = Some(0);
    let phase = s.data.focus.phase;
    if phase == Phase::Work {
        let today = Local::now().format("%Y-%m-%d").to_string();
        let count = s.data.focus.completed.entry(today).or_default();
        *count = count.saturating_add(1);
        while s.data.focus.completed.len() > 90 {
            s.data.focus.completed.pop_first();
        }
    }
    let message = if phase == Phase::Break {
        s.data.focus.phase = Phase::BreakFinished;
        "Jeda selesai."
    } else {
        s.data.focus.phase = Phase::Finished;
        "Sesi fokus selesai. Ambil jeda."
    };
    s.status = message.into();
    Some(message)
}

pub fn notify_completion(s: &mut Shared, message: &'static str, panel_open: bool) {
    if !panel_open {
        s.enqueue_alert(Alert {
            text: message.into(),
            is_water: false,
            page: 1,
            url: String::new(),
        });
    }
}

pub fn task_deleted(s: &mut Session, index: usize) {
    s.task = s.task.and_then(|selected| match selected.cmp(&index) {
        std::cmp::Ordering::Less => Some(selected),
        std::cmp::Ordering::Equal => None,
        std::cmp::Ordering::Greater => Some(selected - 1),
    });
}
