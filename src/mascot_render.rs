//! Full-body dusk character: grounded contacts, articulated free arm and
//! retained vector materials. Sleeping contours are authored separately and
//! interpolate from the awake pose with matching control-point topology.
use super::*;
use crate::mascot_rig::{Point, Rig, SOLES};
#[path = "mascot_deform.rs"]
mod deform;

pub(super) struct Art {
    layers: [ID2D1Geometry; 12],
    paths: [deform::Path; 7],
    sleeping: [deform::Path; 7],
    body_shapes: Option<[ID2D1Geometry; 7]>,
    current: Option<([f32; 7], ID2D1Geometry, ID2D1Geometry)>,
    violet: ID2D1LinearGradientBrush,
    body: ID2D1LinearGradientBrush,
    face: ID2D1LinearGradientBrush,
    amber: ID2D1LinearGradientBrush,
    rim: ID2D1LinearGradientBrush,
    happy_eye: ID2D1Geometry,
    resting_eye: ID2D1Geometry,
}
fn v(p: Point) -> Vector2 {
    Vector2 { X: p.0, Y: p.1 }
}
fn affine(map: impl Fn(Point) -> Point) -> Matrix3x2 {
    let p = map((0., 0.));
    let x = map((1., 0.));
    let y = map((0., 1.));
    Matrix3x2 {
        M11: x.0 - p.0,
        M12: x.1 - p.1,
        M21: y.0 - p.0,
        M22: y.1 - p.1,
        M31: p.0,
        M32: p.1,
    }
}
impl Art {
    unsafe fn new(c: &Canvas) -> Result<Self> {
        unsafe {
            let gradient = |stops: &[(f32, u32)], from, to| -> Result<ID2D1LinearGradientBrush> {
                let stops: Vec<_> = stops
                    .iter()
                    .map(|&(position, rgb)| D2D1_GRADIENT_STOP {
                        position,
                        color: color(rgb),
                    })
                    .collect();
                let collection = c.target.CreateGradientStopCollection(
                    &stops,
                    D2D1_GAMMA_2_2,
                    D2D1_EXTEND_MODE_CLAMP,
                )?;
                c.target.CreateLinearGradientBrush(
                    &D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
                        startPoint: v(from),
                        endPoint: v(to),
                    },
                    None,
                    &collection,
                )
            };
            let layer = |id| crate::icons::from_svg(&factory()?, &crate::mascot::layer(id));
            Ok(Self {
                layers: [
                    layer("back-leg")?,
                    layer("back-foot")?,
                    layer("torso")?,
                    layer("support")?,
                    layer("front-leg")?,
                    layer("front-foot")?,
                    layer("badge")?,
                    layer("hand")?,
                    layer("head")?,
                    layer("face")?,
                    layer("shine")?,
                    layer("smile")?,
                ],
                paths: [
                    deform::Path::from_layer("torso")?,
                    deform::Path::from_layer("support")?,
                    deform::Path::from_layer("back-leg")?,
                    deform::Path::from_layer("back-foot")?,
                    deform::Path::from_layer("front-leg")?,
                    deform::Path::from_layer("front-foot")?,
                    deform::Path::from_layer("badge")?,
                ],
                sleeping: [
                    deform::Path::sleeping("torso")?,
                    deform::Path::sleeping("support")?,
                    deform::Path::sleeping("back-leg")?,
                    deform::Path::sleeping("back-foot")?,
                    deform::Path::sleeping("front-leg")?,
                    deform::Path::sleeping("front-foot")?,
                    deform::Path::sleeping("badge")?,
                ],
                body_shapes: None,
                current: None,
                violet: gradient(
                    &[(0., 0x9b83ad), (0.45, 0x695276), (1., 0x35293f)],
                    (45., 7.),
                    (115., 86.),
                )?,
                body: gradient(&[(0., 0x80628e), (1., 0x34283e)], (75., 75.), (120., 120.))?,
                face: gradient(&[(0., 0xf6e9e2), (1., 0xcdb8c7)], (60., 29.), (107., 76.))?,
                amber: gradient(&[(0., 0xffd38b), (1., 0xd79551)], (95., 81.), (112., 96.))?,
                rim: gradient(
                    &[(0., 0xa5e9d6f8), (0.55, 0x12ffffff), (1., 0x586d567a)],
                    (40., 6.),
                    (112., 120.),
                )?,
                happy_eye: crate::icons::from_svg(
                    &factory()?,
                    "<svg><path d=\"M-4.8 1 Q0 -5 4.8 1\"/></svg>",
                )?,
                resting_eye: crate::icons::from_svg(
                    &factory()?,
                    "<svg><path d=\"M-4.8 -1 Q0 3 4.8 -1\"/></svg>",
                )?,
            })
        }
    }
    unsafe fn update(&mut self, rig: &Rig) -> Result<()> {
        let p = rig.pose;
        let key = [
            p.breath,
            p.body_sway,
            p.wave,
            p.follow,
            p.raise,
            p.curl,
            p.sit,
        ];
        if self.current.as_ref().is_some_and(|(old, _, _)| *old == key) {
            return Ok(());
        }
        unsafe {
            let factory = factory()?;
            let awake = Rig::new(crate::mascot::Pose { curl: 0., ..p });
            let shape = |index: usize| -> Result<ID2D1Geometry> {
                let map = |point| match index {
                    0 | 6 => awake.torso(point),
                    1 => awake.support(point),
                    2 => awake.leg(point, false),
                    3 => awake.foot(point, false),
                    4 => awake.leg(point, true),
                    5 => awake.foot(point, true),
                    _ => point,
                };
                if p.curl == 0. {
                    self.paths[index].geometry(&factory, map)
                } else {
                    self.paths[index].morph(&factory, &self.sleeping[index], map, p.curl)
                }
            };
            let shapes = [
                shape(0)?,
                shape(1)?,
                shape(2)?,
                shape(3)?,
                shape(4)?,
                shape(5)?,
                shape(6)?,
            ];
            let arm = factory.CreatePathGeometry()?;
            let sink = arm.Open()?;
            let points = rig.arm_controls();
            sink.BeginFigure(v(points[0]), D2D1_FIGURE_BEGIN_HOLLOW);
            for points in [&points[1..4], &points[4..7]] {
                sink.AddBezier(&D2D1_BEZIER_SEGMENT {
                    point1: v(points[0]),
                    point2: v(points[1]),
                    point3: v(points[2]),
                });
            }
            sink.EndFigure(D2D1_FIGURE_END_OPEN);
            sink.Close()?;
            self.current = Some((key, shapes[1].clone(), arm.cast()?));
            self.body_shapes = Some(shapes);
        }
        Ok(())
    }
}
impl Canvas {
    pub unsafe fn mascot(
        &self,
        bounds: (f32, f32, f32, f32),
        pose: crate::mascot::Pose,
        _reflected: Option<u32>,
    ) {
        unsafe {
            let mut cache = self.mascot_art.borrow_mut();
            if cache.is_none() {
                let Ok(art) = Art::new(self) else {
                    return;
                };
                *cache = Some(art);
            }
            let art = cache.as_mut().unwrap();
            let rig = Rig::new(pose);
            if art.update(&rig).is_err() {
                return;
            }
            let (_, support, arm) = art.current.as_ref().unwrap();
            let mut previous = Matrix3x2::default();
            self.target.GetTransform(&mut previous);
            let scale = (bounds.2 / 200.).min(bounds.3 / 120.);
            let base = Matrix3x2::scale(scale, scale)
                * Matrix3x2::translation(
                    bounds.0 + (bounds.2 - 200. * scale) * 0.5,
                    bounds.1 + bounds.3 - SOLES[0].1 * scale,
                )
                * previous;
            let head = affine(|p| rig.head(p)) * base;
            let body = art.body_shapes.as_ref().unwrap();
            // Each full-body posture owns its contacts. Feet rotate around
            // their soles while the thigh curves follow hip and ankle joints.
            self.target.SetTransform(&base);
            self.target.FillGeometry(&body[2], &art.violet, None);
            self.target.FillGeometry(&body[3], &art.violet, None);
            self.target.FillGeometry(&body[0], &art.body, None);
            self.target
                .DrawGeometry(&body[0], &art.rim, 0.7, Some(&self.stroke));
            self.target.SetTransform(&base);
            self.target.FillGeometry(support, &art.violet, None);
            self.target
                .DrawGeometry(support, &art.rim, 0.55, Some(&self.stroke));
            self.target.FillGeometry(&body[4], &art.violet, None);
            self.target
                .DrawGeometry(&body[4], &art.rim, 0.55, Some(&self.stroke));
            self.target.FillGeometry(&body[5], &art.violet, None);
            self.target
                .DrawGeometry(&body[5], &art.rim, 0.55, Some(&self.stroke));
            self.target.FillGeometry(&body[6], &art.amber, None);
            self.target.SetTransform(&base);
            self.target
                .DrawGeometry(arm, &art.rim, 11., Some(&self.stroke));
            self.target
                .DrawGeometry(arm, &art.violet, 10., Some(&self.stroke));
            let hand = Matrix3x2::rotation_around(rig.hand_angle, v((114., 101.)))
                * Matrix3x2::translation(rig.wrist.0 - 114., rig.wrist.1 - 101.)
                * base;
            self.target.SetTransform(&hand);
            self.target.FillGeometry(&art.layers[7], &art.violet, None);
            self.target
                .DrawGeometry(&art.layers[7], &art.rim, 0.6, Some(&self.stroke));
            self.target.SetTransform(&head);
            self.target.FillGeometry(&art.layers[8], &art.violet, None);
            self.target
                .DrawGeometry(&art.layers[8], &art.rim, 0.7, Some(&self.stroke));
            self.target.FillGeometry(&art.layers[9], &art.face, None);
            self.solid_brush.SetColor(&D2D1_COLOR_F {
                a: 0.26,
                ..color(0xe2cfef)
            });
            self.target
                .FillGeometry(&art.layers[10], &self.solid_brush, None);
            for (x, y) in [(66., 51.), (96., 48.)] {
                let x = x + pose.look.0 * 1.3;
                let y = y + pose.look.1;
                let normal = (1. - pose.joy) * (1. - pose.rest);
                self.solid_brush.SetColor(&D2D1_COLOR_F {
                    a: normal,
                    ..color(0x292332)
                });
                self.target.FillEllipse(
                    &D2D1_ELLIPSE {
                        point: v((x, y)),
                        radiusX: 4.8,
                        radiusY: (6. * pose.eyes).max(0.65),
                    },
                    &self.solid_brush,
                );
                self.solid_brush.SetColor(&D2D1_COLOR_F {
                    a: normal * smooth_highlight(pose.eyes),
                    ..color(0xffffff)
                });
                self.target.FillEllipse(
                    &D2D1_ELLIPSE {
                        point: v((x + 1.2, y - 1.8 * pose.eyes)),
                        radiusX: 1.4,
                        radiusY: 1.4 * pose.eyes,
                    },
                    &self.solid_brush,
                );
                self.target
                    .SetTransform(&(Matrix3x2::translation(x, y) * head));
                for (geometry, alpha) in [
                    (&art.happy_eye, pose.joy * (1. - pose.rest)),
                    (&art.resting_eye, pose.rest),
                ] {
                    self.solid_brush.SetColor(&D2D1_COLOR_F {
                        a: alpha,
                        ..color(0x292332)
                    });
                    self.target
                        .DrawGeometry(geometry, &self.solid_brush, 1.6, Some(&self.stroke));
                }
                self.target.SetTransform(&head);
            }
            self.target.SetTransform(
                &(Matrix3x2::scale_around(1. + pose.joy * 0.3, 1. + pose.joy * 0.4, v((84., 61.)))
                    * head),
            );
            self.solid_brush.SetColor(&D2D1_COLOR_F {
                a: 1. - pose.yawn,
                ..color(0x292332)
            });
            self.target
                .DrawGeometry(&art.layers[11], &self.solid_brush, 1.8, Some(&self.stroke));
            self.target.SetTransform(&head);
            self.solid_brush.SetColor(&D2D1_COLOR_F {
                a: pose.yawn,
                ..color(0x292332)
            });
            self.target.FillEllipse(
                &D2D1_ELLIPSE {
                    point: v((84., 61.)),
                    radiusX: 1. + pose.yawn * 3.,
                    radiusY: 1. + pose.yawn * 5.,
                },
                &self.solid_brush,
            );
            self.target.SetTransform(&previous);
        }
    }
}
fn smooth_highlight(eyes: f32) -> f32 {
    ((eyes - 0.25) / 0.4).clamp(0., 1.)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lower_body_silhouettes_change_for_sitting_and_sleeping() {
        unsafe {
            let c = Canvas::image(240, 160).unwrap();
            let mut frames = Vec::new();
            for pose in [
                crate::mascot::Pose::default(),
                crate::mascot::Pose {
                    sit: 1.,
                    ..Default::default()
                },
                crate::mascot::Pose {
                    curl: 1.,
                    rest: 1.,
                    eyes: 0.05,
                    ..Default::default()
                },
            ] {
                c.begin_palette([0x101010; 3]);
                c.mascot((20., 20., 200., 120.), pose, None);
                c.end().unwrap();
                let mut pixels = vec![0; 240 * 160 * 4];
                c.bitmap
                    .as_ref()
                    .unwrap()
                    .CopyPixels(std::ptr::null(), 240 * 4, &mut pixels)
                    .unwrap();
                // Head and eyes are excluded: this must be a body-pose change.
                frames.push(pixels[100 * 240 * 4..145 * 240 * 4].to_vec());
                if pose.curl == 1. && std::env::var_os("HORIZON_MASCOT_PREVIEW").is_some() {
                    c.save_bmp(std::path::Path::new("artifacts/mascot-body-validation.bmp"))
                        .unwrap();
                }
            }
            for index in [1, 2] {
                let changed = frames[0]
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .zip(frames[index].as_chunks::<4>().0.iter())
                    .filter(|(a, b)| a != b)
                    .count();
                assert!(changed > 600, "distinct lower-body silhouette: {changed}");
            }
        }
    }
    #[test]
    fn full_body_materials_render_and_current_geometry_is_bounded() {
        unsafe {
            let c = Canvas::image(240, 160).unwrap();
            let transform = Matrix3x2::translation(1., 2.);
            let p = crate::mascot::sample(1., (0., 0.), None, false, true);
            for _ in 0..2 {
                c.begin_palette([0x101010; 3]);
                c.target.SetTransform(&transform);
                c.mascot((20., 20., 200., 120.), p, None);
                let mut restored = Matrix3x2::default();
                c.target.GetTransform(&mut restored);
                assert_eq!(restored, transform);
                c.end().unwrap();
            }
            let old = c
                .mascot_art
                .borrow()
                .as_ref()
                .unwrap()
                .current
                .as_ref()
                .unwrap()
                .2
                .as_raw();
            c.begin_palette([0x101010; 3]);
            c.mascot((20., 20., 200., 120.), p, None);
            c.end().unwrap();
            assert_eq!(
                old,
                c.mascot_art
                    .borrow()
                    .as_ref()
                    .unwrap()
                    .current
                    .as_ref()
                    .unwrap()
                    .2
                    .as_raw()
            );
            let mut pixels = vec![0; 240 * 160 * 4];
            c.bitmap
                .as_ref()
                .unwrap()
                .CopyPixels(std::ptr::null(), 240 * 4, &mut pixels)
                .unwrap();
            let face = pixels
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|p| p[2] > 150 && p[1] > 140 && p[0] > 140)
                .count();
            let violet = pixels
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|p| p[0] > 45 && p[0] > p[1] + 8)
                .count();
            assert!(
                face > 1500 && violet > 3000,
                "full-body face and violet silhouette: {face}, {violet}"
            );
        }
    }
    #[test]
    fn face_moods_are_visible_without_body_motion() {
        unsafe {
            let c = Canvas::image(240, 160).unwrap();
            let mut counts = Vec::new();
            for seconds in [5., 18., 31., 34.5, 40.] {
                let expression =
                    crate::mascot::sample(seconds, (0., 0.), None, seconds != 40., true);
                let p = crate::mascot::Pose {
                    eyes: expression.eyes,
                    joy: expression.joy,
                    yawn: expression.yawn,
                    rest: expression.rest,
                    ..Default::default()
                };
                c.begin_palette([0x101010; 3]);
                c.mascot((20., 20., 200., 120.), p, None);
                c.end().unwrap();
                let mut pixels = vec![0; 240 * 160 * 4];
                c.bitmap
                    .as_ref()
                    .unwrap()
                    .CopyPixels(std::ptr::null(), 240 * 4, &mut pixels)
                    .unwrap();
                let dark = |left: usize, top: usize, width: usize, height: usize| {
                    (top..top + height)
                        .flat_map(|y| (left..left + width).map(move |x| (y * 240 + x) * 4))
                        .filter(|i| pixels[*i..*i + 3].iter().all(|v| *v < 65))
                        .count()
                };
                counts.push((dark(79, 63, 15, 17), dark(95, 75, 20, 16)));
            }
            assert!(counts[1].0 < counts[0].0 / 2);
            assert!(counts[2].1 > counts[0].1 * 2);
            assert!(counts[3].0 < counts[0].0 / 2);
            assert_eq!(counts[0], counts[4]);
        }
    }
}
