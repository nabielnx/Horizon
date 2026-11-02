use std::time::{Duration, Instant};

pub const FRAME: Duration = Duration::from_nanos(8_333_333);
pub const HOVER: f32 = 0.18;
pub const PRESS: f32 = 0.09;
pub const STATE: f32 = 0.22;
pub const CONTENT: f32 = 0.16;

/// Retarget from the visible value, including rapid pointer reversals.
pub struct Tween {
    from: f32,
    target: f32,
    started: Instant,
    duration: f32,
}
impl Tween {
    pub fn new(value: f32, now: Instant) -> Self {
        Self {
            from: value,
            target: value,
            started: now,
            duration: STATE,
        }
    }
    pub fn sample(&self, now: Instant) -> f32 {
        let t = (now.saturating_duration_since(self.started).as_secs_f32() / self.duration)
            .clamp(0., 1.);
        self.from + (self.target - self.from) * ease(t)
    }
    pub fn retarget(&mut self, value: f32, duration: f32, enabled: bool, now: Instant) -> f32 {
        if !enabled {
            self.from = value;
            self.target = value;
        } else if self.target != value {
            self.from = self.sample(now);
            self.target = value;
            self.started = now;
            self.duration = duration.max(0.001);
        }
        self.sample(now)
    }
    pub fn active(&self, now: Instant) -> bool {
        self.from != self.target
            && now.saturating_duration_since(self.started).as_secs_f32() < self.duration
    }
}

/// Smooth organic quartic ease-out for 120Hz fluid, responsive opening
pub fn ease(t: f32) -> f32 {
    let t = t.clamp(0., 1.);
    1. - (1. - t).powi(4)
}

/// Exact critically damped integration, retaining velocity when a hover reverses.
pub fn settle(value: &mut f32, velocity: &mut f32, target: f32, dt: f32) {
    let omega = 26.;
    let offset = *value - target;
    let c = *velocity + omega * offset;
    let decay = (-omega * dt).exp();
    *value = target + (offset + c * dt) * decay;
    *velocity = (*velocity - omega * c * dt) * decay;
    if !(0. ..=1.).contains(value) {
        *value = value.clamp(0., 1.);
        *velocity = 0.;
    }
    if (*value - target).abs() < 0.001 && velocity.abs() < 0.01 {
        *value = target;
        *velocity = 0.;
    }
}

pub struct Palette {
    from: [u32; 3],
    to: [u32; 3],
    started: Option<Instant>,
}
impl Default for Palette {
    fn default() -> Self {
        let color = crate::render::palette(0);
        Self {
            from: color,
            to: color,
            started: None,
        }
    }
}
impl Palette {
    pub fn retarget(&mut self, color: [u32; 3], animate: bool, now: Instant) {
        if color != self.to {
            self.from = self.sample(now);
            self.to = color;
            self.started = animate.then_some(now);
        }
        if !animate {
            self.started = None;
        }
    }
    pub fn active(&self, now: Instant) -> bool {
        self.started
            .is_some_and(|s| now.duration_since(s).as_millis() < 550)
    }
    pub fn sample(&self, now: Instant) -> [u32; 3] {
        let Some(started) = self.started else {
            return self.to;
        };
        let t = (now.duration_since(started).as_secs_f32() / 0.55).clamp(0., 1.);
        let t = if t < 0.5 {
            4. * t * t * t
        } else {
            1. - (-2. * t + 2.).powi(3) / 2.
        };
        std::array::from_fn(|i| mix(self.from[i], self.to[i], t))
    }
}
pub fn mix(a: u32, b: u32, t: f32) -> u32 {
    let t = t.clamp(0., 1.);
    [0, 8, 16]
        .into_iter()
        .map(|shift| {
            let first = ((a >> shift) & 255) as f32;
            let next = ((b >> shift) & 255) as f32;
            ((first + (next - first) * t).round() as u32) << shift
        })
        .sum()
}

/// One waitable timer in the UI message pump. No worker thread, polling loop,
/// process-wide timer resolution change, or timer wakeups while static.
pub struct FrameClock(windows::Win32::Foundation::HANDLE);
impl FrameClock {
    pub fn new() -> windows::core::Result<Self> {
        use windows::{Win32::System::Threading::CreateWaitableTimerExW, core::PCWSTR};
        unsafe {
            CreateWaitableTimerExW(None, PCWSTR::null(), 2, 0x1f0003)
                .or_else(|_| CreateWaitableTimerExW(None, PCWSTR::null(), 0, 0x1f0003))
                .map(Self)
        }
    }
    pub fn handle(&self) -> windows::Win32::Foundation::HANDLE {
        self.0
    }
    pub fn arm(&self, wait: Duration) -> windows::core::Result<()> {
        let due = -((wait.as_nanos() / 100).max(1).min(i64::MAX as u128) as i64);
        unsafe {
            windows::Win32::System::Threading::SetWaitableTimer(self.0, &due, 0, None, None, false)
        }
    }
    pub fn cancel(&self) {
        unsafe {
            let _ = windows::Win32::System::Threading::CancelWaitableTimer(self.0);
        }
    }
}
impl Drop for FrameClock {
    fn drop(&mut self) {
        unsafe {
            let _ = windows::Win32::Foundation::CloseHandle(self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn interaction_reverses_without_jumping_and_stops() {
        let start = Instant::now();
        let mut value = Tween::new(0., start);
        assert_eq!(value.retarget(1., HOVER, true, start), 0.);
        let halfway = start + Duration::from_millis(70);
        let visible = value.sample(halfway);
        assert!(visible > 0. && visible < 1.);
        assert_eq!(value.retarget(0., HOVER, true, halfway), visible);
        assert!(value.active(halfway));
        let end = halfway + Duration::from_millis(200);
        assert_eq!(value.sample(end), 0.);
        assert!(!value.active(end));
        assert_eq!(value.retarget(1., STATE, false, end), 1.);
        assert!(!value.active(end));
    }
    #[test]
    fn cubic_ease_is_monotonic_and_bounded() {
        assert_eq!(ease(0.), 0.);
        assert_eq!(ease(1.), 1.);
        let mut prev = -0.01;
        for i in 0..=100 {
            let t = i as f32 / 100.;
            let val = ease(t);
            assert!(val >= prev);
            assert!((0. ..=1.).contains(&val));
            prev = val;
        }
    }

    #[test]
    fn damping_is_frame_rate_independent_and_survives_reversal() {
        let simulate = |hz: u32| {
            let (mut value, mut velocity) = (0., 0.);
            for _ in 0..hz / 5 {
                settle(&mut value, &mut velocity, 1., 1. / hz as f32);
            }
            value
        };
        assert!((simulate(60) - simulate(120)).abs() < 0.0001);
        let (mut value, mut velocity) = (0.4, 2.);
        settle(&mut value, &mut velocity, 0., 0.001);
        assert!(value > 0.4 && velocity > 0.);
        for _ in 0..120 {
            settle(&mut value, &mut velocity, 0., 1. / 120.);
            assert!((0. ..=1.).contains(&value));
        }
        assert_eq!((value, velocity), (0., 0.));
    }

    #[test]
    fn palette_retargets_from_visible_color_and_finishes() {
        let now = Instant::now();
        let mut p = Palette::default();
        let original = p.sample(now);
        p.retarget([0x442233; 3], true, now);
        assert_eq!(p.sample(now), original);
        let half = now + Duration::from_millis(275);
        let visible = p.sample(half);
        p.retarget([0x334422; 3], true, half);
        assert_eq!(p.sample(half), visible);
        assert_eq!(p.sample(half + Duration::from_millis(550)), [0x334422; 3]);
        assert!(!p.active(half + Duration::from_millis(550)));
        p.retarget([0; 3], false, half);
        assert_eq!(p.sample(half), [0; 3]);
    }
}
