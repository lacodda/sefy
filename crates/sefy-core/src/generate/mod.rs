//! Generating new secrets: random passwords, pronounceable ones and passphrases.
//!
//! Every recipe knows its own entropy exactly, because it is computed from how
//! the value was drawn rather than estimated from how the value looks. That
//! figure is the honest measure of a generated secret; [`crate::strength`] is
//! for secrets a person chose.
//!
//! Randomness comes from the operating system through `getrandom`, and every
//! draw is uniform: an index into a set of `n` choices is taken by rejection
//! sampling, never by a bare modulo, which would favour the low end of the set.
//!
//! # Word lists
//!
//! - English: the EFF large wordlist, 7776 words, by the Electronic Frontier
//!   Foundation, licensed CC BY 3.0 US
//!   (<https://www.eff.org/dice>).
//! - Russian: Russian Diceware 4d6 by Igor Malinyak, 1296 words, dedicated to
//!   the public domain under CC0 1.0
//!   (<https://github.com/igor-malinyak/Russian-Diceware-4d6>, commit
//!   `639e9122`). `ё` is written as `е`: a passphrase is typed by hand, and a
//!   keyboard layout that makes `ё` awkward would turn a correct memory into a
//!   failed sign-in. The substitution collides no two words - a test holds it
//!   to that.

use crate::error::{Error, Result};
use zeroize::Zeroizing;

/// Upper-case letters.
const UPPERCASE: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
/// Lower-case letters.
const LOWERCASE: &str = "abcdefghijklmnopqrstuvwxyz";
/// Decimal digits.
const DIGITS: &str = "0123456789";
/// Punctuation that survives the places a password gets pasted into.
///
/// No quotes, no backtick, no backslash and no space: those are the characters
/// that a shell, a config file or a careless web form mangles, and a password
/// that breaks on the way in costs more than the few bits they would add.
const SYMBOLS: &str = "!#$%&()*+,-./:;<=>?@[]^_{|}~";

/// Consonants of a pronounceable password. `q` is left out: it wants a `u`
/// after it to be pronounceable, and the alternation cannot promise one.
const CONSONANTS: &str = "bcdfghjklmnprstvwxz";
/// Vowels of a pronounceable password.
const VOWELS: &str = "aeiou";

/// The longest password or passphrase sefy generates, in characters or words.
///
/// Far beyond anything a site accepts; the cap only keeps a typo such as
/// `--length 2000000` from filling the terminal.
pub const MAX_LENGTH: usize = 1024;

/// Entropy below which a generated secret is worth a word of caution.
///
/// Fewer bits hold up behind a site that limits sign-in attempts, which is
/// what zxcvbn's scale measures. A master password or a disk key is attacked
/// offline instead, at whatever rate the attacker's hardware allows, and wants
/// at least this much.
pub const OFFLINE_BITS: f64 = 64.0;

/// A language whose word list passphrases are drawn from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    /// The EFF large wordlist, 7776 words.
    English,
    /// Russian Diceware 4d6, 1296 words.
    Russian,
}

impl Language {
    /// Every word of the list, in list order.
    pub fn words(self) -> &'static [&'static str] {
        match self {
            Self::English => &words::ENGLISH,
            Self::Russian => &words::RUSSIAN,
        }
    }
}

/// Which characters a random password is drawn from.
///
/// Lower-case letters are always in. Each class that is in appears at least
/// once in every password, because that is what the sites that ask for "a
/// digit and a symbol" check for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Classes {
    /// Include upper-case letters.
    pub uppercase: bool,
    /// Include digits.
    pub digits: bool,
    /// Include symbols.
    pub symbols: bool,
}

impl Classes {
    /// Every class: letters of both cases, digits and symbols.
    pub const ALL: Self = Self {
        uppercase: true,
        digits: true,
        symbols: true,
    };

    fn sets(self) -> Vec<&'static str> {
        let mut sets = vec![LOWERCASE];
        if self.uppercase {
            sets.push(UPPERCASE);
        }
        if self.digits {
            sets.push(DIGITS);
        }
        if self.symbols {
            sets.push(SYMBOLS);
        }
        sets
    }
}

/// How to make a secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Recipe {
    /// Random characters from the given classes.
    Characters {
        /// Length in characters.
        length: usize,
        /// Classes to draw from, each present at least once.
        classes: Classes,
    },
    /// Alternating consonants and vowels: easier to read out and to type.
    Pronounceable {
        /// Length in characters.
        length: usize,
    },
    /// Words from a list, joined by a separator.
    Words {
        /// How many words.
        count: usize,
        /// Which list they come from.
        language: Language,
        /// What goes between them.
        separator: String,
    },
}

/// A generated secret and what it is worth.
pub struct Generated {
    /// The secret. Wiped from memory when dropped.
    pub value: Zeroizing<String>,
    /// Entropy in bits: log2 of how many values the recipe could have produced,
    /// every one of them equally likely.
    pub bits: f64,
}

impl std::fmt::Debug for Generated {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Generated")
            .field("value", &"<redacted>")
            .field("bits", &self.bits)
            .finish()
    }
}

/// Makes a secret by `recipe`, from the operating system's random source.
pub fn generate(recipe: &Recipe) -> Result<Generated> {
    generate_with(recipe, &mut OsRandom)
}

/// A source of uniformly random 32-bit words.
trait Random {
    fn next_u32(&mut self) -> Result<u32>;
}

/// The operating system's random source.
struct OsRandom;

impl Random for OsRandom {
    fn next_u32(&mut self) -> Result<u32> {
        let mut bytes = [0u8; 4];
        getrandom::fill(&mut bytes).map_err(|_| Error::Random)?;
        Ok(u32::from_le_bytes(bytes))
    }
}

fn generate_with(recipe: &Recipe, random: &mut impl Random) -> Result<Generated> {
    match recipe {
        Recipe::Characters { length, classes } => {
            let sets = classes.sets();
            check_length(*length, sets.len(), "characters")?;
            let alphabet: Vec<char> = sets.iter().flat_map(|set| set.chars()).collect();
            // Drawn whole and thrown away whole until every class is present.
            // Patching a missing class in afterwards would put it at a
            // predictable place; rejection keeps every valid password exactly
            // as likely as every other.
            let value = loop {
                let mut candidate = Zeroizing::new(String::with_capacity(*length));
                for _ in 0..*length {
                    candidate.push(alphabet[index(alphabet.len(), random)?]);
                }
                if sets
                    .iter()
                    .all(|set| candidate.chars().any(|c| set.contains(c)))
                {
                    break candidate;
                }
            };
            let sizes: Vec<usize> = sets.iter().map(|set| set.chars().count()).collect();
            Ok(Generated {
                value,
                bits: bits_with_every_class(&sizes, *length),
            })
        }
        Recipe::Pronounceable { length } => {
            check_length(*length, 1, "characters")?;
            let consonants: Vec<char> = CONSONANTS.chars().collect();
            let vowels: Vec<char> = VOWELS.chars().collect();
            let mut value = Zeroizing::new(String::with_capacity(*length));
            let mut bits = 0.0;
            for position in 0..*length {
                let set = if position % 2 == 0 {
                    &consonants
                } else {
                    &vowels
                };
                value.push(set[index(set.len(), random)?]);
                bits += (set.len() as f64).log2();
            }
            Ok(Generated { value, bits })
        }
        Recipe::Words {
            count,
            language,
            separator,
        } => {
            check_length(*count, 1, "words")?;
            let list = language.words();
            let mut value = Zeroizing::new(String::new());
            for position in 0..*count {
                if position > 0 {
                    value.push_str(separator);
                }
                value.push_str(list[index(list.len(), random)?]);
            }
            Ok(Generated {
                value,
                bits: *count as f64 * (list.len() as f64).log2(),
            })
        }
    }
}

fn check_length(length: usize, minimum: usize, unit: &'static str) -> Result<()> {
    if length < minimum || length > MAX_LENGTH {
        return Err(Error::GeneratorLength {
            length,
            minimum,
            maximum: MAX_LENGTH,
            unit,
        });
    }
    Ok(())
}

/// A uniform index into a set of `n` choices.
///
/// Draws above the largest multiple of `n` are thrown away and drawn again: a
/// plain `draw % n` would make the first `2^32 mod n` choices slightly more
/// likely than the rest.
fn index(n: usize, random: &mut impl Random) -> Result<usize> {
    let n = u32::try_from(n).expect("a choice set fits in 32 bits");
    assert!(n > 0, "cannot choose from nothing");
    let limit = u32::MAX - u32::MAX % n;
    loop {
        let draw = random.next_u32()?;
        if draw < limit {
            return Ok((draw % n) as usize);
        }
    }
}

/// Entropy of a password of `length` characters drawn from classes of the
/// given sizes, when only the passwords holding every class are kept.
///
/// log2 of how many such passwords there are, counted by inclusion-exclusion
/// over the classes left out. It is worked as a ratio to `N^length` so no term
/// overflows a float, however long the password.
fn bits_with_every_class(sizes: &[usize], length: usize) -> f64 {
    let total: usize = sizes.iter().sum();
    let mut ratio = 0.0;
    for excluded in 0u32..(1 << sizes.len()) {
        let missing: usize = sizes
            .iter()
            .enumerate()
            .filter(|(position, _)| excluded & (1 << position) != 0)
            .map(|(_, size)| size)
            .sum();
        let term = ((total - missing) as f64 / total as f64).powi(length as i32);
        if excluded.count_ones() % 2 == 0 {
            ratio += term;
        } else {
            ratio -= term;
        }
    }
    length as f64 * (total as f64).log2() + ratio.log2()
}

mod words {
    use std::sync::LazyLock;

    pub static ENGLISH: LazyLock<Vec<&'static str>> =
        LazyLock::new(|| include_str!("words/en.txt").lines().collect());
    pub static RUSSIAN: LazyLock<Vec<&'static str>> =
        LazyLock::new(|| include_str!("words/ru.txt").lines().collect());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// Hands out a fixed sequence of draws, then fails the test if asked for
    /// more.
    struct Scripted(Vec<u32>);

    impl Scripted {
        fn new(draws: &[u32]) -> Self {
            let mut draws = draws.to_vec();
            draws.reverse();
            Self(draws)
        }
    }

    impl Random for Scripted {
        fn next_u32(&mut self) -> Result<u32> {
            Ok(self.0.pop().expect("the test ran out of scripted draws"))
        }
    }

    #[test]
    fn a_draw_past_the_last_whole_multiple_is_drawn_again() {
        // 2^32 - 1 is 5 more than the largest multiple of 10. Taken modulo 10
        // it would come out as 5; it has to be thrown away instead, so the
        // answer is the next draw.
        let mut random = Scripted::new(&[u32::MAX, 7]);
        assert_eq!(index(10, &mut random).unwrap(), 7);
    }

    #[test]
    fn the_last_draw_below_the_limit_is_kept() {
        let limit = u32::MAX - u32::MAX % 10;
        let mut random = Scripted::new(&[limit - 1]);
        assert_eq!(index(10, &mut random).unwrap(), 9);
    }

    #[test]
    fn every_class_appears_in_every_password() {
        // Four characters from four classes is the tightest case: most draws
        // miss a class, so a generator that did not insist would fail here
        // almost at once.
        let recipe = Recipe::Characters {
            length: 4,
            classes: Classes::ALL,
        };
        for _ in 0..500 {
            let generated = generate(&recipe).unwrap();
            for set in [LOWERCASE, UPPERCASE, DIGITS, SYMBOLS] {
                assert!(
                    generated.value.chars().any(|c| set.contains(c)),
                    "a password lacks a class it was asked for"
                );
            }
        }
    }

    #[test]
    fn a_class_left_out_never_appears() {
        let recipe = Recipe::Characters {
            length: 64,
            classes: Classes {
                uppercase: false,
                digits: true,
                symbols: false,
            },
        };
        for _ in 0..50 {
            let generated = generate(&recipe).unwrap();
            assert!(
                generated
                    .value
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
            );
        }
    }

    #[test]
    fn the_entropy_counts_exactly_the_passwords_that_are_kept() {
        // Two classes of 2 and 3 symbols, length 3: 5^3 strings in all, less
        // the 3^3 with no symbol of the first class and the 2^3 with none of
        // the second - 90 passwords.
        assert!((bits_with_every_class(&[2, 3], 3) - 90f64.log2()).abs() < 1e-9);
        // One class has nothing to leave out.
        assert!((bits_with_every_class(&[26], 10) - 10.0 * 26f64.log2()).abs() < 1e-9);
    }

    #[test]
    fn a_long_password_does_not_overflow_its_entropy() {
        let bits = bits_with_every_class(&[26, 26, 10, 28], MAX_LENGTH);
        assert!(bits.is_finite());
        assert!((bits - MAX_LENGTH as f64 * 90f64.log2()).abs() < 1e-6);
    }

    #[test]
    fn a_password_is_as_long_as_asked() {
        for length in [4, 20, 100] {
            let generated = generate(&Recipe::Characters {
                length,
                classes: Classes::ALL,
            })
            .unwrap();
            assert_eq!(generated.value.chars().count(), length);
        }
    }

    #[test]
    fn too_short_to_hold_every_class_is_refused() {
        let error = generate(&Recipe::Characters {
            length: 3,
            classes: Classes::ALL,
        })
        .unwrap_err();
        assert!(matches!(error, Error::GeneratorLength { minimum: 4, .. }));
    }

    #[test]
    fn nothing_past_the_cap_is_generated() {
        assert!(
            generate(&Recipe::Pronounceable {
                length: MAX_LENGTH + 1
            })
            .is_err()
        );
        assert!(
            generate(&Recipe::Words {
                count: 0,
                language: Language::English,
                separator: "-".into(),
            })
            .is_err()
        );
    }

    #[test]
    fn a_pronounceable_password_alternates_consonants_and_vowels() {
        let generated = generate(&Recipe::Pronounceable { length: 21 }).unwrap();
        for (position, c) in generated.value.chars().enumerate() {
            let set = if position % 2 == 0 {
                CONSONANTS
            } else {
                VOWELS
            };
            assert!(set.contains(c), "{c:?} at {position} breaks the pattern");
        }
        let expected = 11.0 * 19f64.log2() + 10.0 * 5f64.log2();
        assert!((generated.bits - expected).abs() < 1e-9);
    }

    #[test]
    fn a_passphrase_is_words_from_its_list_joined_by_the_separator() {
        for language in [Language::English, Language::Russian] {
            let generated = generate(&Recipe::Words {
                count: 6,
                language,
                separator: " ".into(),
            })
            .unwrap();
            let words: Vec<&str> = generated.value.split(' ').collect();
            assert_eq!(words.len(), 6);
            for word in words {
                assert!(language.words().contains(&word), "{word:?} is not listed");
            }
            let expected = 6.0 * (language.words().len() as f64).log2();
            assert!((generated.bits - expected).abs() < 1e-9);
        }
    }

    #[test]
    fn the_word_lists_are_the_published_ones() {
        // The entropy a passphrase reports rests on these counts, and on no
        // word appearing twice: a duplicate would make its word likelier than
        // the rest and the reported figure a lie.
        for (language, size) in [(Language::English, 7776), (Language::Russian, 1296)] {
            let list = language.words();
            assert_eq!(list.len(), size, "{language:?}");
            let unique: HashSet<_> = list.iter().collect();
            assert_eq!(unique.len(), size, "{language:?} repeats a word");
            assert!(
                list.iter()
                    .all(|word| !word.is_empty() && word.trim() == *word)
            );
        }
        assert!(
            Language::Russian
                .words()
                .iter()
                .all(|word| word.chars().all(|c| ('а'..='я').contains(&c))),
            "the Russian list holds a letter outside а-я, ё included"
        );
    }

    #[test]
    fn a_generated_value_stays_out_of_debug_output() {
        let generated = generate(&Recipe::Pronounceable { length: 30 }).unwrap();
        assert!(!format!("{generated:?}").contains(generated.value.as_str()));
    }
}
