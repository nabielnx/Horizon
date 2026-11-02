//! Read-only Windows battery status. No device polling thread or power changes.
use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Battery {
    pub percent: Option<u8>,
    pub charging: bool,
}
impl Battery {
    fn from_status(status: SYSTEM_POWER_STATUS) -> Self {
        let present = status.BatteryFlag != 255 && status.BatteryFlag & 128 == 0;
        let percent =
            (present && status.BatteryLifePercent <= 100).then_some(status.BatteryLifePercent);
        Self {
            percent,
            charging: percent.is_some() && status.ACLineStatus == 1 && status.BatteryFlag & 8 != 0,
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
                charging: true
            }
        );
        assert_eq!(
            sample(100, 1, 1),
            Battery {
                percent: Some(100),
                charging: false
            }
        );
        assert_eq!(
            sample(25, 2, 0),
            Battery {
                percent: Some(25),
                charging: false
            }
        );
    }
}
