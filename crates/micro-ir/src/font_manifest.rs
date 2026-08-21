use std::borrow::Cow;

pub const REPLACEMENT_GLYPH: char = '\u{fffd}';

const MANIFEST: &str = include_str!("../../../assets/fonts/ui-sans-common.txt");

fn supported(glyph: char) -> bool {
    matches!(glyph, '\n' | '\r' | '\t') || MANIFEST.lines().skip(3).any(|line| line.contains(glyph))
}

/// Replaces glyphs absent from every bundled UI font with U+FFFD.
#[must_use]
pub fn sanitize_ui_text(text: &str) -> (Cow<'_, str>, bool) {
    if text.chars().all(supported) {
        return (Cow::Borrowed(text), false);
    }
    (
        Cow::Owned(
            text.chars()
                .map(|glyph| {
                    if supported(glyph) {
                        glyph
                    } else {
                        REPLACEMENT_GLYPH
                    }
                })
                .collect(),
        ),
        true,
    )
}
