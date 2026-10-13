//! `sefy audit`: what in the vault deserves a second look.
//!
//! The report names items and fields and says what is wrong with them. It
//! never prints a value, nor what made a password weak: "a date is easy to
//! guess" is already a fact about what the password contains.

use crate::commands::now;
use crate::output;
use crate::when;
use anyhow::{Result, bail};
use sefy_core::{Due, Place, Report, Vault};

/// One line under a heading: where, and what about it.
struct Row<'a> {
    /// The group a shared secret belongs to, as `1)`; empty otherwise.
    group: String,
    place: &'a Place,
    detail: String,
}

/// Audits the vault and prints what it found.
///
/// Fails, after printing everything, when it found anything: a scheduled
/// `sefy audit` is then a check that says when to look.
pub fn run(vault: &Vault) -> Result<()> {
    let now = now();
    let report = sefy_core::audit(vault, now)?;
    print(&report, now);

    let findings = report.findings();
    if findings > 0 {
        bail!("{}", output::count(findings, "finding"));
    }
    Ok(())
}

fn print(report: &Report, now: i64) {
    println!(
        "looked at {} and {} in {}",
        output::count(report.passwords, "password"),
        output::count(report.dated, "date"),
        output::count(report.items, "item")
    );
    if report.unknown > 0 {
        println!(
            "{} of a kind this sefy does not know {} not looked into",
            output::count(report.unknown, "item"),
            if report.unknown == 1 { "was" } else { "were" }
        );
    }
    if report.findings() == 0 {
        println!("nothing to look at");
        return;
    }

    let mut sections: Vec<(&str, String, Vec<Row<'_>>)> = Vec::new();

    if !report.weak.is_empty() {
        sections.push((
            "weak",
            format!(
                "{} easy to guess",
                are(report.weak.len(), "password", "is", "are")
            ),
            report
                .weak
                .iter()
                .map(|weak| Row {
                    group: String::new(),
                    place: &weak.place,
                    detail: format!("strength {}/{}", weak.score, sefy_core::Strength::MAX_SCORE),
                })
                .collect(),
        ));
    }
    if !report.reused.is_empty() {
        sections.push((
            "reused",
            format!(
                "{} shared by more than one item",
                are(report.reused.len(), "password", "is", "are each")
            ),
            grouped(&report.reused),
        ));
    }
    if !report.duplicated.is_empty() {
        sections.push((
            "duplicated",
            format!(
                "{} {} in more than one item, likely a leftover copy",
                report.duplicated.len(),
                if report.duplicated.len() == 1 {
                    "card, token or key is"
                } else {
                    "cards, tokens or keys are each"
                }
            ),
            grouped(&report.duplicated),
        ));
    }
    if !report.old.is_empty() {
        sections.push((
            "old",
            format!(
                "{} unchanged for over a year",
                are(report.old.len(), "password", "has been", "have been")
            ),
            report
                .old
                .iter()
                .map(|old| Row {
                    group: String::new(),
                    place: &old.place,
                    detail: format!(
                        "since {} ({} days)",
                        when::date(old.since),
                        (now - old.since).div_euclid(86_400)
                    ),
                })
                .collect(),
        ));
    }
    if !report.expired.is_empty() {
        sections.push((
            "expired",
            format!(
                "{} past the last day it works",
                are(report.expired.len(), "record", "is", "are")
            ),
            dues(&report.expired, now),
        ));
    }
    if !report.expiring.is_empty() {
        sections.push((
            "expiring",
            format!(
                "{} within {} days",
                are(
                    report.expiring.len(),
                    "record",
                    "stops working",
                    "stop working"
                ),
                sefy_core::expiry::SOON_DAYS
            ),
            dues(&report.expiring, now),
        ));
    }
    if !report.unreadable.is_empty() {
        sections.push((
            "unreadable",
            format!(
                "{} not one sefy can read; fix with: sefy edit ID --set {}=2027-01-31",
                are(report.unreadable.len(), "date", "is", "are"),
                sefy_core::expiry::FIELD
            ),
            report
                .unreadable
                .iter()
                .map(|odd| Row {
                    group: String::new(),
                    place: &odd.place,
                    detail: format!("{:?}", odd.value),
                })
                .collect(),
        ));
    }

    // One set of columns for the whole report, so it reads as one table.
    let rows = || sections.iter().flat_map(|(_, _, rows)| rows.iter());
    let label = sections
        .iter()
        .map(|(label, _, _)| label.len())
        .max()
        .unwrap_or(0)
        + 2;
    let id = rows()
        .map(|row| row.place.id.to_string().len())
        .max()
        .unwrap_or(1);
    let title = rows()
        .map(|row| row.place.title.chars().count())
        .max()
        .unwrap_or(0)
        .min(30);
    let kind = rows()
        .map(|row| row.place.kind.as_str().len())
        .max()
        .unwrap_or(0);
    let field = rows()
        .map(|row| row.place.field.chars().count())
        .max()
        .unwrap_or(0);

    for (name, headline, rows) in &sections {
        println!();
        println!("{name:<label$}{headline}");
        for row in rows {
            // A group's number sits against the column of ids, under the
            // heading rather than beside it.
            let line = format!(
                "{:>marker$} {:>id$}  {:<title$}  {:<kind$}  {:<field$}  {}",
                row.group,
                row.place.id,
                output::truncate(&row.place.title, title),
                row.place.kind.as_str(),
                row.place.field,
                row.detail,
                marker = label - 1
            );
            println!("{}", line.trim_end());
        }
    }
    println!();
}

/// `1 password is` or `2 passwords are`.
fn are(n: usize, noun: &str, one: &str, many: &str) -> String {
    format!(
        "{} {}",
        output::count(n, noun),
        if n == 1 { one } else { many }
    )
}

/// The rows of shared secrets, each group numbered on its first line.
fn grouped(groups: &[Vec<Place>]) -> Vec<Row<'_>> {
    groups
        .iter()
        .enumerate()
        .flat_map(|(index, places)| {
            places.iter().enumerate().map(move |(position, place)| Row {
                group: if position == 0 {
                    format!("{})", index + 1)
                } else {
                    String::new()
                },
                place,
                detail: String::new(),
            })
        })
        .collect()
}

fn dues(dues: &[Due], now: i64) -> Vec<Row<'_>> {
    dues.iter()
        .map(|due| Row {
            group: String::new(),
            place: &due.place,
            detail: format!(
                "{} ({})",
                due.expiry,
                when::days_away(due.expiry.days_left(now))
            ),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_count_agrees_with_its_verb() {
        assert_eq!(are(1, "password", "is", "are"), "1 password is");
        assert_eq!(are(2, "password", "is", "are"), "2 passwords are");
    }
}
