//! Choosing an item by typing at it.
//!
//! The command line is exact and remembering is not: `sefy get github-work`
//! only helps someone who knows the title is `github-work` and not
//! `github (work)`. The picker is the answer to "it is in there somewhere" —
//! type a few letters, see what matches, take one.
//!
//! It is offered rather than imposed. A picker that appears when stdout is a
//! pipe would hang a script forever on a prompt nobody can see, so every entry
//! point here checks for a terminal first and falls back to printing the same
//! items as a table. That is the difference between an interactive convenience
//! and a command that behaves differently depending on where it is run.

use anyhow::Result;
use dialoguer::{FuzzySelect, theme::ColorfulTheme};
use sefy_core::ItemSummary;
use std::io::IsTerminal;

/// Whether an interactive prompt can be shown at all.
///
/// Both ends are checked. Without a terminal on input there is nobody to type;
/// without one on output the prompt would be drawn into a pipe, where it is
/// both invisible and part of whatever the caller is parsing.
pub fn available() -> bool {
    std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}

/// Asks the user to pick one of `items`, returning what they chose.
///
/// `None` means they pressed Escape, which is an answer rather than a failure:
/// the caller should do nothing, quietly.
pub fn choose(items: &[ItemSummary], prompt: &str) -> Result<Option<ItemSummary>> {
    if items.is_empty() {
        return Ok(None);
    }

    // One item is not a choice. Asking anyway would make the common case -
    // typing enough of a title to be unambiguous - cost an extra keystroke for
    // nothing.
    if let [only] = items {
        return Ok(Some(only.clone()));
    }

    let labels: Vec<String> = items.iter().map(label).collect();

    let picked = FuzzySelect::with_theme(&ColorfulTheme::default())
        .with_prompt(prompt)
        .items(&labels)
        .default(0)
        // Without a cap the list is as tall as the vault, which on a full one
        // scrolls the terminal's own history away before a key is pressed.
        .max_length(15)
        .interact_opt()?;

    Ok(picked.map(|index| items[index].clone()))
}

/// One line of the list: what a person would recognise the item by.
///
/// Title first, because that is what is being searched for; the kind and tags
/// after it, because two items called `mail` are told apart by them. The id is
/// left out: it is how a *script* names an item, and someone reading a list of
/// their own secrets is not matching numbers.
///
/// Nothing secret is in here. The picker renders into the terminal's scrollback
/// like any other output, and a value on that screen outlives the command.
fn label(item: &ItemSummary) -> String {
    let mut line = item.title.clone();
    line.push_str("  ");
    line.push_str(item.kind.as_str());
    if !item.tags.is_empty() {
        line.push_str("  [");
        line.push_str(&item.tags.join(", "));
        line.push(']');
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;
    use sefy_core::ItemKind;

    fn summary(title: &str, kind: ItemKind, tags: &[&str]) -> ItemSummary {
        ItemSummary {
            id: 1,
            uuid: String::new(),
            title: title.to_owned(),
            kind,
            tags: tags.iter().map(|tag| (*tag).to_owned()).collect(),
            created_at: 0,
            updated_at: 0,
        }
    }

    #[test]
    fn a_label_carries_what_tells_two_items_apart() {
        assert_eq!(
            label(&summary("mail", ItemKind::Login, &["work"])),
            "mail  login  [work]"
        );
        assert_eq!(
            label(&summary("mail", ItemKind::Login, &["home"])),
            "mail  login  [home]"
        );
        assert_eq!(label(&summary("bank", ItemKind::Note, &[])), "bank  note");
    }

    #[test]
    fn an_empty_list_is_no_choice_rather_than_a_prompt() {
        // Reached when a filter matches nothing. A prompt over an empty list
        // is a dead end the user has to escape from.
        assert!(choose(&[], "pick").unwrap().is_none());
    }

    #[test]
    fn a_single_item_is_taken_without_asking() {
        // Also what keeps this callable from a test: no terminal is touched.
        let only = summary("mail", ItemKind::Login, &[]);
        let picked = choose(std::slice::from_ref(&only), "pick").unwrap();
        assert_eq!(picked.map(|item| item.title), Some("mail".to_owned()));
    }
}
