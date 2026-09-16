//! Rendering a moment, for the one place that shows one.
//!
//! sefy stores times as Unix seconds and, until now, never printed one. A date
//! crate for a single line of `sefy status` would be a dependency carried into
//! every build and every audit of what this program links against — a poor
//! trade for arithmetic that is a few lines and has not changed since 1582.
//!
//! Everything here is UTC. A local time would need the platform's zone
//! database, which is the dependency this avoids, and would also make the
//! output depend on where the machine thinks it is — unhelpful for a file that
//! travels between machines.

/// A moment, as a date and as a distance from now.
///
/// Both, because either alone is the wrong one half the time: "3 minutes ago"
/// says nothing useful about last spring, and "2026-03-04" makes the reader do
/// the subtraction for something that happened this morning.
pub fn moment(at: i64, now: i64) -> String {
    format!("{} ({})", date_time(at), ago(now - at))
}

/// `YYYY-MM-DD HH:MM UTC` for a Unix timestamp.
pub fn date_time(at: i64) -> String {
    // Floor division, so times before 1970 land on the right day rather than
    // one too late. They should not occur, but a wrong date is worse than an
    // impossible one being handled.
    let days = at.div_euclid(86_400);
    let seconds = at.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02} UTC",
        seconds / 3600,
        (seconds % 3600) / 60
    )
}

/// A rough distance in the past, in the largest unit that still reads.
fn ago(seconds: i64) -> String {
    // A stamp from the future is not an error worth refusing: clocks disagree,
    // and a vault synced by a machine a minute ahead should not make `status`
    // complain about it.
    if seconds < 0 {
        return "in the future".to_owned();
    }

    const MINUTE: i64 = 60;
    const HOUR: i64 = 60 * MINUTE;
    const DAY: i64 = 24 * HOUR;

    if seconds < MINUTE {
        "just now".to_owned()
    } else if seconds < HOUR {
        plural(seconds / MINUTE, "minute")
    } else if seconds < DAY {
        plural(seconds / HOUR, "hour")
    } else {
        plural(seconds / DAY, "day")
    }
}

fn plural(n: i64, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun} ago")
    } else {
        format!("{n} {noun}s ago")
    }
}

/// Days since the epoch to a civil date, by Howard Hinnant's algorithm.
///
/// Shifts the year to start in March so that the leap day is the last of it and
/// the month lengths become a straight line, which is what makes this closed
/// form possible at all.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_epoch_and_some_dates_after_it() {
        assert_eq!(date_time(0), "1970-01-01 00:00 UTC");
        // A leap day, which is where a wrong month-length table shows up.
        assert_eq!(date_time(1_709_164_800), "2024-02-29 00:00 UTC");
        // The day after it, and the turn of a century that is not a leap year.
        assert_eq!(date_time(1_709_251_200), "2024-03-01 00:00 UTC");
        assert_eq!(date_time(951_782_400), "2000-02-29 00:00 UTC");
    }

    #[test]
    fn the_time_of_day_comes_out_too() {
        assert_eq!(date_time(3600 * 13 + 60 * 45), "1970-01-01 13:45 UTC");
        assert_eq!(date_time(86_399), "1970-01-01 23:59 UTC");
    }

    #[test]
    fn a_date_before_the_epoch_lands_on_the_right_day() {
        // Truncating division would put this on 1970-01-01, a day late: the
        // reason the arithmetic above is explicitly euclidean.
        assert_eq!(date_time(-1), "1969-12-31 23:59 UTC");
    }

    #[test]
    fn distances_pick_the_unit_that_reads() {
        assert_eq!(ago(0), "just now");
        assert_eq!(ago(59), "just now");
        assert_eq!(ago(60), "1 minute ago");
        assert_eq!(ago(119), "1 minute ago");
        assert_eq!(ago(3600), "1 hour ago");
        assert_eq!(ago(86_400), "1 day ago");
        assert_eq!(ago(86_400 * 3), "3 days ago");
    }

    #[test]
    fn a_stamp_from_the_future_is_said_plainly() {
        // Two machines syncing with clocks a little apart is ordinary; it must
        // not turn into "-1 days ago" or a panic.
        assert_eq!(ago(-5), "in the future");
        assert!(moment(100, 50).ends_with("(in the future)"));
    }

    #[test]
    fn a_moment_carries_both_readings() {
        let rendered = moment(1_709_164_800, 1_709_164_800 + 7200);
        assert_eq!(rendered, "2024-02-29 00:00 UTC (2 hours ago)");
    }
}
