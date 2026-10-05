//! Calendar arithmetic: Unix seconds to a date and back.
//!
//! sefy stores times as Unix seconds. Other tools' exports write dates as text
//! (`2024-02-29T13:45:00Z`), so reading one in and writing one out needs the
//! conversion both ways. A date crate for that would be a dependency carried
//! into every build and every audit of what a secret store links against — a
//! poor trade for arithmetic that is a few lines and has not changed since 1582.
//!
//! Everything here is UTC. An offset in a date being read is applied; nothing
//! is ever written in a local zone.

/// Days since the epoch to a civil date, by Howard Hinnant's algorithm.
///
/// Shifts the year to start in March so that the leap day is the last of it and
/// the month lengths become a straight line, which is what makes this closed
/// form possible at all.
pub fn civil_from_days(days: i64) -> (i64, u32, u32) {
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

/// A civil date to days since the epoch: the inverse of [`civil_from_days`].
pub fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let yoe = year.rem_euclid(400);
    let month = i64::from(month);
    let mp = if month > 2 { month - 3 } else { month + 9 };
    let doy = (153 * mp + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// `YYYY-MM-DDTHH:MM:SSZ` for a Unix timestamp.
pub fn format_rfc3339(at: i64) -> String {
    // Floor division, so times before 1970 land on the right day rather than
    // one too late.
    let (year, month, day) = civil_from_days(at.div_euclid(86_400));
    let seconds = at.rem_euclid(86_400);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        seconds / 3600,
        (seconds % 3600) / 60,
        seconds % 60
    )
}

/// Reads an RFC 3339 date and time into Unix seconds.
///
/// Lenient where exports in the wild are: a space may stand for the `T`, a
/// fraction of a second is accepted and dropped, and a time without a zone is
/// taken as UTC. Anything that is not a real date — month 13, February 30 — is
/// `None` rather than a date quietly rolled into the next month.
pub fn parse_rfc3339(text: &str) -> Option<i64> {
    let text = text.trim();
    let bytes = text.as_bytes();
    if bytes.len() < 19 || bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    if !matches!(bytes[10], b'T' | b't' | b' ') || bytes[13] != b':' || bytes[16] != b':' {
        return None;
    }

    let year: i64 = digits(&text[0..4])?;
    let month: u32 = digits(&text[5..7])?;
    let day: u32 = digits(&text[8..10])?;
    let hour: i64 = digits(&text[11..13])?;
    let minute: i64 = digits(&text[14..16])?;
    let second: i64 = digits(&text[17..19])?;
    if !(1..=12).contains(&month) || day == 0 || day > days_in_month(year, month) {
        return None;
    }
    // A leap second reads as the last second of its minute: nothing stored
    // here is that precise, and refusing the date would be worse.
    if hour > 23 || minute > 59 || second > 60 {
        return None;
    }

    let mut rest = &text[19..];
    if let Some(fraction) = rest.strip_prefix('.') {
        let end = fraction
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(fraction.len());
        if end == 0 {
            return None;
        }
        rest = &fraction[end..];
    }

    let offset = match rest {
        "" | "Z" | "z" => 0,
        zone => {
            let sign = match zone.as_bytes()[0] {
                b'+' => 1,
                b'-' => -1,
                _ => return None,
            };
            let zone = &zone[1..];
            let (hours, minutes) = match zone.len() {
                5 if zone.as_bytes()[2] == b':' => (&zone[0..2], &zone[3..5]),
                4 => (&zone[0..2], &zone[2..4]),
                2 => (&zone[0..2], "00"),
                _ => return None,
            };
            let hours: i64 = digits(hours)?;
            let minutes: i64 = digits(minutes)?;
            if hours > 23 || minutes > 59 {
                return None;
            }
            sign * (hours * 3600 + minutes * 60)
        }
    };

    let second = second.min(59);
    Some(days_from_civil(year, month, day) * 86_400 + hour * 3600 + minute * 60 + second - offset)
}

fn digits<T: std::str::FromStr>(text: &str) -> Option<T> {
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

fn days_in_month(year: i64, month: u32) -> u32 {
    match month {
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn days_and_dates_are_inverses_across_the_awkward_ones() {
        // Every day from 1600 to 2400: leap days, century years that are not
        // leap years, and the one in 2000 that is.
        let start = days_from_civil(1600, 1, 1);
        let end = days_from_civil(2400, 12, 31);
        for days in start..=end {
            let (year, month, day) = civil_from_days(days);
            assert_eq!(
                days_from_civil(year, month, day),
                days,
                "{year}-{month}-{day}"
            );
        }
        assert_eq!(days_from_civil(1970, 1, 1), 0);
    }

    #[test]
    fn a_date_reads_back_as_it_was_written() {
        for at in [
            0,
            1_709_164_800,
            1_709_251_199,
            951_782_400,
            -1,
            4_102_444_800,
        ] {
            assert_eq!(parse_rfc3339(&format_rfc3339(at)), Some(at), "{at}");
        }
        assert_eq!(
            format_rfc3339(1_709_164_800 + 13 * 3600 + 45 * 60 + 7),
            "2024-02-29T13:45:07Z"
        );
    }

    #[test]
    fn the_shapes_exports_write_are_all_read() {
        let at = 1_709_210_700; // 2024-02-29T12:45:00Z
        assert_eq!(parse_rfc3339("2024-02-29T12:45:00Z"), Some(at));
        assert_eq!(parse_rfc3339("2024-02-29T12:45:00.123Z"), Some(at));
        assert_eq!(parse_rfc3339("2024-02-29T12:45:00.1234567Z"), Some(at));
        assert_eq!(parse_rfc3339("2024-02-29 12:45:00"), Some(at));
        assert_eq!(parse_rfc3339("2024-02-29T14:45:00+02:00"), Some(at));
        assert_eq!(parse_rfc3339("2024-02-29T09:45:00-0300"), Some(at));
        assert_eq!(parse_rfc3339("  2024-02-29T12:45:00Z "), Some(at));
    }

    #[test]
    fn a_date_that_does_not_exist_is_refused_rather_than_rolled_over() {
        assert_eq!(parse_rfc3339("2023-02-29T00:00:00Z"), None);
        assert_eq!(parse_rfc3339("2100-02-29T00:00:00Z"), None);
        assert_eq!(parse_rfc3339("2024-13-01T00:00:00Z"), None);
        assert_eq!(parse_rfc3339("2024-04-31T00:00:00Z"), None);
        assert_eq!(parse_rfc3339("2024-01-01T24:00:00Z"), None);
        assert_eq!(parse_rfc3339("2024-01-01T00:00:00Q"), None);
        assert_eq!(parse_rfc3339("2024-01-01T00:00:00."), None);
        assert_eq!(parse_rfc3339("yesterday"), None);
        assert_eq!(parse_rfc3339(""), None);
    }
}
