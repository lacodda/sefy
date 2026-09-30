//! Printing a line-by-line comparison, with long unchanged stretches folded.

use sefy_core::Line;

/// Unchanged lines kept on each side of a change; the rest are folded.
const CONTEXT: usize = 2;

/// Prints a comparison: `-` for what went, `+` for what came, and enough of
/// what stayed to see where. A stretch of unchanged lines longer than the
/// context on both sides becomes one line saying how many were skipped.
pub fn print(lines: &[Line<'_>]) {
    for line in render(lines) {
        println!("{line}");
    }
}

fn render(lines: &[Line<'_>]) -> Vec<String> {
    let changed: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| !matches!(line, Line::Same(_)))
        .map(|(index, _)| index)
        .collect();
    let near_a_change = |index: usize| {
        changed
            .iter()
            .any(|&at| index + CONTEXT >= at && index <= at + CONTEXT)
    };

    let mut out = Vec::new();
    let mut folded = 0;
    for (index, line) in lines.iter().enumerate() {
        if let Line::Same(_) = line
            && !near_a_change(index)
        {
            folded += 1;
            continue;
        }
        if folded > 0 {
            out.push(fold(folded));
            folded = 0;
        }
        out.push(match line {
            Line::Same(text) => format!("    {text}"),
            Line::Removed(text) => format!("  - {text}"),
            Line::Added(text) => format!("  + {text}"),
        });
    }
    if folded > 0 {
        out.push(fold(folded));
    }
    out
}

fn fold(count: usize) -> String {
    format!("    … {}", crate::output::count(count, "unchanged line"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_unchanged_stretch_is_folded_and_counted() {
        let old: Vec<String> = (0..20).map(|n| format!("line {n}")).collect();
        let mut new = old.clone();
        new[10] = "changed".to_owned();
        let (old, new) = (old.join("\n"), new.join("\n"));

        let rendered = render(&sefy_core::history::lines(&old, &new));

        assert_eq!(
            rendered,
            [
                "    … 8 unchanged lines",
                "    line 8",
                "    line 9",
                "  - line 10",
                "  + changed",
                "    line 11",
                "    line 12",
                "    … 7 unchanged lines",
            ]
        );
    }

    #[test]
    fn a_one_line_value_shows_as_what_went_and_what_came() {
        let rendered = render(&sefy_core::history::lines(
            "https://old.example.com",
            "https://new.example.com",
        ));
        assert_eq!(
            rendered,
            ["  - https://old.example.com", "  + https://new.example.com"]
        );
    }
}
