//! When a record stops working: the `expires` field.
//!
//! A token, a card, a certificate stops working on a date, however recently
//! it was changed — which is why age alone cannot say when one needs
//! attention. The date is part of a record's contents rather than a label
//! beside them: it belongs to the secret it came with, so a new token brings
//! a new date, and bringing back an earlier version brings back that
//! version's date with it.
//!
//! It is kept in one of two shapes. `2027-01-31` is the last day it works;
//! `2027-01` works through the end of that month, the way a card prints its
//! date. Anything else a person types — a card's own `01/27` — is read into
//! one of those, so a stored date is always one sefy can count from.

use crate::error::{Error, Result};
use crate::model::{Field, ItemKind, Payload};
use crate::time::{civil_from_days, days_from_civil, parse_rfc3339};

/// The name of the field a record's date is kept in, on every kind.
pub const FIELD: &str = "expires";

/// What a card's date was called up to 0.14.0, when it was kept as printed.
///
/// Kept as a name to read, never one to write: a card stored then reads with
/// its date under [`FIELD`], and everything sefy writes from now on says
/// `expires`.
pub const LEGACY_CARD_NAME: &str = "expiry";

/// How many days ahead a date counts as coming up.
///
/// A month: long enough to renew a card or issue a new token without a rush,
/// short enough that a warning means something when it appears.
pub const SOON_DAYS: i64 = 30;

/// A date a record stops working after.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Expiry {
    year: i64,
    month: u32,
    /// `None` for a month as a whole, as a card states it.
    day: Option<u32>,
}

impl Expiry {
    /// A day: the last one it works.
    pub fn day(year: i64, month: u32, day: u32) -> Option<Self> {
        let date = Self {
            year,
            month,
            day: Some(day),
        };
        date.is_real().then_some(date)
    }

    /// A month: it works through the last day of it.
    pub fn month(year: i64, month: u32) -> Option<Self> {
        let date = Self {
            year,
            month,
            day: None,
        };
        date.is_real().then_some(date)
    }

    /// Reads a date in one of the two shapes it is stored in.
    ///
    /// Strict: this is the reading of what sefy wrote. What a person or
    /// another tool wrote goes through [`Expiry::read`].
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        match text.len() {
            10 if text.as_bytes()[4] == b'-' && text.as_bytes()[7] == b'-' => Self::day(
                digits(&text[..4])?,
                digits(&text[5..7])?,
                digits(&text[8..])?,
            ),
            7 if text.as_bytes()[4] == b'-' => {
                Self::month(digits(&text[..4])?, digits(&text[5..])?)
            }
            _ => None,
        }
    }

    /// Reads a date the way people and exports write one.
    ///
    /// The stored shapes, a card's printed `MM/YY` or `MM/YYYY`, and a full
    /// RFC 3339 moment, which becomes its day. Day-first and month-first
    /// dates with slashes are refused rather than guessed at: `03/04/2027`
    /// is a different day on either side of the Atlantic.
    pub fn read(text: &str) -> Result<Self> {
        let text = text.trim();
        if let Some(date) = Self::parse(text) {
            return Ok(date);
        }
        if let Some((month, year)) = text.split_once('/') {
            let year: Option<i64> = match year.len() {
                2 => digits::<i64>(year).map(|year| 2000 + year),
                4 => digits(year),
                _ => None,
            };
            let month: Option<u32> = (1..=2)
                .contains(&month.len())
                .then(|| digits(month))
                .flatten();
            if let (Some(year), Some(month)) = (year, month)
                && let Some(date) = Self::month(year, month)
            {
                return Ok(date);
            }
        }
        if let Some(at) = parse_rfc3339(text) {
            let (year, month, day) = civil_from_days(at.div_euclid(86_400));
            if let Some(date) = Self::day(year, month, day) {
                return Ok(date);
            }
        }
        Err(Error::UnreadableExpiry(text.to_owned()))
    }

    /// The last day it works, in days since the Unix epoch.
    pub fn last_day(&self) -> i64 {
        match self.day {
            Some(day) => days_from_civil(self.year, self.month, day),
            // The day before the first of the next month.
            None if self.month == 12 => days_from_civil(self.year + 1, 1, 1) - 1,
            None => days_from_civil(self.year, self.month + 1, 1) - 1,
        }
    }

    /// Days from the day `now` falls on to the last day it works.
    ///
    /// Zero on that last day, negative once it has passed. Counted in whole
    /// UTC days, like every other date sefy keeps.
    pub fn days_left(&self, now: i64) -> i64 {
        self.last_day() - now.div_euclid(86_400)
    }

    /// The month and year as a card prints them: `03/29`.
    ///
    /// What a payment form asks for, and what `sefy fill` hands over for a
    /// card.
    pub fn printed(&self) -> String {
        format!("{:02}/{:02}", self.month, self.year.rem_euclid(100))
    }

    /// The moment it stops working: the start of the day after the last one.
    pub fn ends_at(&self) -> i64 {
        (self.last_day() + 1) * 86_400
    }

    /// The date of a moment something stops working at: the day before it,
    /// when the moment is the very start of a day, and that day otherwise.
    ///
    /// The reading [`Expiry::ends_at`] is the inverse of, so a date sent out
    /// as a moment comes back as the same date.
    pub fn ending_at(at: i64) -> Option<Self> {
        let (year, month, day) = civil_from_days((at - 1).div_euclid(86_400));
        Self::day(year, month, day)
    }

    fn is_real(&self) -> bool {
        (1000..=9999).contains(&self.year)
            && (1..=12).contains(&self.month)
            && self
                .day
                .is_none_or(|day| day >= 1 && day <= days_in_month(self.year, self.month))
    }
}

impl std::fmt::Display for Expiry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.day {
            Some(day) => write!(f, "{:04}-{:02}-{day:02}", self.year, self.month),
            None => write!(f, "{:04}-{:02}", self.year, self.month),
        }
    }
}

/// Checks a value on its way into an `expires` field and returns what to
/// store: always one of the two stored shapes.
pub fn normalize(text: &str) -> Result<String> {
    Ok(Expiry::read(text)?.to_string())
}

/// What a record says about when it stops working.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stated<'a> {
    /// A date sefy can count from.
    Date(Expiry),
    /// A value that is not one — written by hand into a database, or carried
    /// in from another tool. `expires` is a public field, so the value is
    /// safe to quote.
    Unreadable(&'a str),
}

/// The date a record states, if it states one.
///
/// Notes and files have no fields to state one in.
pub fn of(payload: &Payload) -> Option<Stated<'_>> {
    let field = payload.field(FIELD)?;
    Some(match Expiry::parse(&field.value) {
        Some(date) => Stated::Date(date),
        None => Stated::Unreadable(&field.value),
    })
}

/// Brings a card written up to 0.14.0 to the name its date has now.
///
/// The field moves in place, so the record reads in the order it did, and its
/// value takes the stored shape. A printed date sefy cannot read — "2029" on
/// its own — stays under the old name, as an extra field: nothing is lost,
/// and nothing claims to be a date that is not one. A card that already has
/// an `expires` keeps it and the old field beside it.
pub(crate) fn upgrade_legacy(kind: &ItemKind, fields: &mut [Field]) {
    if *kind != ItemKind::Card || fields.iter().any(|field| field.name == FIELD) {
        return;
    }
    if let Some(field) = fields
        .iter_mut()
        .find(|field| field.name == LEGACY_CARD_NAME)
        && let Ok(date) = Expiry::read(&field.value)
    {
        field.name = FIELD.to_owned();
        field.value = date.to_string();
        // Public, as it always was: a date is printed on the card's face.
        field.secret = false;
    }
}

fn digits<T: std::str::FromStr>(text: &str) -> Option<T> {
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

fn days_in_month(year: i64, month: u32) -> u32 {
    let next = if month == 12 {
        days_from_civil(year + 1, 1, 1)
    } else {
        days_from_civil(year, month + 1, 1)
    };
    (next - days_from_civil(year, month, 1)) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: i64 = 86_400;

    fn date(text: &str) -> Expiry {
        Expiry::read(text).unwrap()
    }

    #[test]
    fn a_date_is_stored_in_one_of_two_shapes_and_read_back_from_them() {
        assert_eq!(normalize("2027-01-31").unwrap(), "2027-01-31");
        assert_eq!(normalize(" 2027-01 ").unwrap(), "2027-01");
        assert_eq!(Expiry::parse("2027-01-31"), Expiry::day(2027, 1, 31));
        assert_eq!(Expiry::parse("2027-01"), Expiry::month(2027, 1));
    }

    #[test]
    fn a_card_date_as_printed_becomes_its_month() {
        assert_eq!(normalize("03/29").unwrap(), "2029-03");
        assert_eq!(normalize("3/29").unwrap(), "2029-03");
        assert_eq!(normalize("03/2029").unwrap(), "2029-03");
        assert_eq!(date("2029-03").printed(), "03/29");
        assert_eq!(date("2029-03-15").printed(), "03/29");
    }

    #[test]
    fn a_moment_becomes_the_day_it_falls_on() {
        assert_eq!(normalize("2027-01-31T18:00:00Z").unwrap(), "2027-01-31");
        assert_eq!(
            normalize("2027-02-01T01:00:00+02:00").unwrap(),
            "2027-01-31"
        );
    }

    #[test]
    fn what_is_not_a_date_is_refused_rather_than_guessed_at() {
        for text in [
            "",
            "next spring",
            "2029",
            "13/29",
            "00/29",
            "03/04/2027",
            "2027-02-30",
            "2027-13",
            "27-01-31",
            "03/123",
        ] {
            assert!(
                matches!(Expiry::read(text), Err(Error::UnreadableExpiry(_))),
                "{text:?}"
            );
        }
        // The strict reading takes only what sefy stores.
        assert_eq!(Expiry::parse("03/29"), None);
    }

    #[test]
    fn a_month_works_through_its_last_day_and_a_day_through_itself() {
        let leap = date("2028-02");
        assert_eq!(leap.last_day(), days_from_civil(2028, 2, 29));
        let december = date("2027-12");
        assert_eq!(december.last_day(), days_from_civil(2027, 12, 31));
        let day = date("2027-06-15");
        assert_eq!(day.last_day(), days_from_civil(2027, 6, 15));
    }

    #[test]
    fn days_left_is_zero_on_the_last_day_and_negative_after_it() {
        let last = days_from_civil(2027, 6, 15) * DAY;
        let expiry = date("2027-06-15");
        assert_eq!(expiry.days_left(last), 0);
        assert_eq!(expiry.days_left(last + DAY - 1), 0);
        assert_eq!(expiry.days_left(last + DAY), -1);
        assert_eq!(expiry.days_left(last - 1), 1);
        assert_eq!(expiry.days_left(last - 30 * DAY), 30);
    }

    #[test]
    fn a_date_sent_out_as_a_moment_comes_back_as_itself() {
        for text in ["2027-01-31", "2028-02-29", "1999-12-31"] {
            let expiry = date(text);
            assert_eq!(Expiry::ending_at(expiry.ends_at()), Some(expiry), "{text}");
        }
        // A month goes out as its last day.
        let month = date("2029-03");
        assert_eq!(
            Expiry::ending_at(month.ends_at()).unwrap().to_string(),
            "2029-03-31"
        );
        // A moment in the middle of a day is that day.
        let noon = days_from_civil(2027, 1, 31) * DAY + 12 * 3600;
        assert_eq!(Expiry::ending_at(noon).unwrap().to_string(), "2027-01-31");
    }

    #[test]
    fn a_record_states_a_date_or_something_that_is_not_one() {
        let token = Payload::fields(
            ItemKind::ApiToken,
            [
                Field::secret("token", "t"),
                Field::public(FIELD, "2027-01-31"),
            ],
        );
        assert_eq!(of(&token), Some(Stated::Date(date("2027-01-31"))));

        let odd = Payload::fields(ItemKind::ApiToken, [Field::public(FIELD, "never")]);
        assert_eq!(of(&odd), Some(Stated::Unreadable("never")));

        let note = Payload::Note {
            text: "expires: 2027-01-31".to_owned(),
        };
        assert_eq!(of(&note), None);
    }

    #[test]
    fn a_card_from_before_reads_with_its_date_under_the_new_name() {
        let mut fields = vec![
            Field::secret("number", "4111"),
            Field::public(LEGACY_CARD_NAME, "01/29"),
            Field::secret("cvv", "123"),
        ];
        upgrade_legacy(&ItemKind::Card, &mut fields);
        let names: Vec<&str> = fields.iter().map(|field| field.name.as_str()).collect();
        assert_eq!(names, ["number", FIELD, "cvv"], "moved in place");
        assert_eq!(fields[1].value, "2029-01");
        assert!(!fields[1].secret);
    }

    #[test]
    fn an_old_card_date_is_public_whatever_it_arrived_as() {
        // A name a template does not list arrives secret from an import; under
        // the new name it is the template's public date.
        let mut fields = vec![Field::secret(LEGACY_CARD_NAME, "01/29")];
        upgrade_legacy(&ItemKind::Card, &mut fields);
        assert_eq!(fields[0].name, FIELD);
        assert!(!fields[0].secret);
    }

    #[test]
    fn an_old_card_date_that_is_not_one_keeps_its_old_name() {
        let mut fields = vec![Field::public(LEGACY_CARD_NAME, "2029")];
        upgrade_legacy(&ItemKind::Card, &mut fields);
        assert_eq!(fields[0].name, LEGACY_CARD_NAME);
        assert_eq!(fields[0].value, "2029");
    }

    #[test]
    fn only_a_card_has_an_old_name_to_read() {
        let mut fields = vec![Field::public(LEGACY_CARD_NAME, "01/29")];
        upgrade_legacy(&ItemKind::ApiToken, &mut fields);
        assert_eq!(fields[0].name, LEGACY_CARD_NAME);

        // Nor is a card's own new field overwritten by an old one beside it.
        let mut fields = vec![
            Field::public(FIELD, "2030-05"),
            Field::public(LEGACY_CARD_NAME, "01/29"),
        ];
        upgrade_legacy(&ItemKind::Card, &mut fields);
        assert_eq!(fields[0].value, "2030-05");
        assert_eq!(fields[1].name, LEGACY_CARD_NAME);
    }
}
