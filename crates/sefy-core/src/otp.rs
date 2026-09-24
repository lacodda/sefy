//! One-time passwords (RFC 6238): the key a site hands over when two-factor
//! sign-in is turned on, and the short code it asks for afterwards.
//!
//! A key reaches sefy in one of two shapes. A QR code on the site's setup page
//! carries an `otpauth://` link, and that link is what an authenticator app
//! reads; the "can't scan it?" text beside the code is the bare key in base32.
//! Both are accepted, and both are kept the way they came: a link keeps its
//! issuer, account and parameters exactly, so drawing it again for a phone
//! shows what the site meant it to show; a bare key means the defaults every
//! site assumes when it hands one over.

use crate::error::{Error, Result};
use hmac::{Hmac, KeyInit, Mac};
use zeroize::Zeroizing;

/// The field a one-time password key lives in, in any record that has one.
///
/// A name rather than a flag on the field: what makes a value a key is what
/// the field is called, the same way `url` is what `sefy open` opens.
pub const FIELD: &str = "totp";

/// What every site means by a bare key: HMAC-SHA1, six digits, thirty seconds.
const DEFAULT_ALGORITHM: Algorithm = Algorithm::Sha1;
const DEFAULT_DIGITS: u32 = 6;
const DEFAULT_PERIOD: u64 = 30;

const SCHEME: &str = "otpauth://";

/// The hash behind the codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Algorithm {
    /// HMAC-SHA1: what nearly every site uses, and the default.
    Sha1,
    /// HMAC-SHA256.
    Sha256,
    /// HMAC-SHA512.
    Sha512,
}

impl Algorithm {
    /// The name an `otpauth://` link spells it with.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sha1 => "SHA1",
            Self::Sha256 => "SHA256",
            Self::Sha512 => "SHA512",
        }
    }

    fn parse(value: &str) -> Result<Self> {
        match value.to_ascii_uppercase().as_str() {
            "SHA1" => Ok(Self::Sha1),
            "SHA256" => Ok(Self::Sha256),
            "SHA512" => Ok(Self::Sha512),
            _ => Err(invalid("the algorithm must be SHA1, SHA256 or SHA512")),
        }
    }
}

/// A one-time password key and the parameters its codes are made with.
#[derive(Clone)]
pub struct Totp {
    key: Zeroizing<Vec<u8>>,
    /// The hash behind the codes.
    pub algorithm: Algorithm,
    /// How many digits a code has.
    pub digits: u32,
    /// How many seconds a code lives.
    pub period: u64,
    /// Who issued the key, when the link said.
    pub issuer: Option<String>,
    /// Which account at the issuer, when the link said.
    pub account: Option<String>,
}

// Written out so the key never reaches a log line or a panic message.
impl std::fmt::Debug for Totp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Totp")
            .field("key", &"<hidden>")
            .field("algorithm", &self.algorithm)
            .field("digits", &self.digits)
            .field("period", &self.period)
            .field("issuer", &self.issuer)
            .field("account", &self.account)
            .finish()
    }
}

impl Totp {
    /// Reads a key in either shape: an `otpauth://totp/` link or bare base32.
    pub fn parse(text: &str) -> Result<Self> {
        let text = text.trim();
        if has_scheme(text) {
            parse_link(&text[SCHEME.len()..])
        } else {
            Ok(Self {
                key: decode_base32(text)?,
                algorithm: DEFAULT_ALGORITHM,
                digits: DEFAULT_DIGITS,
                period: DEFAULT_PERIOD,
                issuer: None,
                account: None,
            })
        }
    }

    /// The code valid at `unix` seconds since the epoch.
    pub fn code_at(&self, unix: u64) -> String {
        let counter = (unix / self.period).to_be_bytes();
        let digest = match self.algorithm {
            Algorithm::Sha1 => mac::<sha1::Sha1>(&self.key, &counter),
            Algorithm::Sha256 => mac::<sha2::Sha256>(&self.key, &counter),
            Algorithm::Sha512 => mac::<sha2::Sha512>(&self.key, &counter),
        };
        // Dynamic truncation, RFC 4226 section 5.3: the low nibble of the last
        // byte picks four bytes, and the top bit is dropped so the result reads
        // the same as a signed and an unsigned number.
        let offset = usize::from(digest[digest.len() - 1] & 0x0f);
        let binary = u32::from_be_bytes([
            digest[offset] & 0x7f,
            digest[offset + 1],
            digest[offset + 2],
            digest[offset + 3],
        ]);
        let code = u64::from(binary) % 10u64.pow(self.digits);
        format!("{code:0width$}", width = self.digits as usize)
    }

    /// Seconds the code valid at `unix` has left.
    pub fn remaining(&self, unix: u64) -> u64 {
        self.period - unix % self.period
    }

    /// The key as an `otpauth://` link, for an authenticator app to read.
    ///
    /// The issuer and account the key came with win; the fallbacks name it
    /// when it came bare, so the phone shows something recognisable rather
    /// than an empty line.
    pub fn link(&self, issuer: &str, account: Option<&str>) -> String {
        let issuer = self.issuer.as_deref().unwrap_or(issuer);
        let account = self.account.as_deref().or(account).unwrap_or(issuer);
        let mut link = format!(
            "{SCHEME}totp/{}:{}?secret={}&issuer={}",
            percent_encode(issuer),
            percent_encode(account),
            encode_base32(&self.key),
            percent_encode(issuer)
        );
        // Only what differs from the defaults: some authenticator apps refuse
        // a link that spells out parameters they do not support, even when
        // the value is the one they would have assumed.
        if self.algorithm != DEFAULT_ALGORITHM {
            link.push_str(&format!("&algorithm={}", self.algorithm.as_str()));
        }
        if self.digits != DEFAULT_DIGITS {
            link.push_str(&format!("&digits={}", self.digits));
        }
        if self.period != DEFAULT_PERIOD {
            link.push_str(&format!("&period={}", self.period));
        }
        link
    }
}

/// Checks a key and returns what should be stored for it.
///
/// A link is stored as given, since its issuer and account are the site's own
/// words; a bare key loses the spaces and dashes sites print it with, and is
/// upper-cased, so the same key is always stored the same way.
pub fn normalize(text: &str) -> Result<String> {
    let text = text.trim();
    Totp::parse(text)?;
    if has_scheme(text) {
        Ok(text.to_owned())
    } else {
        Ok(text
            .chars()
            .filter(|c| !is_filler(*c))
            .map(|c| c.to_ascii_uppercase())
            .collect())
    }
}

fn has_scheme(text: &str) -> bool {
    text.get(..SCHEME.len())
        .is_some_and(|start| start.eq_ignore_ascii_case(SCHEME))
}

fn mac<D>(key: &[u8], message: &[u8]) -> Vec<u8>
where
    D: hmac::EagerHash,
    Hmac<D>: KeyInit + Mac,
{
    let mut mac =
        <Hmac<D> as KeyInit>::new_from_slice(key).expect("HMAC takes a key of any length");
    mac.update(message);
    mac.finalize().into_bytes().to_vec()
}

/// Reads what follows `otpauth://`.
fn parse_link(rest: &str) -> Result<Totp> {
    let (kind, rest) = rest
        .split_once('/')
        .ok_or_else(|| invalid("the link has no type"))?;
    match kind.to_ascii_lowercase().as_str() {
        "totp" => {}
        "hotp" => {
            return Err(invalid(
                "this is a counter-based (HOTP) key; sefy makes time-based codes only",
            ));
        }
        _ => return Err(invalid("the link is neither totp nor hotp")),
    }

    let (label, query) = rest.split_once('?').unwrap_or((rest, ""));
    let label = percent_decode(label)?;
    let (mut issuer, account) = match label.split_once(':') {
        Some((issuer, account)) => (non_empty(issuer), non_empty(account)),
        None => (None, non_empty(&label)),
    };

    let mut key = None;
    let mut algorithm = DEFAULT_ALGORITHM;
    let mut digits = DEFAULT_DIGITS;
    let mut period = DEFAULT_PERIOD;
    for pair in query.split('&').filter(|pair| !pair.is_empty()) {
        let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
        let value = percent_decode(&value.replace('+', " "))?;
        match name.to_ascii_lowercase().as_str() {
            "secret" => key = Some(decode_base32(&value)?),
            // The parameter is the one the specification recommends relying
            // on; the label prefix is older and often absent or abbreviated.
            "issuer" => issuer = non_empty(&value).or(issuer),
            "algorithm" => algorithm = Algorithm::parse(&value)?,
            "digits" => {
                digits = match value.parse() {
                    Ok(digits @ 6..=8) => digits,
                    _ => return Err(invalid("a code has 6 to 8 digits")),
                }
            }
            "period" => {
                period = match value.parse() {
                    Ok(period @ 1..) => period,
                    _ => return Err(invalid("the period is a whole number of seconds")),
                }
            }
            // `image`, `color` and whatever an issuer adds: not needed to make
            // a code, and not an error to carry.
            _ => {}
        }
    }

    Ok(Totp {
        key: key.ok_or_else(|| invalid("the link carries no secret"))?,
        algorithm,
        digits,
        period,
        issuer,
        account,
    })
}

fn non_empty(text: &str) -> Option<String> {
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

/// Characters sites print a key with for readability, which are not part of it.
fn is_filler(c: char) -> bool {
    c.is_whitespace() || c == '-' || c == '='
}

const BASE32: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

/// Decodes RFC 4648 base32, forgiving case, spaces, dashes and padding.
fn decode_base32(text: &str) -> Result<Zeroizing<Vec<u8>>> {
    let mut bytes = Zeroizing::new(Vec::with_capacity(text.len() * 5 / 8));
    let mut buffer: u64 = 0;
    let mut bits = 0;
    for c in text.chars().filter(|c| !is_filler(*c)) {
        let value = BASE32
            .iter()
            .position(|&symbol| char::from(symbol) == c.to_ascii_uppercase())
            .ok_or_else(|| invalid("the key is not base32 (letters A-Z and digits 2-7)"))?;
        buffer = (buffer << 5) | value as u64;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            bytes.push((buffer >> bits) as u8);
            buffer &= (1 << bits) - 1;
        }
    }
    if bytes.is_empty() {
        return Err(invalid("the key is empty"));
    }
    Ok(bytes)
}

fn encode_base32(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len().div_ceil(5) * 8);
    let mut buffer: u64 = 0;
    let mut bits = 0;
    for &byte in bytes {
        buffer = (buffer << 8) | u64::from(byte);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            text.push(char::from(BASE32[((buffer >> bits) & 31) as usize]));
        }
        buffer &= (1 << bits) - 1;
    }
    if bits > 0 {
        text.push(char::from(BASE32[((buffer << (5 - bits)) & 31) as usize]));
    }
    text
}

fn percent_encode(text: &str) -> String {
    let mut encoded = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~@".contains(&byte) {
            encoded.push(char::from(byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

fn percent_decode(text: &str) -> Result<String> {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let byte = text
                .get(index + 1..index + 3)
                .and_then(|hex| u8::from_str_radix(hex, 16).ok())
                .ok_or_else(|| invalid("the link has a broken %-escape"))?;
            decoded.push(byte);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).map_err(|_| invalid("the link is not UTF-8"))
}

fn invalid(reason: &'static str) -> Error {
    Error::InvalidOtpKey(reason)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 6238 appendix B: the reference keys, eight-digit codes, and the
    /// times the specification checks them at.
    const TIMES: [u64; 6] = [
        59,
        1_111_111_109,
        1_111_111_111,
        1_234_567_890,
        2_000_000_000,
        20_000_000_000,
    ];

    fn reference(key: &[u8], algorithm: Algorithm) -> Totp {
        Totp {
            key: Zeroizing::new(key.to_vec()),
            algorithm,
            digits: 8,
            period: 30,
            issuer: None,
            account: None,
        }
    }

    #[test]
    fn sha1_codes_match_the_rfc() {
        let totp = reference(b"12345678901234567890", Algorithm::Sha1);
        let codes: Vec<String> = TIMES.iter().map(|&t| totp.code_at(t)).collect();
        assert_eq!(
            codes,
            [
                "94287082", "07081804", "14050471", "89005924", "69279037", "65353130"
            ]
        );
    }

    #[test]
    fn sha256_codes_match_the_rfc() {
        let totp = reference(b"12345678901234567890123456789012", Algorithm::Sha256);
        let codes: Vec<String> = TIMES.iter().map(|&t| totp.code_at(t)).collect();
        assert_eq!(
            codes,
            [
                "46119246", "68084774", "67062674", "91819424", "90698825", "77737706"
            ]
        );
    }

    #[test]
    fn sha512_codes_match_the_rfc() {
        let key = b"1234567890123456789012345678901234567890123456789012345678901234";
        let totp = reference(key, Algorithm::Sha512);
        let codes: Vec<String> = TIMES.iter().map(|&t| totp.code_at(t)).collect();
        assert_eq!(
            codes,
            [
                "90693936", "25091201", "99943326", "93441116", "38618901", "47863826"
            ]
        );
    }

    #[test]
    fn a_bare_key_means_the_defaults() {
        // "12345678901234567890" in base32, spaced and lower-cased the way a
        // setup page prints it for typing.
        let totp = Totp::parse("gezd gnbv gy3t qojq gezd gnbv gy3t qojq").unwrap();
        assert_eq!(totp.algorithm, Algorithm::Sha1);
        assert_eq!(totp.digits, 6);
        assert_eq!(totp.period, 30);
        // The RFC's SHA-1 code at 59 seconds, cut to six digits.
        assert_eq!(totp.code_at(59), "287082");
    }

    #[test]
    fn a_link_carries_its_parameters_and_names() {
        let totp = Totp::parse(
            "otpauth://totp/ACME%20Co:jane@example.com?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ\
             &issuer=ACME%20Co&algorithm=SHA256&digits=8&period=60&image=x",
        )
        .unwrap();
        assert_eq!(totp.algorithm, Algorithm::Sha256);
        assert_eq!(totp.digits, 8);
        assert_eq!(totp.period, 60);
        assert_eq!(totp.issuer.as_deref(), Some("ACME Co"));
        assert_eq!(totp.account.as_deref(), Some("jane@example.com"));
    }

    #[test]
    fn the_issuer_parameter_wins_over_the_label() {
        let totp = Totp::parse("otpauth://totp/Old:jane?secret=GEZDGNBV&issuer=New").unwrap();
        assert_eq!(totp.issuer.as_deref(), Some("New"));
    }

    #[test]
    fn a_link_drawn_again_reads_back_the_same() {
        let original = Totp::parse(
            "otpauth://totp/Example:alice@google.com?secret=JBSWY3DPEHPK3PXP&issuer=Example\
             &digits=8&period=45&algorithm=SHA512",
        )
        .unwrap();
        let again = Totp::parse(&original.link("ignored", Some("ignored"))).unwrap();
        assert_eq!(*again.key, *original.key);
        assert_eq!(again.algorithm, original.algorithm);
        assert_eq!(again.digits, original.digits);
        assert_eq!(again.period, original.period);
        assert_eq!(again.issuer, original.issuer);
        assert_eq!(again.account, original.account);
    }

    #[test]
    fn a_bare_key_is_named_by_the_fallbacks_and_keeps_the_defaults_implicit() {
        let totp = Totp::parse("JBSWY3DPEHPK3PXP").unwrap();
        assert_eq!(
            totp.link("forum & co", Some("jane")),
            "otpauth://totp/forum%20%26%20co:jane?secret=JBSWY3DPEHPK3PXP&issuer=forum%20%26%20co"
        );
    }

    #[test]
    fn base32_round_trips_every_length() {
        for length in 1..=21 {
            let bytes: Vec<u8> = (0..length).map(|n| (n * 37 + 11) as u8).collect();
            let text = encode_base32(&bytes);
            assert_eq!(*decode_base32(&text).unwrap(), bytes, "length {length}");
        }
        assert_eq!(
            *decode_base32("JBSWY3DPEHPK3PXP").unwrap(),
            b"Hello!\xde\xad\xbe\xef"
        );
    }

    #[test]
    fn what_is_not_a_key_is_refused() {
        for bad in [
            "",
            "   ",
            "not base32 at all!",
            "JBSWY3DP1",
            "otpauth://hotp/x?secret=JBSWY3DP&counter=1",
            "otpauth://totp/x?issuer=nobody",
            "otpauth://totp/x?secret=JBSWY3DP&digits=5",
            "otpauth://totp/x?secret=JBSWY3DP&digits=9",
            "otpauth://totp/x?secret=JBSWY3DP&period=0",
            "otpauth://totp/x?secret=JBSWY3DP&algorithm=MD5",
            "otpauth://totp/%zz?secret=JBSWY3DP",
            "https://example.com/?secret=JBSWY3DP",
        ] {
            assert!(Totp::parse(bad).is_err(), "{bad:?} was accepted");
        }
    }

    #[test]
    fn the_refusal_never_quotes_the_key() {
        let error = Totp::parse("otpauth://totp/x?secret=SECRETVALUE1&digits=5").unwrap_err();
        assert!(!error.to_string().contains("SECRETVALUE"));
    }

    #[test]
    fn debug_output_hides_the_key() {
        let totp = Totp::parse("JBSWY3DPEHPK3PXP").unwrap();
        let printed = format!("{totp:?}");
        assert!(printed.contains("<hidden>"));
        assert!(!printed.contains("[72"), "{printed}");
    }

    #[test]
    fn a_bare_key_is_stored_one_way_and_a_link_as_given() {
        assert_eq!(
            normalize(" jbsw y3dp-ehpk 3pxp== ").unwrap(),
            "JBSWY3DPEHPK3PXP"
        );
        let link = "otpauth://totp/Example:alice?secret=JBSWY3DPEHPK3PXP&issuer=Example";
        assert_eq!(normalize(&format!("  {link}\n")).unwrap(), link);
        assert!(normalize("hunter2!").is_err());
    }

    #[test]
    fn the_code_counts_down_to_the_next_window() {
        let totp = Totp::parse("JBSWY3DPEHPK3PXP").unwrap();
        assert_eq!(totp.remaining(60), 30);
        assert_eq!(totp.remaining(89), 1);
        assert_ne!(totp.code_at(89), totp.code_at(90));
        assert_eq!(totp.code_at(60), totp.code_at(89));
    }
}
