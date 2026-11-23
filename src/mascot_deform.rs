//! Master paths are parsed once. Only the current six deformed geometries are
//! retained; control points bend together rather than moving as rigid plates.
use super::*;
use svgtypes::{PathParser, PathSegment};

enum Segment {
    Move((f32, f32)),
    Line((f32, f32)),
    Cubic([(f32, f32); 3]),
    Quadratic([(f32, f32); 2]),
    Close,
}
pub(super) struct Path(Vec<Segment>);
impl Path {
    pub fn from_layer(id: &str) -> Result<Self> {
        Self::parse(&crate::mascot::layer(id))
    }
    pub fn sleeping(id: &str) -> Result<Self> {
        Self::parse(&crate::mascot::sleep_layer(id))
    }
    fn parse(source: &str) -> Result<Self> {
        let data = source
            .split(" d=\"")
            .nth(1)
            .and_then(|text| text.split('"').next())
            .ok_or_else(|| {
                Error::new(
                    windows::Win32::Foundation::E_INVALIDARG,
                    "Missing mascot path",
                )
            })?;
        let point = |x: f64, y: f64| (x as f32, y as f32);
        let mut segments = Vec::new();
        for parsed in PathParser::from(data) {
            let segment = match parsed {
                Ok(PathSegment::MoveTo { abs: true, x, y }) => Segment::Move(point(x, y)),
                Ok(PathSegment::LineTo { abs: true, x, y }) => Segment::Line(point(x, y)),
                Ok(PathSegment::CurveTo {
                    abs: true,
                    x1,
                    y1,
                    x2,
                    y2,
                    x,
                    y,
                }) => Segment::Cubic([point(x1, y1), point(x2, y2), point(x, y)]),
                Ok(PathSegment::Quadratic {
                    abs: true,
                    x1,
                    y1,
                    x,
                    y,
                }) => Segment::Quadratic([point(x1, y1), point(x, y)]),
                Ok(PathSegment::ClosePath { .. }) => Segment::Close,
                _ => {
                    return Err(Error::new(
                        windows::Win32::Foundation::E_INVALIDARG,
                        "Mascot master requires absolute M/L/C/Q/Z curves",
                    ));
                }
            };
            segments.push(segment);
        }
        Ok(Self(segments))
    }
    pub unsafe fn geometry(
        &self,
        factory: &ID2D1Factory,
        map: impl Fn((f32, f32)) -> (f32, f32),
    ) -> Result<ID2D1Geometry> {
        unsafe { self.morph(factory, self, map, 0.) }
    }
    pub unsafe fn morph(
        &self,
        factory: &ID2D1Factory,
        target: &Self,
        map: impl Fn((f32, f32)) -> (f32, f32),
        weight: f32,
    ) -> Result<ID2D1Geometry> {
        unsafe {
            let path = factory.CreatePathGeometry()?;
            let sink = path.Open()?;
            sink.SetFillMode(D2D1_FILL_MODE_WINDING);
            let vertex = |point, end| {
                let (x, y) = map(point);
                let (ex, ey) = end;
                let (x, y) = (x + (ex - x) * weight, y + (ey - y) * weight);
                Vector2 { X: x, Y: y }
            };
            if self.0.len() != target.0.len() {
                return Err(Error::new(
                    windows::Win32::Foundation::E_INVALIDARG,
                    "Sleeping contour must match master topology",
                ));
            }
            for (segment, end) in self.0.iter().zip(&target.0) {
                match (segment, end) {
                    (Segment::Move(point), Segment::Move(end)) => {
                        sink.BeginFigure(vertex(*point, *end), D2D1_FIGURE_BEGIN_FILLED)
                    }
                    (Segment::Line(point), Segment::Line(end)) => {
                        sink.AddLine(vertex(*point, *end))
                    }
                    (Segment::Cubic(points), Segment::Cubic(end)) => {
                        sink.AddBezier(&D2D1_BEZIER_SEGMENT {
                            point1: vertex(points[0], end[0]),
                            point2: vertex(points[1], end[1]),
                            point3: vertex(points[2], end[2]),
                        })
                    }
                    (Segment::Quadratic(points), Segment::Quadratic(end)) => sink
                        .AddQuadraticBezier(&D2D1_QUADRATIC_BEZIER_SEGMENT {
                            point1: vertex(points[0], end[0]),
                            point2: vertex(points[1], end[1]),
                        }),
                    (Segment::Close, Segment::Close) => sink.EndFigure(D2D1_FIGURE_END_CLOSED),
                    _ => {
                        return Err(Error::new(
                            windows::Win32::Foundation::E_INVALIDARG,
                            "Mismatched sleeping contour segments",
                        ));
                    }
                }
            }
            sink.Close()?;
            path.cast()
        }
    }
}
