//! How long ago a key event happened, from the timestamp the event carries.
//!
//! Its own file for the reason `budget.rs` has one: this is arithmetic over three integers,
//! with nothing macOS about it, so `backend/mod.rs` borrows it by `#[path]` under `cfg(test)`
//! and it runs on the machine it is written on. The tap only supplies the numbers.
//!
//! **The units are the hard part, and they are not settled.** `CGEventGetTimestamp` is
//! documented as nanoseconds since startup. On an Intel Mac that is also what
//! `mach_absolute_time` counts, so the question never came up; on Apple silicon the mach clock
//! ticks at 24 MHz, and there are reports of event timestamps arriving in those ticks rather
//! than in nanoseconds. Nobody here can look. So both readings are tried, and the timestamp
//! itself says which one it is: nanoseconds since startup are about forty times the tick count
//! on Apple silicon, so a timestamp above the current tick count cannot be ticks. On Intel the
//! two clocks are the same number, and both readings agree.

/// How old an event with timestamp `ts` is, in nanoseconds, given the mach tick count and the
/// uptime in nanoseconds read at the same moment. `None` when the timestamp says nothing: zero
/// (a synthesised event that was never stamped), or later than now in both readings.
pub(crate) fn age_ns(ts: u64, now_ticks: u64, now_ns: u64) -> Option<u64> {
    if ts == 0 || now_ticks == 0 {
        return None;
    }
    if ts > now_ticks {
        // Not ticks, so nanoseconds: the same clock `now_ns` reads.
        return now_ns.checked_sub(ts);
    }
    // Ticks, converted with the ratio the two clocks show right now. Measured rather than
    // asked of `mach_timebase_info`, whose binding `libc` deprecates, and exact enough: both
    // counters started at boot, so the ratio is the timebase to many significant figures.
    let ticks = now_ticks - ts;
    let per_tick = now_ns as f64 / now_ticks as f64;
    Some((ticks as f64 * per_tick) as u64)
}

#[cfg(test)]
mod tests {
    use super::age_ns;

    /// Apple silicon: 24 MHz ticks, so 125/3 ns per tick. An hour after boot.
    const NS: u64 = 3_600_000_000_000;
    const TICKS: u64 = NS * 3 / 125;

    #[test]
    fn a_nanosecond_timestamp_on_apple_silicon() {
        let ts = NS - 300_000_000; // 300 ms ago, in nanoseconds
        assert_eq!(age_ns(ts, TICKS, NS), Some(300_000_000));
    }

    #[test]
    fn a_tick_timestamp_on_apple_silicon() {
        let ts = TICKS - 300_000_000 * 3 / 125; // 300 ms ago, in ticks
        let age = age_ns(ts, TICKS, NS).unwrap();
        assert!((age as i64 - 300_000_000).abs() < 1_000, "{age}");
    }

    #[test]
    fn intel_where_both_clocks_are_one() {
        let ts = NS - 5_000_000;
        assert_eq!(age_ns(ts, NS, NS), Some(5_000_000));
    }

    #[test]
    fn an_event_stamped_this_instant_is_no_age_at_all() {
        assert_eq!(age_ns(NS, TICKS, NS), Some(0));
        assert_eq!(age_ns(TICKS, TICKS, NS), Some(0));
    }

    #[test]
    fn nothing_to_say_about_an_unstamped_event() {
        assert_eq!(age_ns(0, TICKS, NS), None);
    }

    #[test]
    fn a_timestamp_from_the_future_is_not_an_age() {
        // Above the tick count, so read as nanoseconds, and later than now in those too.
        assert_eq!(age_ns(NS + 1_000, TICKS, NS), None);
    }
}
