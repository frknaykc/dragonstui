use std::time::{Duration, Instant};

use dragons_tui::Animation;

#[test]
fn animation_starts_at_its_first_frame_and_only_changes_after_its_frame_duration() {
    let start = Instant::now();
    let mut animation = Animation::new(["A", "B", "C"])
        .frame_duration(Duration::from_millis(100))
        .looped(true);

    assert_eq!(animation.current(), Some(&"A"));
    assert!(!animation.update(start));
    assert!(!animation.update(start + Duration::from_millis(99)));
    assert_eq!(animation.current(), Some(&"A"));
    assert!(animation.update(start + Duration::from_millis(100)));
    assert_eq!(animation.current(), Some(&"B"));
}

#[test]
fn looping_animation_catches_up_without_losing_fractional_elapsed_time() {
    let start = Instant::now();
    let mut animation = Animation::new(["A", "B", "C"])
        .frame_duration(Duration::from_millis(100))
        .looped(true);

    animation.update(start);
    assert!(!animation.update(start + Duration::from_millis(350)));
    assert_eq!(animation.current(), Some(&"A"));
    assert!(animation.update(start + Duration::from_millis(400)));
    assert_eq!(animation.current(), Some(&"B"));
}

#[test]
fn non_looping_animation_stops_on_its_final_frame_after_catch_up() {
    let start = Instant::now();
    let mut animation = Animation::new(["A", "B", "C"])
        .frame_duration(Duration::from_millis(100))
        .looped(false);

    animation.update(start);
    assert!(animation.update(start + Duration::from_millis(350)));
    assert_eq!(animation.current(), Some(&"C"));
    assert!(animation.is_completed());
    assert!(!animation.update(start + Duration::from_secs(60)));
}

#[test]
fn empty_zero_duration_and_one_frame_animations_are_safe_noops() {
    let start = Instant::now();
    let mut empty = Animation::<&str>::new([]);
    let mut paused = Animation::new(["A", "B"]).frame_duration(Duration::ZERO);
    let mut single = Animation::new(["A"]).frame_duration(Duration::from_millis(1));

    assert_eq!(empty.current(), None);
    assert!(empty.is_completed());
    assert!(!empty.update(start + Duration::from_secs(1)));
    assert!(!paused.update(start));
    assert!(!paused.update(start + Duration::from_secs(1)));
    assert_eq!(paused.current(), Some(&"A"));
    single.update(start);
    assert!(!single.update(start + Duration::from_secs(1)));
    assert_eq!(single.current(), Some(&"A"));
}

#[test]
fn one_frame_non_looping_animation_is_completed_immediately() {
    let animation = Animation::new(["A"]).looped(false);

    assert_eq!(animation.current(), Some(&"A"));
    assert!(animation.is_completed());
}

#[test]
fn independent_frame_durations_progress_at_different_rates() {
    let start = Instant::now();
    let mut fast = Animation::new(["0", "1", "2", "3"]).frame_duration(Duration::from_millis(100));
    let mut slow = Animation::new(["0", "1", "2", "3"]).frame_duration(Duration::from_millis(200));

    fast.update(start);
    slow.update(start);
    assert!(fast.update(start + Duration::from_millis(300)));
    assert!(slow.update(start + Duration::from_millis(300)));
    assert_eq!(fast.current(), Some(&"3"));
    assert_eq!(slow.current(), Some(&"1"));
}
