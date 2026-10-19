use std::time::{Duration, Instant};

use super::TabReorderMotion;

fn strip(widths: &[f32]) -> (TabReorderMotion, Instant) {
    let now = Instant::now();
    let mut motion = TabReorderMotion::default();
    let order: Vec<_> = (0..widths.len()).map(|i| i.to_string()).collect();
    motion.configure(order.clone(), order);
    let mut x = 0.;
    for (index, width) in widths.iter().enumerate() {
        motion.measure(&index.to_string(), x, *width, now);
        x += width;
    }
    (motion, now)
}

fn order(motion: &TabReorderMotion, source: &str) -> Vec<String> {
    motion.preview(source).unwrap().to_vec()
}

#[test]
fn unequal_tabs_reorder_repeatedly_in_both_directions_without_changing_business_order() {
    let (mut motion, now) = strip(&[120., 200., 150., 90.]);
    motion.begin("0", 30.);
    motion.update(600., now);
    assert_eq!(order(&motion, "0"), ["1", "2", "3", "0"]);
    motion.update(-20., now);
    assert_eq!(order(&motion, "0"), ["0", "1", "2", "3"]);
    motion.update(330., now);
    assert_eq!(order(&motion, "0"), ["1", "2", "0", "3"]);
    assert_eq!(motion.order, ["0", "1", "2", "3"]);
}

#[test]
fn hysteresis_keeps_the_target_stable_near_the_swap_boundary() {
    let (mut motion, now) = strip(&[120., 200.]);
    motion.begin("0", 60.);
    motion.update(169., now);
    assert_eq!(order(&motion, "0"), ["1", "0"]);
    motion.update(155., now);
    assert_eq!(order(&motion, "0"), ["1", "0"]);
    motion.update(151., now);
    assert_eq!(order(&motion, "0"), ["0", "1"]);
}

#[test]
fn retargeting_mid_animation_preserves_the_current_painted_position() {
    let (mut motion, now) = strip(&[120., 200.]);
    motion.begin("0", 60.);
    motion.update(250., now);
    let halfway = now + Duration::from_millis(75);
    let before = motion.offset("1", halfway).0;
    assert!(before < 0. && before > -120.);
    motion.update(0., halfway);
    assert_eq!(motion.offset("1", halfway).0, before);
    assert_eq!(
        motion.offset("1", halfway + Duration::from_millis(150)),
        (0., false)
    );
}

#[test]
fn cancel_or_leave_restores_slots_and_reentry_can_preview_again() {
    let (mut motion, now) = strip(&[120., 200.]);
    motion.begin("0", 60.);
    motion.update(250., now);
    motion.suspend(now);
    assert_eq!(order(&motion, "0"), ["0", "1"]);
    motion.update(250., now);
    assert_eq!(order(&motion, "0"), ["1", "0"]);
    motion.finish(now);
    assert!(motion.preview("0").is_none());
    assert_eq!(
        motion.offset("1", now + Duration::from_millis(150)),
        (0., false)
    );
}

#[test]
fn membership_change_invalidates_the_preview_and_commit_rebases_without_a_jump() {
    let (mut motion, now) = strip(&[120., 200.]);
    motion.begin("0", 60.);
    motion.update(250., now);
    let settled = now + Duration::from_millis(150);
    motion.finish(settled);
    motion.configure(vec!["1".into(), "0".into()], vec!["1".into(), "0".into()]);
    motion.measure("1", 0., 200., settled);
    assert_eq!(motion.offset("1", settled).0, 0.);
    motion.begin("0", 60.);
    motion.configure(vec!["1".into()], vec!["1".into()]);
    assert!(motion.preview("0").is_none());
}

#[test]
fn edge_scroll_continues_without_mouse_moves_and_stops_at_the_content_boundary() {
    let (mut motion, now) = strip(&[120., 200.]);
    motion.begin("0", 60.);
    motion.set_pointer(Some(199.));
    let first = motion.advance_scroll(0., 200., 0., 120., now).unwrap();
    let second = motion
        .advance_scroll(0., 200., first, 120., now + Duration::from_millis(16))
        .unwrap();
    assert!(second < first);
    assert!(
        motion
            .advance_scroll(0., 200., -120., 120., now + Duration::from_millis(32))
            .is_none()
    );
    motion.suspend(now + Duration::from_millis(32));
    assert!(
        motion
            .advance_scroll(0., 200., second, 120., now + Duration::from_millis(48))
            .is_none()
    );
}
