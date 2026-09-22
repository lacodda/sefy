//! How hard a password would be to guess, estimated offline with zxcvbn.
//!
//! zxcvbn models the way people choose passwords - words, names, dates,
//! keyboard runs, substitutions - and counts how many guesses an attacker who
//! knows those habits would need. It runs entirely on this machine: nothing
//! about the password is looked up anywhere.
//!
//! It is the right measure for a password a person chose. For one sefy
//! generated, the generator's own entropy is exact and this is a second
//! opinion; zxcvbn caps its estimate for random strings by their length and
//! reads only the first 100 characters.

/// What zxcvbn makes of a password.
#[derive(Debug, Clone, PartialEq)]
pub struct Strength {
    /// 0 (guessable in moments) to 4 (very unguessable), zxcvbn's own scale.
    pub score: u8,
    /// log10 of the estimated number of guesses.
    pub guesses_log10: f64,
    /// What makes it weak, when zxcvbn can name something.
    pub warning: Option<String>,
}

impl Strength {
    /// The highest score on the scale.
    pub const MAX_SCORE: u8 = 4;
}

/// Estimates how hard `password` is to guess.
///
/// `context` holds words the password should not lean on - the item's title,
/// its login, the site it is for. A password built from them scores as the
/// guessable thing it is.
pub fn estimate(password: &str, context: &[&str]) -> Strength {
    let entropy = zxcvbn::zxcvbn(password, context);
    Strength {
        score: u8::from(entropy.score()),
        guesses_log10: entropy.guesses_log10(),
        warning: entropy
            .feedback()
            .and_then(|feedback| feedback.warning())
            .map(|warning| warning.to_string()),
    }
}

/// What zxcvbn makes of a password sefy generated, held to what the generator
/// knows about it.
///
/// zxcvbn judges a string by how it looks, and a passphrase from a word list it
/// does not carry looks stronger than it is: two EFF words are 26 bits, yet
/// zxcvbn scores them at the top of its scale. The generator's entropy is
/// exact, so the estimate may not claim more guesses than half the space the
/// value was drawn from - what an attacker who knows the recipe needs on
/// average.
pub fn estimate_generated(password: &str, bits: f64, context: &[&str]) -> Strength {
    let mut strength = estimate(password, context);
    let known = (bits - 1.0).max(0.0) * std::f64::consts::LOG10_2;
    if known < strength.guesses_log10 {
        strength.guesses_log10 = known;
        strength.score = score_for(known);
    }
    strength
}

/// zxcvbn's own scale, from log10 of the guesses: under a thousand is 0, under
/// a million 1, under a hundred million 2, under ten billion 3, beyond that 4.
fn score_for(guesses_log10: f64) -> u8 {
    match guesses_log10 {
        g if g < 3.0 => 0,
        g if g < 6.0 => 1,
        g if g < 8.0 => 2,
        g if g < 10.0 => 3,
        _ => Strength::MAX_SCORE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_generated_value_scores_no_higher_than_its_entropy_allows() {
        // Two words from the EFF list: zxcvbn alone rates them at the top.
        let bits = 2.0 * 7776f64.log2();
        assert_eq!(
            estimate("fountain-fastness", &[]).score,
            Strength::MAX_SCORE
        );

        let strength = estimate_generated("fountain-fastness", bits, &[]);
        assert_eq!(strength.score, 2, "{strength:?}");
        assert!((strength.guesses_log10 - (bits - 1.0) * std::f64::consts::LOG10_2).abs() < 1e-9);
    }

    #[test]
    fn plenty_of_entropy_does_not_lift_a_score_zxcvbn_marked_down() {
        // The cap only ever lowers: a generator that happened to produce a
        // guessable string is judged on the string.
        let strength = estimate_generated("password1", 128.0, &[]);
        assert_eq!(strength, estimate("password1", &[]));
    }

    #[test]
    fn the_scale_matches_zxcvbn_at_its_thresholds() {
        assert_eq!(score_for(2.99), 0);
        assert_eq!(score_for(3.0), 1);
        assert_eq!(score_for(6.0), 2);
        assert_eq!(score_for(8.0), 3);
        assert_eq!(score_for(10.0), 4);
    }

    #[test]
    fn a_common_password_scores_at_the_bottom() {
        let strength = estimate("password1", &[]);
        assert!(strength.score <= 1, "{strength:?}");
        assert!(strength.warning.is_some());
    }

    #[test]
    fn a_random_password_scores_at_the_top() {
        let strength = estimate("kT7#vQ2!pZ9@wM4$", &[]);
        assert_eq!(strength.score, Strength::MAX_SCORE, "{strength:?}");
    }

    #[test]
    fn the_context_counts_against_a_password_built_from_it() {
        let alone = estimate("quillfeather1987", &[]);
        let with_context = estimate("quillfeather1987", &["quillfeather"]);
        assert!(with_context.guesses_log10 < alone.guesses_log10);
    }
}
