use micro_ir::{REPLACEMENT_GLYPH, sanitize_ui_text};

#[test]
fn replaces_unsupported_glyphs_but_preserves_layout_controls() {
    assert_eq!(sanitize_ui_text("ASCII\n一🦄"), ("ASCII\n一�".into(), true));
    assert_eq!(sanitize_ui_text("欢迎�"), ("欢迎�".into(), false));
    assert_eq!(REPLACEMENT_GLYPH, '\u{fffd}');
}

#[test]
fn checked_in_manifest_contains_the_replacement_glyph() {
    let manifest = include_str!("../../../assets/fonts/ui-sans-common.txt");
    assert!(manifest.contains(REPLACEMENT_GLYPH));
}
