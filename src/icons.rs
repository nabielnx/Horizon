use svgtypes::{PathParser, PathSegment as S};
use windows::{
    Win32::Graphics::{Direct2D::Common::*, Direct2D::*},
    core::*,
};
use windows_numerics::Vector2;
pub unsafe fn outline(factory: &ID2D1Factory, name: &str) -> Result<ID2D1Geometry> {
    unsafe {
        let source = match name {
            "bell" | "bell-ringing" => include_str!("../assets/icons/bell-ringing.svg"),
            "check" | "check-fat" => include_str!("../assets/icons/check-fat.svg"),
            "chevron-left" | "caret-left" => include_str!("../assets/icons/caret-left.svg"),
            "chevron-right" | "caret-right" => include_str!("../assets/icons/caret-right.svg"),
            "chevron-down" | "caret-down" => include_str!("../assets/icons/caret-down.svg"),
            "chevron-up" | "caret-up" => include_str!("../assets/icons/caret-up.svg"),
            "external-link" | "arrow-square-out" => {
                include_str!("../assets/icons/arrow-square-out.svg")
            }
            "music-2" | "music-notes" => include_str!("../assets/icons/music-notes.svg"),
            "music-note" => include_str!("../assets/icons/music-note.svg"),
            "timer" => include_str!("../assets/icons/timer.svg"),
            "calendar-days" | "calendar-dots" => {
                include_str!("../assets/icons/calendar-dots.svg")
            }
            "droplet" | "drop" => include_str!("../assets/icons/drop.svg"),
            "list-checks" => include_str!("../assets/icons/list-checks.svg"),
            "play" => include_str!("../assets/icons/play.svg"),
            "pause" => include_str!("../assets/icons/pause.svg"),
            "skip-back" => include_str!("../assets/icons/skip-back.svg"),
            "skip-forward" => include_str!("../assets/icons/skip-forward.svg"),
            "rotate-ccw" | "repeat" => include_str!("../assets/icons/repeat.svg"),
            "sliders-horizontal" => include_str!("../assets/icons/sliders-horizontal.svg"),
            "headphones" => include_str!("../assets/icons/headphones.svg"),
            "x" => include_str!("../assets/icons/x.svg"),
            "plus" => include_str!("../assets/icons/plus.svg"),
            "minus" => include_str!("../assets/icons/minus.svg"),
            "info" => include_str!("../assets/icons/info.svg"),
            "sparkles" | "sparkle" => include_str!("../assets/icons/sparkle.svg"),
            "mic-2" | "microphone-stage" => include_str!("../assets/icons/microphone-stage.svg"),
            "heart" => include_str!("../assets/icons/heart.svg"),
            "heart-fill" => include_str!("../assets/icons/heart-fill.svg"),
            "clock" => include_str!("../assets/icons/clock.svg"),
            "pencil" | "pencil-simple" => include_str!("../assets/icons/pencil-simple.svg"),
            "folder" => include_str!("../assets/icons/folder.svg"),
            "files" => include_str!("../assets/icons/files.svg"),
            "copy" => include_str!("../assets/icons/copy.svg"),
            "file-text" => include_str!("../assets/icons/file-text.svg"),
            "spotify" | "spotify-logo" => include_str!("../assets/icons/spotify-logo.svg"),
            _ => {
                return Err(Error::new(
                    windows::Win32::Foundation::E_INVALIDARG,
                    "Unknown icon",
                ));
            }
        };
        let scale: f64 = if source.contains("256") {
            24.0 / 256.0
        } else {
            1.0
        };
        let mut shapes: Vec<Option<ID2D1Geometry>> = vec![];
        for tag in source.split('<') {
            let geom: Option<ID2D1Geometry> = if tag.starts_with("path ") {
                let path = factory.CreatePathGeometry()?;
                let sink = path.Open()?;
                sink.SetFillMode(D2D1_FILL_MODE_WINDING);
                let mut current = (0., 0.);
                let mut start = (0., 0.);
                let mut last_control = (0., 0.);
                let mut open = false;
                for parsed in PathParser::from(attr(tag, "d").unwrap_or("")) {
                    let s = parsed.map_err(|e| {
                        Error::new(windows::Win32::Foundation::E_INVALIDARG, e.to_string())
                    })?;
                    match s {
                        S::MoveTo { abs, x, y } => {
                            if open {
                                sink.EndFigure(D2D1_FIGURE_END_OPEN);
                            }
                            current = point(current, abs, x * scale, y * scale);
                            start = current;
                            sink.BeginFigure(v(current), D2D1_FIGURE_BEGIN_FILLED);
                            open = true;
                        }
                        S::LineTo { abs, x, y } => {
                            current = point(current, abs, x * scale, y * scale);
                            sink.AddLine(v(current));
                        }
                        S::HorizontalLineTo { abs, x } => {
                            current.0 = if abs {
                                x * scale
                            } else {
                                current.0 + x * scale
                            };
                            sink.AddLine(v(current));
                        }
                        S::VerticalLineTo { abs, y } => {
                            current.1 = if abs {
                                y * scale
                            } else {
                                current.1 + y * scale
                            };
                            sink.AddLine(v(current));
                        }
                        S::CurveTo {
                            abs,
                            x1,
                            y1,
                            x2,
                            y2,
                            x,
                            y,
                        } => {
                            let a = point(current, abs, x1 * scale, y1 * scale);
                            let b = point(current, abs, x2 * scale, y2 * scale);
                            current = point(current, abs, x * scale, y * scale);
                            sink.AddBezier(&D2D1_BEZIER_SEGMENT {
                                point1: v(a),
                                point2: v(b),
                                point3: v(current),
                            });
                            last_control = b;
                        }
                        S::SmoothCurveTo { abs, x2, y2, x, y } => {
                            let a = (
                                2. * current.0 - last_control.0,
                                2. * current.1 - last_control.1,
                            );
                            let b = point(current, abs, x2 * scale, y2 * scale);
                            current = point(current, abs, x * scale, y * scale);
                            sink.AddBezier(&D2D1_BEZIER_SEGMENT {
                                point1: v(a),
                                point2: v(b),
                                point3: v(current),
                            });
                            last_control = b;
                        }
                        S::Quadratic { abs, x1, y1, x, y } => {
                            let a = point(current, abs, x1 * scale, y1 * scale);
                            current = point(current, abs, x * scale, y * scale);
                            sink.AddQuadraticBezier(&D2D1_QUADRATIC_BEZIER_SEGMENT {
                                point1: v(a),
                                point2: v(current),
                            });
                            last_control = a;
                        }
                        S::SmoothQuadratic { abs, x, y } => {
                            let a = (
                                2. * current.0 - last_control.0,
                                2. * current.1 - last_control.1,
                            );
                            current = point(current, abs, x * scale, y * scale);
                            sink.AddQuadraticBezier(&D2D1_QUADRATIC_BEZIER_SEGMENT {
                                point1: v(a),
                                point2: v(current),
                            });
                            last_control = a;
                        }
                        S::EllipticalArc {
                            abs,
                            rx,
                            ry,
                            x_axis_rotation,
                            large_arc,
                            sweep,
                            x,
                            y,
                        } => {
                            current = point(current, abs, x * scale, y * scale);
                            sink.AddArc(&D2D1_ARC_SEGMENT {
                                point: v(current),
                                size: D2D_SIZE_F {
                                    width: (rx * scale) as f32,
                                    height: (ry * scale) as f32,
                                },
                                rotationAngle: x_axis_rotation as f32,
                                sweepDirection: if sweep {
                                    D2D1_SWEEP_DIRECTION_CLOCKWISE
                                } else {
                                    D2D1_SWEEP_DIRECTION_COUNTER_CLOCKWISE
                                },
                                arcSize: if large_arc {
                                    D2D1_ARC_SIZE_LARGE
                                } else {
                                    D2D1_ARC_SIZE_SMALL
                                },
                            });
                        }
                        S::ClosePath { .. } => {
                            sink.EndFigure(D2D1_FIGURE_END_CLOSED);
                            current = start;
                            open = false;
                        }
                    }
                }
                if open {
                    sink.EndFigure(D2D1_FIGURE_END_OPEN);
                }
                sink.Close()?;
                Some(path.cast()?)
            } else if tag.starts_with("polygon ") {
                let coordinates: Vec<f32> = attr(tag, "points")
                    .unwrap_or("")
                    .split(|c: char| c.is_whitespace() || c == ',')
                    .filter(|s| !s.is_empty())
                    .filter_map(|s| s.parse::<f32>().ok().map(|c| c * scale as f32))
                    .collect();
                if coordinates.len() < 6 || !coordinates.len().is_multiple_of(2) {
                    return Err(Error::new(
                        windows::Win32::Foundation::E_INVALIDARG,
                        "Invalid polygon",
                    ));
                }
                let path = factory.CreatePathGeometry()?;
                let sink = path.Open()?;
                sink.SetFillMode(D2D1_FILL_MODE_WINDING);
                sink.BeginFigure(
                    Vector2 {
                        X: coordinates[0],
                        Y: coordinates[1],
                    },
                    D2D1_FIGURE_BEGIN_FILLED,
                );
                for pair in coordinates[2..].as_chunks::<2>().0 {
                    sink.AddLine(Vector2 {
                        X: pair[0],
                        Y: pair[1],
                    });
                }
                sink.EndFigure(D2D1_FIGURE_END_CLOSED);
                sink.Close()?;
                Some(path.cast()?)
            } else if tag.starts_with("circle ") {
                let sc = scale as f32;
                Some(
                    factory
                        .CreateEllipseGeometry(&D2D1_ELLIPSE {
                            point: Vector2 {
                                X: n(tag, "cx") * sc,
                                Y: n(tag, "cy") * sc,
                            },
                            radiusX: n(tag, "r") * sc,
                            radiusY: n(tag, "r") * sc,
                        })?
                        .cast()?,
                )
            } else if tag.starts_with("line ") {
                let sc = scale as f32;
                let path = factory.CreatePathGeometry()?;
                let sink = path.Open()?;
                sink.BeginFigure(
                    Vector2 {
                        X: n(tag, "x1") * sc,
                        Y: n(tag, "y1") * sc,
                    },
                    D2D1_FIGURE_BEGIN_HOLLOW,
                );
                sink.AddLine(Vector2 {
                    X: n(tag, "x2") * sc,
                    Y: n(tag, "y2") * sc,
                });
                sink.EndFigure(D2D1_FIGURE_END_OPEN);
                sink.Close()?;
                Some(path.cast()?)
            } else if tag.starts_with("rect ") {
                let sc = scale as f32;
                Some(
                    factory
                        .CreateRoundedRectangleGeometry(&D2D1_ROUNDED_RECT {
                            rect: D2D_RECT_F {
                                left: n(tag, "x") * sc,
                                top: n(tag, "y") * sc,
                                right: (n(tag, "x") + n(tag, "width")) * sc,
                                bottom: (n(tag, "y") + n(tag, "height")) * sc,
                            },
                            radiusX: n(tag, "rx") * sc,
                            radiusY: n(tag, "rx") * sc,
                        })?
                        .cast()?,
                )
            } else {
                None
            };
            if let Some(g) = geom {
                shapes.push(Some(g));
            }
        }
        factory
            .CreateGeometryGroup(D2D1_FILL_MODE_WINDING, &shapes)?
            .cast()
    }
}
fn point(p: (f64, f64), abs: bool, x: f64, y: f64) -> (f64, f64) {
    if abs { (x, y) } else { (p.0 + x, p.1 + y) }
}
fn v(p: (f64, f64)) -> Vector2 {
    Vector2 {
        X: p.0 as f32,
        Y: p.1 as f32,
    }
}
fn attr<'a>(s: &'a str, key: &str) -> Option<&'a str> {
    let pattern = format!(" {key}=\"");
    let start = s.find(&pattern)? + pattern.len();
    s[start..].split('"').next()
}
fn n(s: &str, key: &str) -> f32 {
    attr(s, key).and_then(|n| n.parse().ok()).unwrap_or(0.)
}
