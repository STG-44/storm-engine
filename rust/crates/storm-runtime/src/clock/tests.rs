use super::*;
use serde_json::{Value, json};

#[test]
fn native_timer_minimum_fps_order_and_fixed_state_match_complete_original_header_snapshots() {
    let profiles: Value =
        serde_json::from_str(include_str!("../../reference/timer/cases.json")).unwrap();
    let mut count = 0;
    for profile in profiles.as_array().unwrap() {
        let mut timer = Timer::default();
        for frame in profile["frames"].as_array().unwrap() {
            let returned = if frame["initial"].as_bool().unwrap() {
                timer.delta_ms()
            } else {
                if let Some(delta) = frame["set_delta"].as_i64() {
                    timer.set_delta(delta as i32);
                }
                timer
                    .advance(Duration::from_nanos(
                        frame["elapsed_nanos"].as_u64().unwrap(),
                    ))
                    .unwrap()
            };
            let s = timer.snapshot();
            assert_eq!(
                returned,
                u32::try_from(frame["returned"].as_u64().unwrap()).unwrap()
            );
            assert_eq!(
                json!({"ring":u8::from(s.ring),"fixed":u8::from(s.fixed),"fixed_value":s.fixed_value,
                "delta_ms":s.delta_ms,"high_precision_bits":s.high_precision_bits,"real_delta_ms":s.real_delta_ms,
                "fps":s.fps,"fps_count":s.fps_count,"fps_time_ms":s.fps_time_ms}),
                frame["state"],
                "profile {} frame {}",
                profile["profile"],
                count
            );
            count += 1;
        }
    }
    assert_eq!(count, 1040);
}

#[test]
fn unrepresentable_elapsed_intervals_preserve_clock_state_and_disabling_fixed_time_keeps_its_value()
{
    let mut timer = Timer::default();
    timer.set_delta(16);
    assert_eq!(timer.advance(Duration::ZERO).unwrap(), 16);
    assert_eq!(timer.snapshot().real_delta_ms, 1);
    assert_eq!(timer.snapshot().fps_time_ms, 0);
    timer.set_delta(0);
    assert_eq!(timer.snapshot().fixed_value, 16);
    assert_eq!(timer.advance(Duration::from_millis(5000)).unwrap(), 100);
    let before = timer.snapshot();
    assert!(timer.advance(Duration::MAX).is_err());
    assert_eq!(timer.snapshot(), before);
}
