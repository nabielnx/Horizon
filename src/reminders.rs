use crate::{Alert, model::Data};
use chrono::{Local, TimeZone};
use std::collections::VecDeque;

#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Queue {
    pending: VecDeque<Alert>,
}
impl Queue {
    pub fn push(&mut self, alert: Alert) {
        self.pending.push_back(alert);
    }
    pub fn next(&mut self) -> Option<Alert> {
        self.pending.pop_front()
    }
    pub fn current(&self) -> Option<Alert> {
        self.pending.front().cloned()
    }
    pub fn remove_water(&mut self) {
        self.pending.retain(|alert| !alert.is_water);
    }
}

pub fn collect(data: &mut Data, now: i64) -> Vec<Alert> {
    let mut due = Vec::new();
    for task in &mut data.tasks {
        if !task.done && !task.notified && task.due.is_some_and(|at| at <= now) {
            task.notified = true;
            due.push(Alert {
                text: task.text.clone(),
                is_water: false,
                page: 4,
                url: String::new(),
            });
        }
    }
    for event in &mut data.events {
        if !event.notified
            && event.at.saturating_sub(
                event
                    .lead_minutes
                    .unwrap_or(data.settings.event_lead_minutes) as i64
                    * 60,
            ) <= now
        {
            event.notified = true;
            let time = Local
                .timestamp_opt(event.at, 0)
                .single()
                .map(|t| t.format("%H:%M").to_string())
                .unwrap_or_default();
            due.push(Alert {
                text: format!("{} · {}", time, event.title),
                is_water: false,
                page: 2,
                url: event.url.clone(),
            });
        }
    }
    due
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Event, Task};
    #[test]
    fn simultaneous_reminders_are_delivered_once_in_order() {
        let mut data = Data::default();
        for text in ["First", "Second"] {
            data.tasks.push(Task {
                text: text.into(),
                done: false,
                due: Some(100),
                notified: false,
                priority: 0,
            });
        }
        data.events.push(Event {
            title: "Meeting".into(),
            at: 120,
            notified: false,
            lead_minutes: Some(1),
            url: String::new(),
        });
        let mut queue = Queue::default();
        for alert in collect(&mut data, 100) {
            queue.push(alert);
        }
        let bytes = serde_json::to_vec(&queue).unwrap();
        queue = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(queue.next().unwrap().text, "First");
        assert_eq!(queue.next().unwrap().text, "Second");
        assert_eq!(queue.next().unwrap().page, 2);
        assert!(queue.next().is_none());
        assert!(collect(&mut data, 100).is_empty());
    }
}
