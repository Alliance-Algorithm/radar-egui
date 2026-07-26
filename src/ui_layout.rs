use egui::{Pos2, Rect, Vec2};

/// Fit a fixed aspect-ratio rectangle inside `outer`, centered (letterbox).
#[must_use]
pub fn letterbox_rect(outer: Rect, aspect: f32) -> Rect {
    let aspect = aspect.max(0.01);
    let ow = outer.width().max(1.0);
    let oh = outer.height().max(1.0);
    let (w, h) = if ow / oh > aspect {
        let h = oh;
        let w = h * aspect;
        (w, h)
    } else {
        let w = ow;
        let h = w / aspect;
        (w, h)
    };
    Rect::from_center_size(outer.center(), Vec2::new(w, h))
}

/// Minimap asset aspect (1659×916).
pub const MINIMAP_ASPECT: f32 = 1659.0 / 916.0;

/// Laser video aspect (16:9).
pub const VIDEO_ASPECT: f32 = 16.0 / 9.0;

/// Stage frame padding around letterboxed content.
pub const STAGE_PAD: f32 = 8.0;

/// Inset `outer` by `pad` on all sides, clamped to non-empty.
#[must_use]
pub fn inset_rect(outer: Rect, pad: f32) -> Rect {
    let pad = pad.max(0.0);
    let min = Pos2::new(outer.left() + pad, outer.top() + pad);
    let max = Pos2::new(
        (outer.right() - pad).max(min.x + 1.0),
        (outer.bottom() - pad).max(min.y + 1.0),
    );
    Rect::from_min_max(min, max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letterbox_prefers_width_when_outer_is_wide() {
        // Given a wide outer rect
        let outer = Rect::from_min_size(Pos2::ZERO, Vec2::new(1000.0, 400.0));
        // When fitting 16:9
        let inner = letterbox_rect(outer, 16.0 / 9.0);
        // Then height fills outer; width is shorter
        assert!((inner.height() - 400.0).abs() < 0.5);
        assert!(inner.width() < outer.width());
        assert!((inner.width() / inner.height() - 16.0 / 9.0).abs() < 0.01);
        assert!((inner.center() - outer.center()).length() < 0.5);
    }

    #[test]
    fn letterbox_prefers_height_when_outer_is_tall() {
        let outer = Rect::from_min_size(Pos2::ZERO, Vec2::new(400.0, 1000.0));
        let inner = letterbox_rect(outer, MINIMAP_ASPECT);
        assert!((inner.width() - 400.0).abs() < 0.5);
        assert!(inner.height() < outer.height());
        assert!((inner.width() / inner.height() - MINIMAP_ASPECT).abs() < 0.01);
    }

    #[test]
    fn inset_shrinks_all_sides() {
        let outer = Rect::from_min_size(Pos2::new(10.0, 20.0), Vec2::new(200.0, 100.0));
        let inner = inset_rect(outer, 8.0);
        assert!((inner.left() - 18.0).abs() < 0.01);
        assert!((inner.top() - 28.0).abs() < 0.01);
        assert!((inner.width() - 184.0).abs() < 0.01);
        assert!((inner.height() - 84.0).abs() < 0.01);
    }
}
