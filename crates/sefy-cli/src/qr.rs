//! Drawing an `otpauth://` link as a QR code, for a phone to scan.
//!
//! The picture is the key itself, as readable as the key in text. It is drawn
//! only on a terminal, and taken off the screen and out of the scrollback as
//! soon as the phone has read it.

use anyhow::{Result, anyhow, bail};
use qrcodegen::{QrCode, QrCodeEcc};
use std::io::{IsTerminal, Write};

/// Light modules around the code: the specification asks for four, and a
/// phone reading a terminal needs every one of them against a dark theme.
const QUIET_ZONE: i32 = 4;

/// Draws `link`, waits for Enter, and wipes the screen.
pub fn show(link: &str) -> Result<()> {
    if !std::io::stdout().is_terminal() || !std::io::stdin().is_terminal() {
        bail!(
            "the QR code carries the key itself, so sefy draws it only on a terminal\n\
             and clears it again once it has been scanned"
        );
    }
    // Asking is what switches a Windows console into understanding colour
    // escapes; the colours are not decoration here, they fix which modules
    // are dark whatever the terminal's theme.
    if !console::Term::stdout().features().colors_supported() {
        bail!(
            "this terminal cannot draw the QR code in colour (is NO_COLOR set?)\n\
             copy the key by hand instead with: sefy get <REFERENCE> --field {}",
            sefy_core::otp::FIELD
        );
    }

    let code = QrCode::encode_text(link, QrCodeEcc::Medium)
        .map_err(|_| anyhow!("the key is too long to fit in a QR code"))?;

    let mut out = std::io::stdout().lock();
    out.write_all(render(&code).as_bytes())?;
    write!(
        out,
        "\nscan it with the authenticator app, then press Enter to clear the screen "
    )?;
    out.flush()?;
    drop(out);

    let mut line = String::new();
    let read = std::io::stdin().read_line(&mut line);

    // Cleared however the wait ended: the picture must not outlive the reason
    // it was drawn. `3J` drops the scrollback too, where a terminal keeps it.
    print!("\x1b[2J\x1b[3J\x1b[H");
    std::io::stdout().flush()?;
    read?;
    println!("cleared");
    Ok(())
}

/// The code as rows of half blocks, two modules to a character cell.
///
/// Each cell is an upper half block whose foreground is the top module and
/// whose background is the bottom one, so the drawing is square and keeps its
/// polarity on a light and a dark terminal alike.
fn render(code: &QrCode) -> String {
    const DARK_FG: &str = "30";
    const LIGHT_FG: &str = "97";
    const DARK_BG: &str = "40";
    const LIGHT_BG: &str = "107";

    let size = code.size();
    let mut text = String::new();
    let mut y = -QUIET_ZONE;
    while y < size + QUIET_ZONE {
        for x in -QUIET_ZONE..size + QUIET_ZONE {
            // Outside the symbol `get_module` answers light, which is exactly
            // the quiet zone.
            let top = code.get_module(x, y);
            let bottom = code.get_module(x, y + 1);
            let fg = if top { DARK_FG } else { LIGHT_FG };
            let bg = if bottom { DARK_BG } else { LIGHT_BG };
            text.push_str(&format!("\x1b[{fg};{bg}m\u{2580}"));
        }
        text.push_str("\x1b[0m\n");
        y += 2;
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_drawing_is_the_symbol_with_its_quiet_zone() {
        let code =
            QrCode::encode_text("otpauth://totp/x?secret=JBSWY3DP", QrCodeEcc::Medium).unwrap();
        let drawing = render(&code);
        let width = (code.size() + 2 * QUIET_ZONE) as usize;
        let rows: Vec<&str> = drawing.lines().collect();
        assert_eq!(rows.len(), width.div_ceil(2));
        for row in &rows {
            assert_eq!(row.matches('\u{2580}').count(), width);
        }
        // The first row is all quiet zone: light over light.
        assert!(!rows[0].contains("\x1b[30"));
        assert!(!rows[0].contains(";40m"));
        // And somewhere below it a finder pattern starts dark.
        assert!(drawing.contains("\x1b[30;40m"));
    }
}
