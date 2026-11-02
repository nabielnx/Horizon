//! Read-only Windows battery status. No device polling thread or power changes.
use std::time::{Duration, Instant};
use windows::Win32::{
    Foundation::{HANDLE, HWND},
    System::Power::{
        GetSystemPowerStatus, HPOWERNOTIFY, RegisterPowerSettingNotification, SYSTEM_POWER_STATUS,
        UnregisterPowerSettingNotification,
    },
    UI::WindowsAndMessaging::DEVICE_NOTIFY_WINDOW_HANDLE,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Battery {
    pub percent: Option<u8>,
    pub charging: bool,
    pub plugged: Option<bool>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionChange {
    Connected,
    Disconnected,
}
pub struct Notice {
    change: ConnectionChange,
    percent: Option<u8>,
    started: Instant,
}
impl Notice {
    pub fn new(change: ConnectionChange, percent: Option<u8>, now: Instant) -> Self {
        Self {
            change,
            percent,
            started: now,
        }
    }
    pub fn expired(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.started) >= Duration::from_millis(3200)
    }
    pub fn text(&self) -> String {
        let label = match self.change {
            ConnectionChange::Connected => "Charger terhubung",
            ConnectionChange::Disconnected => "Charger dilepas",
        };
        self.percent.map_or_else(
            || label.to_string(),
            |percent| format!("{label} · {percent}%"),
        )
    }
}
pub struct Notification(HPOWERNOTIFY);
impl Notification {
    pub fn register(hwnd: HWND) -> Option<Self> {
        // GUID_ACDC_POWER_SOURCE, from the Windows SDK. This tracks connection
        // changes independently from the battery's actual charging-current flag.
        let setting = windows::core::GUID::from_u128(0x5d3e9a59_e9d5_4b00_a6bd_ff34ff516548);
        unsafe {
            RegisterPowerSettingNotification(HANDLE(hwnd.0), &setting, DEVICE_NOTIFY_WINDOW_HANDLE)
                .ok()
                .map(Self)
        }
    }
}
impl Drop for Notification {
    fn drop(&mut self) {
        unsafe {
            let _ = UnregisterPowerSettingNotification(self.0);
        }
    }
}
impl Battery {
    pub fn energized(self) -> bool {
        self.plugged == Some(true)
    }
    pub fn connection_change(self, next: Self) -> Option<ConnectionChange> {
        match (self.plugged, next.plugged) {
            (Some(false), Some(true)) => Some(ConnectionChange::Connected),
            (Some(true), Some(false)) => Some(ConnectionChange::Disconnected),
            _ => None,
        }
    }
    fn from_status(status: SYSTEM_POWER_STATUS) -> Self {
        let present = status.BatteryFlag != 255 && status.BatteryFlag & 128 == 0;
        let percent =
            (present && status.BatteryLifePercent <= 100).then_some(status.BatteryLifePercent);
        Self {
            percent,
            charging: percent.is_some() && status.ACLineStatus == 1 && status.BatteryFlag & 8 != 0,
            plugged: if present {
                match status.ACLineStatus {
                    0 => Some(false),
                    1 => Some(true),
                    _ => None,
                }
            } else {
                None
            },
        }
    }
    pub fn read() -> Option<Self> {
        let mut status = SYSTEM_POWER_STATUS::default();
        unsafe {
            GetSystemPowerStatus(&mut status).ok()?;
        }
        Some(Self::from_status(status))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn adapter_connection_is_visible_even_when_charge_current_is_zero() {
        let unplugged = Battery::from_status(SYSTEM_POWER_STATUS {
            BatteryLifePercent: 80,
            BatteryFlag: 1,
            ACLineStatus: 0,
            ..Default::default()
        });
        let connected = Battery::from_status(SYSTEM_POWER_STATUS {
            BatteryLifePercent: 80,
            BatteryFlag: 1,
            ACLineStatus: 1,
            ..Default::default()
        });
        assert!(!connected.charging);
        assert!(connected.energized());
        assert!(!unplugged.energized());
        assert_eq!(
            unplugged.connection_change(connected),
            Some(ConnectionChange::Connected)
        );
        assert_eq!(
            connected.connection_change(unplugged),
            Some(ConnectionChange::Disconnected)
        );
        assert_eq!(connected.connection_change(connected), None);
        assert_eq!(Battery::default().connection_change(connected), None);
        assert_eq!(
            connected.connection_change(Battery {
                percent: Some(79),
                ..connected
            }),
            None
        );
        let now = Instant::now();
        let notice = Notice::new(ConnectionChange::Connected, Some(80), now);
        assert_eq!(notice.text(), "Charger terhubung · 80%");
        assert!(!notice.expired(now + Duration::from_secs(3)));
        assert!(notice.expired(now + Duration::from_millis(3200)));
    }
    #[test]
    fn battery_status_distinguishes_unknown_absent_charging_and_full() {
        let sample = |percent, flag, ac| {
            Battery::from_status(SYSTEM_POWER_STATUS {
                BatteryLifePercent: percent,
                BatteryFlag: flag,
                ACLineStatus: ac,
                ..Default::default()
            })
        };
        assert_eq!(sample(255, 255, 255), Battery::default());
        assert_eq!(sample(100, 128, 1), Battery::default());
        assert_eq!(
            sample(40, 8, 1),
            Battery {
                percent: Some(40),
                charging: true,
                plugged: Some(true)
            }
        );
        assert_eq!(
            sample(100, 1, 1),
            Battery {
                percent: Some(100),
                charging: false,
                plugged: Some(true)
            }
        );
        assert_eq!(
            sample(25, 2, 0),
            Battery {
                percent: Some(25),
                charging: false,
                plugged: Some(false)
            }
        );
    }
}
