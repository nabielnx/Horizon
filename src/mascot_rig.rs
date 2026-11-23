//! Full-body seated rig. Feet, hip and supporting elbow remain on the notch;
//! the free arm uses a two-bone solve with a delayed wrist gesture.
use crate::mascot::Pose;
pub type Point = (f32, f32);
pub const HIP: Point = (100., 120.);
pub const ELBOW: Point = (35., 84.);
pub const SUPPORT_HAND: Point = (48., 96.);
pub const SOLES: [Point; 2] = [(150., 120.), (180., 120.)];
pub fn distance(a: Point, b: Point) -> f32 {
    (a.0 - b.0).hypot(a.1 - b.1)
}
fn add(a: Point, b: Point) -> Point {
    (a.0 + b.0, a.1 + b.1)
}
fn sub(a: Point, b: Point) -> Point {
    (a.0 - b.0, a.1 - b.1)
}
fn scale(a: Point, n: f32) -> Point {
    (a.0 * n, a.1 * n)
}
fn mix(a: Point, b: Point, t: f32) -> Point {
    add(a, scale(sub(b, a), t))
}
fn rotate(point: Point, degrees: f32) -> Point {
    let (sin, cos) = degrees.to_radians().sin_cos();
    (point.0 * cos - point.1 * sin, point.0 * sin + point.1 * cos)
}
pub struct Rig {
    pub pose: Pose,
    pub shoulder: Point,
    pub elbow: Point,
    pub wrist: Point,
    pub hand_angle: f32,
}
impl Rig {
    pub fn new(pose: Pose) -> Self {
        let mut rig = Self {
            pose,
            shoulder: (0., 0.),
            elbow: (0., 0.),
            wrist: (0., 0.),
            hand_angle: 0.,
        };
        let shoulder = mix(rig.torso((109., 84.)), (137., 96.), pose.curl);
        let (curl, sit) = rig.weights();
        let resting = mix(mix((114., 101.), (118., 112.), sit), (110., 113.), curl);
        let target = mix(
            resting,
            (137. + pose.wave * 3., 53. + pose.follow * 1.5),
            pose.raise,
        );
        let delta = sub(target, shoulder);
        let d = distance(target, shoulder).clamp(2.01, 45.99);
        let direction = scale(delta, 1. / distance(target, shoulder).max(0.001));
        let along = (24_f32.powi(2) - 22_f32.powi(2) + d * d) / (2. * d);
        let height = (24_f32.powi(2) - along * along).max(0.).sqrt();
        rig.shoulder = shoulder;
        rig.elbow = add(
            add(shoulder, scale(direction, along)),
            (-direction.1 * height, direction.0 * height),
        );
        rig.wrist = add(shoulder, scale(direction, d));
        rig.hand_angle = (rig.wrist.1 - rig.elbow.1)
            .atan2(rig.wrist.0 - rig.elbow.0)
            .to_degrees()
            + pose.follow * 7.;
        rig.hand_angle *= 1. - curl;
        rig
    }
    pub fn torso(&self, point: Point) -> Point {
        let (x, y) = point;
        let (curl, sit) = self.weights();
        let local = (
            (x - HIP.0) / self.pose.breath.sqrt() + (HIP.1 - y) * self.pose.body_sway * 0.018,
            (y - HIP.1) * self.pose.breath,
        );
        let local = scale(rotate(local, 15. * sit - 24. * curl), 1. + 0.04 * curl);
        add(mix(HIP, (162., 116.), curl), local)
    }
    pub fn head(&self, point: Point) -> Point {
        let neck = (87., 77.);
        let pivot = self.torso(neck);
        let (curl, sit) = self.weights();
        let pivot = mix(
            pivot,
            (
                109. + self.pose.body_sway * 0.2,
                112. + (self.pose.breath - 1.) * 20.,
            ),
            curl,
        );
        let angle = ((self.pose.head_tilt + self.pose.head_lag) * (1. - 0.8 * curl)
            + sit * 2.
            + curl * 15.)
            .to_radians();
        let (sin, cos) = angle.sin_cos();
        let (x, y) = sub(point, neck);
        (pivot.0 + x * cos - y * sin, pivot.1 + x * sin + y * cos)
    }
    pub fn support(&self, point: Point) -> Point {
        // Only the shoulder end follows the breathing torso. Elbow and
        // hanging hand stay fixed, bending the upper arm between contacts.
        let (curl, sit) = self.weights();
        let shoulder = (79., 80.);
        let elbow = mix(mix(ELBOW, (75., 97.), sit), (100., 115.), curl);
        let hand = mix(mix(SUPPORT_HAND, (81., 114.), sit), (116., 111.), curl);
        let anchors = [shoulder, ELBOW, SUPPORT_HAND];
        let moved = [self.torso(shoulder), elbow, hand];
        let mut sum = 0.;
        let mut offset = (0., 0.);
        for (source, target) in anchors.into_iter().zip(moved) {
            let d = distance(point, source);
            if d < 0.001 {
                return target;
            }
            let w = 1. / d.powi(4);
            sum += w;
            offset = add(offset, scale(sub(target, source), w));
        }
        add(point, scale(offset, 1. / sum))
    }
    fn weights(&self) -> (f32, f32) {
        let curl = self.pose.curl.clamp(0., 1.);
        (curl, self.pose.sit.clamp(0., 1.) * (1. - curl))
    }
    pub fn foot(&self, point: Point, front: bool) -> Point {
        let (curl, sit) = self.weights();
        let index = usize::from(front);
        let resting = SOLES[index];
        let sitting = if front { (94., 120.) } else { (131., 120.) };
        let sleeping = if front { (188., 120.) } else { (188., 106.) };
        let angle = if front {
            12. * sit + 5. * curl
        } else {
            -12. * sit - 55. * curl
        };
        add(
            mix(mix(resting, sitting, sit), sleeping, curl),
            rotate(sub(point, resting), angle),
        )
    }
    pub fn leg(&self, point: Point, front: bool) -> Point {
        let root = if front { (110., 108.) } else { (105., 101.) };
        let ankle = if front { (150., 115.) } else { (140., 113.) };
        let axis = sub(ankle, root);
        let p = sub(point, root);
        let t = ((p.0 * axis.0 + p.1 * axis.1) / (axis.0 * axis.0 + axis.1 * axis.1)).clamp(0., 1.);
        let root_delta = sub(self.torso(root), root);
        let foot_delta = sub(self.foot(ankle, front), ankle);
        add(point, mix(root_delta, foot_delta, t * t * (3. - 2. * t)))
    }
    pub fn arm_controls(&self) -> [Point; 7] {
        let root = self.shoulder;
        let elbow = self.elbow;
        let hand = self.wrist;
        let tangent = scale(sub(hand, root), 0.14);
        [
            root,
            add(root, scale(sub(elbow, root), 0.55)),
            sub(elbow, tangent),
            elbow,
            add(elbow, tangent),
            sub(hand, scale(sub(hand, elbow), 0.45)),
            hand,
        ]
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn contacts_and_arm_lengths_hold_through_all_moods() {
        let mut previous: Option<[Point; 4]> = None;
        for frame in 0..60_000 {
            let rig = Rig::new(crate::mascot::sample(
                frame as f32 * 0.001,
                (0., 0.),
                None,
                true,
                true,
            ));
            if rig.pose.curl == 0. && rig.pose.sit == 0. {
                assert_eq!(rig.torso(HIP), HIP);
                assert_eq!(rig.support(ELBOW), ELBOW);
                assert_eq!(rig.support(SUPPORT_HAND), SUPPORT_HAND);
                let contact = rig.support((29., 89.));
                let gap =
                    24. - distance((contact.0 * 50. / 120., contact.1 * 50. / 120.), (33., 26.));
                assert!((0.0..0.8).contains(&gap));
            }
            assert!(
                (rig.foot(SOLES[1], true).1 - 120.).abs() < 0.001,
                "front sole stays grounded"
            );
            assert!((distance(rig.shoulder, rig.elbow) - 24.).abs() < 0.001);
            assert!((distance(rig.elbow, rig.wrist) - 22.).abs() < 0.001);
            let points = [
                rig.head((88., 6.)),
                rig.head((66., 51.)),
                rig.wrist,
                rig.elbow,
            ];
            if let Some(old) = previous {
                for (a, b) in old.into_iter().zip(points) {
                    assert!(distance(a, b) < 0.5, "pose jumped at {frame}");
                }
            }
            previous = Some(points);
        }
    }
    #[test]
    fn full_body_postures_move_head_hips_and_feet() {
        let lounge = Rig::new(Pose::default());
        let upright = Rig::new(Pose {
            sit: 1.,
            ..Default::default()
        });
        let asleep = Rig::new(Pose {
            curl: 1.,
            rest: 1.,
            ..Default::default()
        });
        assert!(
            distance(
                lounge.foot((170., 110.), true),
                upright.foot((170., 110.), true)
            ) > 70.
        );
        assert!(distance(lounge.torso(HIP), asleep.torso(HIP)) > 60.);
        assert!(distance(lounge.head((66., 51.)), asleep.head((66., 51.))) > 40.);
        assert!(asleep.support(SUPPORT_HAND).1 > 105.);
    }
}
