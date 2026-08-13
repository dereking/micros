use micro_compiler::validate_source;

#[test]
fn accepts_the_counter_sdk_surface() {
    let source = include_str!("../../../apps/counter/app.ts");
    validate_source("app.ts", source).unwrap();
}

#[test]
fn rejects_unsupported_constructs_with_locations() {
    let cases = [
        ("class.ts", "class App {}", "MTS001", 1, 1),
        ("function.ts", "function run() {}", "MTS001", 1, 1),
        ("async.ts", "const run = async () => 1;", "MTS001", 1, 13),
        ("import.ts", "import value from 'pkg';", "MTS001", 1, 1),
        ("jsx.ts", "const view = <Button />;", "MTS000", 1, 23),
        ("spread.ts", "const x = [...items];", "MTS001", 1, 12),
        ("destructure.ts", "const { value } = item;", "MTS001", 1, 7),
        (
            "dynamic.ts",
            "const x = state(0); x['value'];",
            "MTS001",
            1,
            21,
        ),
        ("global.ts", "console.log('no');", "MTS001", 1, 1),
    ];

    for (path, source, code, line, column) in cases {
        let errors = validate_source(path, source).unwrap_err();
        assert_eq!(errors[0].code, code, "{path}: {errors:?}");
        assert_eq!((errors[0].line, errors[0].column), (line, column), "{path}");
        assert!(
            errors[0]
                .to_string()
                .starts_with(&format!("{path}:{line}:{column}"))
        );
    }
}

#[test]
fn rejects_unknown_button_props_and_multiple_mounts() {
    let unknown = r#"ui.mount(ui.button("Add", { onTap: () => 1 }));"#;
    let errors = validate_source("prop.ts", unknown).unwrap_err();
    assert_eq!(errors[0].code, "MTS002");

    let mounts = "ui.mount(ui.text('one'));\nui.mount(ui.text('two'));";
    let errors = validate_source("mount.ts", mounts).unwrap_err();
    assert_eq!(errors[0].code, "MTS003");
    assert_eq!((errors[0].line, errors[0].column), (2, 1));
}
