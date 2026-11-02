/// Centre the active lyric, backfilling spare rows at the start/end of a song.
pub fn lyric_window(total: usize, current: usize, capacity: usize) -> std::ops::Range<usize> {
    let count = capacity.min(total);
    if count == 0 {
        return 0..0;
    }
    let current = current.min(total - 1);
    let start = current.saturating_sub((count - 1) / 2).min(total - count);
    start..start + count
}

#[derive(Debug)]
pub struct LyricRow {
    pub index: usize,
    pub y: f32,
    pub height: f32,
}
/// Use measured paragraph heights, centring the active line when space permits.
/// At song boundaries, spare space is filled by context from the other side.
pub fn lyric_rows(
    heights: &[f32],
    current: usize,
    top: f32,
    height: f32,
    gap: f32,
) -> Vec<LyricRow> {
    if heights.is_empty() || height <= 0. {
        return vec![];
    }
    let current = current.min(heights.len() - 1);
    let h = |i: usize| heights[i].max(1.).min(height);
    let (mut start, mut end) = (current, current + 1);
    let (mut before, mut after) = (0., 0.);
    let mut used = h(current);
    loop {
        let left = (start > 0)
            .then(|| h(start - 1) + gap)
            .filter(|n| used + n <= height);
        let right = (end < heights.len())
            .then(|| h(end) + gap)
            .filter(|n| used + n <= height);
        match (left, right) {
            (Some(n), r) if r.is_none() || before < after => {
                start -= 1;
                used += n;
                before += n;
            }
            (_, Some(n)) => {
                end += 1;
                used += n;
                after += n;
            }
            _ => break,
        }
    }
    let centred = top + (height - h(current)) / 2. - before;
    let mut y = centred.clamp(top, top + height - used);
    (start..end)
        .map(|index| {
            let row = LyricRow {
                index,
                y,
                height: h(index),
            };
            y += h(index) + gap;
            row
        })
        .collect()
}

// The same contour drives Win32 clipping and render verification.
pub fn contour(w: f32, h: f32, p: f32, radius: f32) -> Vec<(f32, f32)> {
    let p = p.clamp(0., 1.);
    let shoulder = 14. * (1. - p);
    let shoulder_h = 14. * (1. - p);
    let bottom = (radius + 2.).min(h * 0.48).max(4.);
    let k = 0.552_284_8;

    // Top edge: flat against the screen bezel from 0 to w.
    let mut points = vec![(0., 0.), (w, 0.)];

    if shoulder > 0.05 {
        // Right flared shoulder (concave curve from screen edge into notch body):
        curve(
            &mut points,
            (w - shoulder * k, 0.),
            (w - shoulder, shoulder_h * (1. - k)),
            (w - shoulder, shoulder_h),
        );
    }

    // Right body side
    points.push((w - shoulder, h - bottom));

    // Bottom-right rounded corner (convex curve from body into flat bottom):
    curve(
        &mut points,
        (w - shoulder, h - bottom + bottom * k),
        (w - shoulder - bottom + bottom * k, h),
        (w - shoulder - bottom, h),
    );

    // Bottom edge
    points.push((shoulder + bottom, h));

    // Bottom-left rounded corner (convex curve):
    curve(
        &mut points,
        (shoulder + bottom - bottom * k, h),
        (shoulder, h - bottom + bottom * k),
        (shoulder, h - bottom),
    );

    // Left body side
    points.push((shoulder, shoulder_h));

    if shoulder > 0.05 {
        // Left flared shoulder (concave curve back out to top-left screen edge):
        curve(
            &mut points,
            (shoulder, shoulder_h * (1. - k)),
            (shoulder * k, 0.),
            (0., 0.),
        );
    }

    points
}

fn curve(points: &mut Vec<(f32, f32)>, b: (f32, f32), c: (f32, f32), d: (f32, f32)) {
    let a = points.last().copied().unwrap_or_default();
    // 24 subdivisions per quadrant gives smooth continuous curves without redundant duplicate points
    for i in 1..=24 {
        let t = i as f32 / 24.;
        let u = 1. - t;
        points.push((
            u * u * u * a.0 + 3. * u * u * t * b.0 + 3. * u * t * t * c.0 + t * t * t * d.0,
            u * u * u * a.1 + 3. * u * u * t * b.1 + 3. * u * t * t * c.1 + t * t * t * d.1,
        ));
    }
}

/// Keep the sung glyph inside the reading area, without an independent clock.
pub fn karaoke_scroll(caret: f32, text_width: f32, viewport: f32) -> f32 {
    (caret - viewport * 0.55).clamp(0., (text_width - viewport).max(0.))
}

#[cfg(test)]
mod tests {
    #[test]
    fn karaoke_scroll_follows_the_sung_glyph_and_resets_on_seek() {
        assert_eq!(super::karaoke_scroll(0., 600., 175.), 0.);
        assert!(super::karaoke_scroll(300., 600., 175.) > 0.);
        assert_eq!(super::karaoke_scroll(600., 600., 175.), 425.);
        assert_eq!(super::karaoke_scroll(0., 600., 175.), 0.);
        assert_eq!(super::karaoke_scroll(100., 100., 175.), 0.);
    }
    use super::*;
    #[test]
    fn lyric_paragraphs_fit_without_overlap_and_backfill_song_edges() {
        let rows = lyric_rows(&[20.; 12], 0, 31., 313., 10.);
        assert_eq!(rows.len(), 10);
        assert_eq!(rows[0].index, 0);
        for current in 0..8 {
            let heights = [18., 36., 18., 90., 54., 18., 36., 18.];
            let rows = lyric_rows(&heights, current, 31., 313., 10.);
            assert!(rows.iter().any(|r| r.index == current));
            for row in &rows {
                assert!(row.y >= 31. && row.y + row.height <= 344.001);
            }
            for pair in rows.windows(2) {
                assert!(pair[0].y + pair[0].height + 9.99 <= pair[1].y);
            }
        }
        let rows = lyric_rows(&[20., 800., 20.], 1, 31., 313., 10.);
        assert_eq!(rows.len(), 1);
        assert_eq!((rows[0].y, rows[0].height), (31., 313.));
    }

    #[test]
    fn idle_edge_is_flat_and_shoulders_flare() {
        let p = contour(270., 36., 0., 24.);
        assert_eq!(p[0], (0., 0.));
        assert_eq!(p[1], (270., 0.));
        assert!(
            p.iter()
                .all(|(x, y)| *x >= -0.01 && *x <= 270.01 && *y >= -0.01 && *y <= 36.01)
        );
    }

    #[test]
    fn expanded_corners_and_transition_stay_bounded() {
        let expanded = contour(560., 296., 1., 24.);
        assert_eq!(expanded[0], (0., 0.));
        assert_eq!(expanded[1], (560., 0.));
        let min_x = expanded
            .iter()
            .map(|(x, _)| *x)
            .fold(f32::INFINITY, f32::min);
        let max_x = expanded
            .iter()
            .map(|(x, _)| *x)
            .fold(f32::NEG_INFINITY, f32::max);
        assert!(
            (min_x - 0.).abs() < 0.01,
            "Expanded notch must start at x=0 for sidebar"
        );
        assert!(
            (max_x - 560.).abs() < 0.01,
            "Expanded notch must extend to full width"
        );

        for i in 0..=10 {
            let t = i as f32 / 10.;
            let w = 270. + 290. * t;
            let h = 36. + 260. * t;
            let p = contour(w, h, t, 24.);
            assert_eq!(p[0], (0., 0.));
            assert_eq!(p[1], (w, 0.));
            assert!(
                p.iter()
                    .all(|(x, y)| *x >= -0.01 && *x <= w + 0.01 && *y >= -0.01 && *y <= h + 0.01)
            );
        }
    }
}
