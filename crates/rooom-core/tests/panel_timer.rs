use rooom_core::{PanelTimer, Surface};
use std::time::{Duration, Instant};

#[test]
fn pointer_reveals_the_strip_and_an_explicit_close_waits_for_the_pointer_to_leave() {
    let now = Instant::now();
    let mut timer = PanelTimer::default();
    assert_eq!(
        timer.observe(Surface::Strip, true, false, now),
        Some(Surface::Panel)
    );
    timer.surface_changed(Surface::Strip);
    assert_eq!(timer.observe(Surface::Strip, true, false, now), None);
    assert_eq!(timer.observe(Surface::Strip, false, false, now), None);
    assert_eq!(
        timer.observe(Surface::Strip, true, false, now),
        Some(Surface::Panel)
    );
}

#[test]
fn retraction_requires_three_uninterrupted_seconds_outside_an_unheld_panel() {
    let now = Instant::now();
    let mut timer = PanelTimer::default();
    timer.surface_changed(Surface::Panel);
    assert_eq!(timer.observe(Surface::Panel, false, false, now), None);
    assert_eq!(
        timer.observe(
            Surface::Panel,
            false,
            false,
            now + Duration::from_millis(2999)
        ),
        None
    );
    assert_eq!(
        timer.observe(Surface::Panel, true, false, now + Duration::from_secs(3)),
        None
    );
    assert_eq!(
        timer.observe(Surface::Panel, false, true, now + Duration::from_secs(8)),
        None
    );
    assert_eq!(
        timer.observe(Surface::Panel, false, false, now + Duration::from_secs(9)),
        None
    );
    assert_eq!(
        timer.observe(Surface::Panel, false, false, now + Duration::from_secs(12)),
        Some(Surface::Strip)
    );
}
