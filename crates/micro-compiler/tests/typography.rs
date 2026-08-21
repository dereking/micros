use micro_compiler::compile_source;
use micro_ir::{FontFamily, FontWeight, TextSource, TextStyle, UiKind};

fn style() -> TextStyle {
    TextStyle::new(FontFamily::UiSans, 18, FontWeight::Regular, 24).unwrap()
}

#[test]
fn lowers_exact_text_and_button_styles() {
    let source = r#"
ui.mount(ui.column([
  ui.text("Welcome", { font: "uiSans", size: 18, weight: "regular", lineHeight: 24 }),
  ui.button("Confirm", {
    onClick: () => {},
    textStyle: { font: "uiSans", size: 18, weight: "regular", lineHeight: 24 },
  }),
]));
"#;

    let image = compile_source("styles.ts", source).unwrap();
    let text = image
        .nodes
        .iter()
        .find(|node| node.kind == UiKind::Text)
        .unwrap();
    let button = image
        .nodes
        .iter()
        .find(|node| node.kind == UiKind::Button)
        .unwrap();

    assert_eq!(text.text_style, Some(style()));
    assert_eq!(button.text_style, Some(style()));
}

#[test]
fn omitting_text_style_preserves_none() {
    let image = compile_source(
        "unstyled.ts",
        r#"ui.mount(ui.button("Confirm", { onClick: () => {} }));"#,
    )
    .unwrap();

    assert_eq!(image.nodes[0].text_style, None);
}

#[test]
fn rejects_invalid_style_literals_with_stable_diagnostic() {
    let cases = [
        (
            "font.ts",
            r#"ui.mount(ui.text("A", { font: "serif", size: 18, weight: "regular", lineHeight: 24 }));"#,
        ),
        (
            "size.ts",
            r#"ui.mount(ui.text("A", { font: "uiSans", size: 16, weight: "regular", lineHeight: 24 }));"#,
        ),
        (
            "height.ts",
            r#"ui.mount(ui.text("A", { font: "uiSans", size: 18, weight: "regular", lineHeight: 17 }));"#,
        ),
        (
            "extra.ts",
            r#"ui.mount(ui.text("A", { font: "uiSans", size: 18, weight: "regular", lineHeight: 24, color: "red" }));"#,
        ),
        (
            "missing.ts",
            r#"ui.mount(ui.text("A", { font: "uiSans", size: 18, weight: "regular" }));"#,
        ),
        (
            "nonliteral.ts",
            r#"const size = state(18); ui.mount(ui.text("A", { font: "uiSans", size: size.value, weight: "regular", lineHeight: 24 }));"#,
        ),
    ];

    for (path, source) in cases {
        let errors = compile_source(path, source).unwrap_err();
        assert_eq!(errors[0].code, "MTS014", "{path}: {errors:?}");
    }
}

#[test]
fn rejects_unknown_top_level_button_option() {
    let errors = compile_source(
        "button-option.ts",
        r#"ui.mount(ui.button("Confirm", { onClick: () => {}, color: "red" }));"#,
    )
    .unwrap_err();

    assert_eq!(errors[0].code, "MTS002");
}

#[test]
fn rejects_weights_without_generated_assets() {
    for weight in ["medium", "bold"] {
        let source = format!(
            r#"ui.mount(ui.text("A", {{ font: "uiSans", size: 18, weight: "{weight}", lineHeight: 24 }}));"#
        );
        let errors = compile_source("weight.ts", &source).unwrap_err();

        assert_eq!(errors[0].code, "MTS014");
        assert_eq!(
            errors[0].message,
            format!("font weight `{weight}` has no generated uiSans asset; use `regular`")
        );
    }
}

#[test]
fn accepts_ascii_common_chinese_and_dynamic_bindings() {
    let image = compile_source(
        "glyphs.ts",
        r#"
const value = state("未静态检查");
ui.mount(ui.column([
  ui.text("ASCII ~ 123"),
  ui.text("欢迎"),
  ui.button("确认", { onClick: () => {} }),
  ui.text("龚"),
  ui.text(bind(() => value.value)),
]));
"#,
    )
    .unwrap();

    assert!(
        image
            .nodes
            .iter()
            .any(|node| { matches!(node.text, Some(TextSource::Binding(_))) })
    );
}

#[test]
fn rejects_unlisted_literal_glyph_with_stable_diagnostic() {
    let errors = compile_source("glyph.ts", r#"ui.mount(ui.text("龘"));"#).unwrap_err();

    assert_eq!(errors[0].code, "MTS015");
}
