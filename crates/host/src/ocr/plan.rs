//! How the regions of one read are photographed: one capture of their bounding box, cut into
//! pieces, or one capture per region.
//!
//! The standard capture on Windows costs a fixed compositor frame (~17 ms measured) whatever
//! its size, so two read-outs side by side are cheaper as one box than as two captures — the
//! measurement `recognizeMany` was built on. But the box of two regions in opposite corners
//! of a 4K screen is the whole screen, and copying 33 MB to read two words is the expensive
//! way round. So the box is used only when it is not wasteful: at most `BBOX_WASTE` times the
//! regions' own area (with a floor, so read-outs a few pixels apart always share one), and
//! never above `BBOX_MAX`.
//!
//! Every region of a call is still recognised on its own, whichever way it was photographed.
//! Desktop duplication does not come here: there one request carries every region as a piece
//! of one frame, and its cost grows with the area, so a box would only add to it.
//!
//! Std only; borrowed by `crates/macos-check`.

use super::policy::{BBOX_FLOOR, BBOX_MAX, BBOX_WASTE};
use super::types::Rect;

/// What the capture stage does for one call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Plan {
    /// One capture of this rectangle; every region is cut out of it.
    BoundingBox(Rect),
    /// One capture per region.
    Each,
}

/// The plan for `regions`. Empty regions take no part: they are answered as empty on their
/// own and neither widen the box nor count toward it.
pub fn capture_plan(regions: &[Rect]) -> Plan {
    let live: Vec<&Rect> = regions.iter().filter(|r| !r.is_empty()).collect();
    if live.len() < 2 {
        return Plan::Each;
    }
    let bbox = live.iter().fold(Rect::default(), |acc, r| acc.union(r));
    let own: i64 = live.iter().map(|r| r.area()).sum();
    let area = bbox.area();
    if area <= BBOX_MAX && area <= (own.saturating_mul(BBOX_WASTE)).max(BBOX_FLOOR) {
        Plan::BoundingBox(bbox)
    } else {
        Plan::Each
    }
}

/// Where `region` sits inside a capture of `bbox`, when it lies wholly inside it.
pub fn offset_in(region: &Rect, bbox: &Rect) -> Option<(i32, i32)> {
    let inside = region.x >= bbox.x
        && region.y >= bbox.y
        && region.right() <= bbox.right()
        && region.bottom() <= bbox.bottom();
    inside.then(|| (region.x - bbox.x, region.y - bbox.y))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_outs_side_by_side_share_one_capture() {
        let a = Rect::new(220, 61, 67, 13);
        let b = Rect::new(300, 61, 67, 13);
        assert_eq!(capture_plan(&[a, b]), Plan::BoundingBox(Rect::new(220, 61, 147, 13)));
        assert_eq!(offset_in(&b, &Rect::new(220, 61, 147, 13)), Some((80, 0)));
    }

    #[test]
    fn regions_far_apart_are_captured_one_by_one() {
        let a = Rect::new(0, 0, 300, 40);
        let b = Rect::new(3500, 2100, 300, 40);
        assert_eq!(capture_plan(&[a, b]), Plan::Each);
    }

    #[test]
    fn a_single_region_and_empty_ones_are_captured_by_themselves() {
        let a = Rect::new(10, 10, 100, 20);
        assert_eq!(capture_plan(&[a]), Plan::Each);
        assert_eq!(capture_plan(&[a, Rect::new(5000, 5000, 0, 10)]), Plan::Each);
        assert_eq!(capture_plan(&[]), Plan::Each);
        // An empty region neither widens the box nor spoils it for the others.
        let b = Rect::new(120, 10, 100, 20);
        assert_eq!(
            capture_plan(&[a, Rect::new(9000, 9000, 0, 0), b]),
            Plan::BoundingBox(Rect::new(10, 10, 210, 20))
        );
    }

    #[test]
    fn the_box_is_capped_in_pixels_even_when_it_is_not_wasteful() {
        // Two halves of a 4K screen: no waste at all, and still far over the cap.
        let a = Rect::new(0, 0, 1920, 2160);
        let b = Rect::new(1920, 0, 1920, 2160);
        assert_eq!(capture_plan(&[a, b]), Plan::Each);
        // Two halves of a 1080p screen: under the cap, no waste.
        let a = Rect::new(0, 0, 960, 1080);
        let b = Rect::new(960, 0, 960, 1080);
        assert_eq!(capture_plan(&[a, b]), Plan::BoundingBox(Rect::new(0, 0, 1920, 1080)));
    }

    /// Regions a read accepts (200 pixels in all) whose box does not fit in an `i32`: one each.
    #[test]
    fn regions_billions_of_pixels_apart_are_captured_one_by_one() {
        let a = Rect::new(-2_000_000_000, 0, 10, 10);
        let b = Rect::new(2_000_000_000, 0, 10, 10);
        assert_eq!(capture_plan(&[a, b]), Plan::Each);
    }

    #[test]
    fn tiny_regions_a_little_apart_share_one_under_the_floor() {
        // 4 x (10x10) = 400 px of regions, box 200x200 = 40 000 px: wasteful by the ratio,
        // but under the floor, and one capture is still cheaper than two.
        let a = Rect::new(0, 0, 10, 10);
        let b = Rect::new(190, 190, 10, 10);
        assert_eq!(capture_plan(&[a, b]), Plan::BoundingBox(Rect::new(0, 0, 200, 200)));
        assert_eq!(offset_in(&Rect::new(-1, 0, 5, 5), &Rect::new(0, 0, 200, 200)), None);
    }
}
