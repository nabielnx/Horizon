//! Seated full-body dusk companion. Poses are deterministic; ground contacts
//! belong to the rig, not a floating whole-character transform.
use std::time::{Duration, Instant};

#[derive(Clone, Copy)]
pub struct EyePose {
    pub open: [f32; 2],
    pub look: (f32, f32),
    pub sleep: f32,
    pub sleep_phase: f32,
}

impl Default for EyePose {
    fn default() -> Self {
        Self {
            open: [1.; 2],
            look: (0., 0.),
            sleep: 0.,
            sleep_phase: 0.,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub breath: f32,
    pub body_sway: f32,
    pub head_tilt: f32,
    pub head_lag: f32,
    pub eyes: f32,
    pub look: (f32, f32),
    pub joy: f32,
    pub yawn: f32,
    pub rest: f32,
    pub wave: f32,
    pub follow: f32,
    pub raise: f32,
    pub curl: f32,
    pub sit: f32,
}
impl Default for Pose {
    fn default() -> Self {
        sample(0., (0., 0.), None, false, false)
    }
}
pub struct Companion {
    started: Instant,
    touched: Instant,
    pet: Option<Instant>,
    greeting: Instant,
    last_visible: Option<Instant>,
    hovered: bool,
    cursor: Option<(i32, i32)>,
    look: [crate::motion::Tween; 2],
    rest_blend: crate::motion::Tween,
    curl_blend: crate::motion::Tween,
}
impl Default for Companion {
    fn default() -> Self {
        let now = Instant::now();
        Self {
            started: now,
            touched: now,
            pet: None,
            greeting: now,
            last_visible: None,
            hovered: false,
            cursor: None,
            look: std::array::from_fn(|_| crate::motion::Tween::new(0.5, now)),
            rest_blend: crate::motion::Tween::new(0., now),
            curl_blend: crate::motion::Tween::new(0., now),
        }
    }
}
impl Companion {
    pub fn hide(&mut self) {
        self.last_visible = None;
        self.hovered = false;
        self.cursor = None;
    }
    pub fn observe_cursor(&mut self, position: (i32, i32), now: Instant) -> bool {
        let moved = self
            .cursor
            .is_some_and(|(x, y)| x.abs_diff(position.0).max(y.abs_diff(position.1)) >= 2);
        if moved || self.cursor.is_none() {
            self.cursor = Some(position);
        }
        if moved {
            self.touched = now;
        }
        moved
    }
    pub fn ambient_interval(&self, now: Instant) -> Duration {
        if self.rest_blend.sample(now) < 1. {
            Duration::from_millis(33)
        } else {
            Duration::from_millis(50)
        }
    }
    pub fn eyes(&mut self, motion: bool, hovered: bool, look: (f32, f32)) -> EyePose {
        self.eyes_at(motion, hovered, look, Instant::now())
    }
    fn eyes_at(&mut self, motion: bool, hovered: bool, look: (f32, f32), now: Instant) -> EyePose {
        if !motion {
            self.hide();
            return EyePose::default();
        }
        let pose = self.pose_at(true, hovered, look, now);
        let elapsed = now.duration_since(self.started).as_secs_f64();
        let opening = smooth(elapsed as f32 / 0.42);
        let cue = self.pet.map_or(self.greeting, |pet| pet.max(self.greeting));
        let age = now.duration_since(cue).as_secs_f32();
        EyePose {
            open: [0.19, 0.26].map(|offset| {
                let phase = ((age - offset) / 0.16).clamp(0., 1.);
                let blink = (phase * std::f32::consts::PI).sin().powi(2);
                opening * pose.eyes * (1. - pose.rest) * (1. - blink * 0.96)
            }),
            look: pose.look,
            sleep: smooth((pose.rest - 0.55) / 0.45),
            sleep_phase: (elapsed.rem_euclid(1.8) / 1.8) as f32,
        }
    }
    pub fn reacting(&self) -> bool {
        let now = Instant::now();
        self.pet
            .is_some_and(|start| now.duration_since(start).as_secs_f32() < 1.6)
            || now.duration_since(self.greeting).as_secs_f32() < 3.6
            || self.curl_blend.active(now)
    }
    pub fn pet(&mut self) {
        let now = Instant::now();
        self.pet = Some(now);
        self.touched = now;
    }
    fn pose_at(&mut self, motion: bool, hovered: bool, look: (f32, f32), now: Instant) -> Pose {
        if !motion {
            self.last_visible = None;
            self.hovered = hovered;
            return Pose::default();
        }
        // Time spent hidden is not visible idle time. Every reveal starts awake
        // and welcomes the user, even after hours behind an active music view.
        if self.last_visible.is_none() {
            self.started = now;
            self.touched = now;
            self.greeting = now;
            self.rest_blend = crate::motion::Tween::new(0., now);
            self.curl_blend = crate::motion::Tween::new(0., now);
        } else if hovered && !self.hovered && now.duration_since(self.greeting).as_secs() >= 6 {
            self.greeting = now;
        }
        self.last_visible = Some(now);
        self.hovered = hovered;
        if hovered {
            self.touched = now;
        }
        let mut gaze = [0.; 2];
        for ((axis, output), target) in self.look.iter_mut().zip(&mut gaze).zip([look.0, look.1]) {
            axis.retarget(
                (target.clamp(-1., 1.) + 1.) * 0.5,
                crate::motion::HOVER,
                motion,
                now,
            );
            *output = axis.sample(now) * 2. - 1.;
        }
        let idle_seconds = now.duration_since(self.touched).as_secs_f32();
        let resting = self.rest_blend.retarget(
            f32::from(idle_seconds >= 33.),
            crate::motion::STATE,
            true,
            now,
        );
        let mut pose = sample_weighted(
            now.duration_since(self.started).as_secs_f32(),
            (gaze[0], gaze[1]),
            self.pet
                .map(|start| now.duration_since(start).as_secs_f32()),
            idle_seconds,
            resting,
            motion,
        );
        if motion && self.greeting != self.started {
            greet(
                &mut pose,
                now.duration_since(self.greeting).as_secs_f32(),
                1.,
            );
        }
        pose.curl = self
            .curl_blend
            .retarget(f32::from(idle_seconds >= 33.), 1.2, true, now);
        pose.sit *= 1. - pose.curl;
        // Finish unfolding before lifting the free hand into a greeting.
        pose.raise *= 1. - pose.curl;
        pose.wave *= 1. - pose.curl;
        pose.follow *= 1. - pose.curl;
        pose
    }
}

pub fn cursor_look(position: (i32, i32), bounds: (f32, f32, f32, f32)) -> (f32, f32) {
    let (x, y, width, height) = bounds;
    let dx = position.0 as f32 - (x + width / 2.);
    let dy = position.1 as f32 - (y + height / 2.);
    let radius = dx.hypot(dy).max(width);
    (dx / radius, dy / radius)
}

fn smooth(value: f32) -> f32 {
    let t = value.clamp(0., 1.);
    t * t * (3. - 2. * t)
}
fn moment(t: f32, start: f32, end: f32) -> f32 {
    smooth((t - start) / 0.65) * smooth((end - t) / 0.75)
}
fn greet(pose: &mut Pose, seconds: f32, strength: f32) {
    let weight = moment(seconds, 0., 3.6) * strength;
    if weight <= 0. {
        return;
    }
    pose.raise = pose.raise.max(weight);
    pose.sit = pose.sit.max(weight * 0.7);
    pose.wave = (seconds * 4.6).sin() * weight;
    pose.follow = (seconds * 4.6 - 0.65).sin() * weight;
    pose.body_sway -= (seconds * 4.6 - 0.35).sin() * 0.25 * weight;
    pose.joy = pose.joy.max(weight * 0.4);
    pose.rest *= 1. - weight;
    pose.yawn *= 1. - weight;
    pose.eyes = pose.eyes.max(weight * 0.9);
}
pub fn sample(
    seconds: f32,
    look: (f32, f32),
    pet: Option<f32>,
    rest_allowed: bool,
    motion: bool,
) -> Pose {
    sample_weighted(
        seconds,
        look,
        pet,
        if rest_allowed { seconds } else { 0. },
        if rest_allowed {
            smooth((seconds - 33.) / 0.6)
        } else {
            0.
        },
        motion,
    )
}
fn sample_weighted(
    seconds: f32,
    look: (f32, f32),
    pet: Option<f32>,
    idle_seconds: f32,
    rest_weight: f32,
    motion: bool,
) -> Pose {
    if !motion {
        return Pose {
            breath: 1.,
            body_sway: 0.,
            head_tilt: 0.,
            head_lag: 0.,
            eyes: 1.,
            look: (0., 0.),
            joy: 0.,
            yawn: 0.,
            rest: 0.,
            wave: 0.,
            follow: 0.,
            raise: 0.,
            curl: 0.,
            sit: 0.,
        };
    }
    let t = seconds.max(0.);
    let blink_phase = t.rem_euclid(7.3);
    let blink = if blink_phase > 5.7 && blink_phase < 5.94 {
        ((blink_phase - 5.7) / 0.24 * std::f32::consts::PI)
            .sin()
            .powi(2)
    } else {
        0.
    };
    let cycle = t.rem_euclid(48.);
    let rest = rest_weight.clamp(0., 1.);
    let awake = 1. - rest;
    let curious = moment(cycle, 8., 12.5) * awake;
    let delight = pet.map_or(0., |seconds| moment(seconds, 0., 1.6));
    let joy = moment(cycle, 16., 20.).max(delight) * awake;
    let tired = moment(idle_seconds, 29., 33.5);
    let mut pose = Pose {
        breath: 1.
            + (t * 1.35).sin() * 0.012 * awake
            + (t * 0.85).sin() * 0.008 * rest
            + delight * 0.018 * awake,
        body_sway: (t * 0.95).sin() * (0.55 - 0.43 * rest) - delight * 0.2 * awake,
        head_tilt: ((t * 0.7).sin() * 0.9 + look.0 * 1.2 + curious * (t * 0.85).sin() * 2.) * awake
            - rest * 6.,
        head_lag: (t * 0.95 - 0.7).sin() * 0.45 * (1. - rest * 0.75),
        eyes: (1. - blink).max(0.05) * (1. - tired * 0.4) * (1. - rest * 0.95),
        look: (
            look.0 * awake + curious * (t * 0.85).sin() * 0.5,
            look.1 * awake - curious * 0.25,
        ),
        joy,
        yawn: moment(cycle, 30., 32.5) * tired,
        rest,
        wave: 0.,
        follow: 0.,
        raise: 0.,
        curl: smooth((idle_seconds - 33.) / 1.8) * rest,
        sit: curious,
    };
    greet(&mut pose, cycle, awake);
    pose
}
#[cfg(test)]
pub fn layer(id: &str) -> String {
    path_layer(include_str!("../assets/branding/mascot.svg"), id)
}
#[cfg(test)]
pub fn sleep_layer(id: &str) -> String {
    path_layer(include_str!("../assets/branding/mascot-sleep.svg"), id)
}
#[cfg(test)]
fn path_layer(source: &str, id: &str) -> String {
    let tag = source
        .split('<')
        .find(|tag| tag.starts_with(&format!("path id=\"{id}\"")))
        .expect("mascot master layer exists")
        .split('>')
        .next()
        .unwrap();
    format!("<svg viewBox=\"0 0 200 120\"><{tag}></svg>")
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cursor_look_preserves_direction_far_outside_the_window() {
        let bounds = (10., 10., 60., 40.);
        assert_eq!(cursor_look((40, 30), bounds), (0., 0.));
        let (x, y) = cursor_look((-960, 130), bounds);
        assert!(x < -0.98 && (0.09..0.11).contains(&y));
        let (x, y) = cursor_look((240, -370), bounds);
        assert!(x > 0.4 && y < -0.85);
    }
    #[test]
    fn global_cursor_movement_wakes_the_eyes_after_sleep() {
        use std::time::Duration;
        let start = Instant::now();
        let at = |seconds| start + Duration::from_secs(seconds);
        let mut companion = Companion::default();
        let pointer = (-700, 600);
        assert!(!companion.observe_cursor(pointer, start));
        companion.eyes_at(true, false, (0., 0.), start);
        assert!(!companion.observe_cursor(pointer, at(34)));
        companion.eyes_at(true, false, (-0.7, 0.7), at(34));
        assert_eq!(
            companion.eyes_at(true, false, (-0.7, 0.7), at(36)).open,
            [0.; 2]
        );
        assert_eq!(
            companion.ambient_interval(at(36)),
            Duration::from_millis(50)
        );
        assert!(!companion.observe_cursor((-699, 600), at(37)));
        assert!(companion.observe_cursor((600, 420), at(38)));
        companion.eyes_at(true, false, (0.8, 0.6), at(38));
        let awake = companion.eyes_at(true, false, (0.8, 0.6), at(39));
        assert!(awake.open.iter().all(|open| *open > 0.8) && awake.look.0 > 0.5);
        assert_eq!(
            companion.ambient_interval(at(39)),
            Duration::from_millis(33)
        );
    }
    #[test]
    fn eye_slits_open_wink_asymmetrically_and_remain_closed_until_woken() {
        use std::time::Duration;
        let start = Instant::now();
        let mut presence = Companion::default();
        let mut sample = |seconds, hovered| {
            presence.eyes_at(
                true,
                hovered,
                (0.8, 0.),
                start + Duration::from_secs_f32(seconds),
            )
        };
        assert_eq!(sample(0., false).open, [0.; 2]);
        let greeting = sample(0.28, false);
        assert!(greeting.open[1] > greeting.open[0] + 0.25);
        assert_eq!(greeting.sleep, 0.);
        assert!(sample(0.8, false).open.iter().all(|open| *open > 0.85));
        sample(34., false);
        let asleep = sample(36., false);
        assert_eq!(asleep.open, [0.; 2]);
        assert_eq!(asleep.sleep, 1.);
        assert_eq!(sample(300., false).open, [0.; 2]);
        sample(301., true);
        let awake = sample(302., true);
        assert!(awake.open.iter().all(|open| *open > 0.8) && awake.look.0 > 0.5);
        assert_eq!(awake.sleep, 0.);
    }
    #[test]
    fn moods_wake_and_reduced_motion_is_static() {
        assert!(sample(5.82, (0., 0.), None, false, true).eyes < 0.1);
        assert!(sample(1., (0., 0.), None, true, true).raise > 0.9);
        assert!(sample(18., (0., 0.), None, true, true).joy > 0.9);
        assert!(sample(31., (0., 0.), None, true, true).yawn > 0.9);
        assert!(sample(34.5, (0., 0.), None, true, true).rest > 0.9);
        assert!(sample(40., (0., 0.), None, true, true).rest > 0.9);
        assert!(sample(40., (0., 0.), None, false, true).eyes > 0.9);
        assert_eq!(
            sample(1., (1., 1.), Some(0.5), true, false),
            Pose::default()
        );
    }
    #[test]
    fn reveal_after_hidden_time_wakes_the_character() {
        let mut companion = Companion::default();
        let old = Instant::now() - std::time::Duration::from_secs(200);
        companion.started = old;
        companion.touched = old;
        companion.last_visible = Some(old);
        companion.hide();
        let pose = companion.pose_at(true, false, (0., 0.), Instant::now());
        assert!(pose.eyes > 0.9 && pose.rest == 0. && companion.reacting());
    }
    #[test]
    fn unattended_sleep_persists_until_hover_or_click() {
        let mut companion = Companion::default();
        let start = Instant::now();
        for frame in 0..9000 {
            let seconds = frame as f32 / 30.;
            let pose = companion.pose_at(
                true,
                false,
                (0., 0.),
                start + std::time::Duration::from_secs_f32(seconds),
            );
            if seconds >= 34. {
                assert!(
                    pose.rest > 0.99 && pose.eyes < 0.1 && pose.raise == 0. && pose.joy == 0.,
                    "woke without interaction at {seconds}"
                );
            }
            if seconds >= 36. {
                assert!(pose.curl > 0.99, "sleep must use the curled body pose");
            }
        }
        for frame in 0..60 {
            let pose = companion.pose_at(
                true,
                true,
                (0., 0.),
                start + std::time::Duration::from_secs_f32(300. + frame as f32 / 30.),
            );
            if frame > 15 {
                assert!(pose.rest < 0.01 && pose.eyes > 0.8);
            }
            if frame > 45 {
                assert!(pose.curl < 0.01, "waking must unfold the body");
            }
        }
        companion.touched = Instant::now() - std::time::Duration::from_secs(60);
        companion.pet();
        assert!(companion.touched.elapsed().as_secs_f32() < 0.1);
    }
}
