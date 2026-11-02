use chrono::{Local, Timelike};
use serde::{Deserialize, Serialize};
use std::{fs, io, path::PathBuf};

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    #[serde(default)]
    pub design_revision: u32,
    pub width: i32,
    pub opacity: u8,
    pub radius: i32,
    pub motion: bool,
    pub sound: bool,
    pub startup: bool,
    pub theme: usize,
    pub album_colors: bool,
    pub show_lyrics: bool,
    pub focus_minutes: u32,
    pub water_goal: u32,
    pub cup: u32,
    pub water_interval: u32,
    pub water_enabled: bool,
    pub water_start_hour: u8,
    pub water_end_hour: u8,
    pub event_lead_minutes: u32,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            design_revision: 2,
            width: 680,
            opacity: 255,
            radius: 21,
            motion: true,
            sound: true,
            startup: false,
            theme: 0,
            album_colors: true,
            show_lyrics: true,
            focus_minutes: 25,
            water_goal: 2000,
            cup: 250,
            water_interval: 60,
            water_enabled: true,
            water_start_hour: 8,
            water_end_hour: 22,
            event_lead_minutes: 5,
        }
    }
}
impl Settings {
    pub fn adjust(&mut self, key: usize, delta: i32) -> Result<(), String> {
        match key {
            0 => self.width = self.width.saturating_add(delta),
            1 => self.opacity = (self.opacity as i32 + delta).clamp(166, 255) as u8,
            2 => self.radius = self.radius.saturating_add(delta),
            3 => self.theme = (self.theme as i32 + delta).rem_euclid(3) as usize,
            4 => self.motion = !self.motion,
            5 => self.startup = !self.startup,
            6 => self.focus_minutes = (self.focus_minutes as i32 + delta).clamp(1, 180) as u32,
            7 => self.water_goal = (self.water_goal as i32 + delta).clamp(250, 10000) as u32,
            8 => self.cup = (self.cup as i32 + delta).clamp(50, 1000) as u32,
            9 => self.water_interval = (self.water_interval as i32 + delta).clamp(1, 240) as u32,
            10 => self.sound = !self.sound,
            11 => self.water_enabled = !self.water_enabled,
            12 => self.water_start_hour = (self.water_start_hour as i32 + delta).clamp(0, 23) as u8,
            13 => self.water_end_hour = (self.water_end_hour as i32 + delta).clamp(0, 23) as u8,
            14 => {
                self.event_lead_minutes =
                    (self.event_lead_minutes as i32 + delta).clamp(0, 60) as u32
            }
            15 => self.album_colors = !self.album_colors,
            16 => self.show_lyrics = !self.show_lyrics,
            _ => return Err("Pengaturan tidak dikenal.".into()),
        }
        self.normalize();
        Ok(())
    }
    pub fn normalize(&mut self) {
        if self.design_revision == 0 {
            self.width = 591;
            self.design_revision = 1;
        }
        if self.design_revision == 1 {
            if self.width == 591 {
                self.width = 680;
            }
            self.design_revision = 2;
        }
        self.width = self.width.clamp(360, 680);
        self.opacity = self.opacity.max(166);
        self.radius = self.radius.clamp(8, 32);
        self.theme %= 3;
        self.focus_minutes = self.focus_minutes.clamp(1, 180);
        self.water_goal = self.water_goal.clamp(250, 10000);
        self.cup = self.cup.clamp(50, 1000);
        self.water_interval = self.water_interval.clamp(1, 240);
        self.water_start_hour = self.water_start_hour.min(23);
        self.water_end_hour = self.water_end_hour.min(23);
        self.event_lead_minutes = self.event_lead_minutes.min(60);
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Task {
    pub text: String,
    pub done: bool,
    #[serde(default)]
    pub due: Option<i64>,
    #[serde(default)]
    pub notified: bool,
    #[serde(default)]
    pub priority: u8,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Event {
    pub title: String,
    pub at: i64,
    pub notified: bool,
    #[serde(default)]
    pub lead_minutes: Option<u32>,
    #[serde(default)]
    pub url: String,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct ShelfFile {
    pub path: String,
    pub name: String,
    pub size: u64,
}
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Data {
    pub focus_history: std::collections::BTreeMap<String, u64>,
    pub reminders: crate::reminders::Queue,
    pub focus_remaining: Option<u64>,
    pub settings: Settings,
    pub tasks: Vec<Task>,
    pub events: Vec<Event>,
    pub water: u32,
    pub day: String,
    pub water_due: i64,
    pub favorites: Vec<String>,
    #[serde(default)]
    pub shelf_files: Vec<ShelfFile>,
}
impl Data {
    pub fn path() -> PathBuf {
        PathBuf::from(std::env::var_os("APPDATA").unwrap_or_default())
            .join("Horizon")
            .join("data.json")
    }
    pub fn load(path: &std::path::Path) -> io::Result<Self> {
        let mut data: Self = if path.exists() {
            serde_json::from_slice(&fs::read(path)?).map_err(io::Error::other)?
        } else {
            Self::default()
        };
        data.settings.normalize();
        data.reset_day();
        Ok(data)
    }
    pub fn reset_day(&mut self) {
        let today = Local::now().format("%Y-%m-%d").to_string();
        if self.day != today {
            self.day = today;
            self.water = 0;
        }
        if self.water_due == 0 {
            self.water_due = Local::now().timestamp() + self.settings.water_interval as i64 * 60;
        }
    }
    pub fn save(&self, path: &std::path::Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let temporary = path.with_extension("tmp");
        let bytes = serde_json::to_vec_pretty(self).map_err(io::Error::other)?;
        use std::io::Write;
        let mut file = fs::File::create(&temporary)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        if path.exists() {
            fs::copy(path, path.with_extension("bak"))?;
        }
        // MoveFileEx provides replace semantics on Windows without deleting the original first.
        #[cfg(windows)]
        unsafe {
            use std::os::windows::ffi::OsStrExt;
            use windows::{
                Win32::Storage::FileSystem::{
                    MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
                },
                core::PCWSTR,
            };
            let a: Vec<u16> = temporary.as_os_str().encode_wide().chain(Some(0)).collect();
            let b: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
            MoveFileExW(
                PCWSTR(a.as_ptr()),
                PCWSTR(b.as_ptr()),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
            .map_err(io::Error::other)?;
        }
        Ok(())
    }
    pub fn water_tick(&mut self) -> bool {
        let now = Local::now();
        if self.settings.water_enabled
            && self.water < self.settings.water_goal
            && active_hours(
                now.hour() as u8,
                self.settings.water_start_hour,
                self.settings.water_end_hour,
            )
            && now.timestamp() >= self.water_due
        {
            self.water_due = now.timestamp() + self.settings.water_interval as i64 * 60;
            true
        } else {
            false
        }
    }
    pub fn add_shelf_file(&mut self, path: String) {
        let p = std::path::Path::new(&path);
        let name = p
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.clone());
        let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        if !self.shelf_files.iter().any(|f| f.path == path) {
            self.shelf_files.push(ShelfFile { path, name, size });
        }
    }
}
pub fn active_hours(hour: u8, start: u8, end: u8) -> bool {
    start == end
        || if start < end {
            hour >= start && hour < end
        } else {
            hour >= start || hour < end
        }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounds_are_safe() {
        let mut s = Settings {
            width: 1,
            water_interval: 0,
            theme: 99,
            ..Default::default()
        };
        s.normalize();
        assert_eq!(s.width, 360);
        assert_eq!(s.water_interval, 1);
        assert_eq!(s.theme, 0);
    }
    #[test]
    fn damaged_data_is_not_replaced() {
        let p = std::env::temp_dir().join(format!("horizon-corrupt-{}.json", std::process::id()));
        fs::write(&p, b"broken").expect("fixture");
        assert!(Data::load(&p).is_err());
        assert_eq!(fs::read(&p).expect("fixture"), b"broken");
        fs::remove_file(p).expect("cleanup");
    }
    #[test]
    fn data_round_trip() {
        let p = std::env::temp_dir().join(format!("horizon-roundtrip-{}.json", std::process::id()));
        let mut d = Data::default();
        d.reset_day();
        d.settings.water_interval = 1;
        d.tasks.push(Task {
            text: "Keep me".into(),
            done: false,
            due: None,
            notified: false,
            priority: 0,
        });
        d.save(&p).expect("save");
        let r = Data::load(&p).expect("load");
        assert_eq!(r.settings.water_interval, 1);
        assert_eq!(r.tasks[0].text, "Keep me");
        fs::remove_file(p).expect("cleanup");
    }
}
