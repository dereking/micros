use micro_board_profile::{BoardProfile, DriverCatalog, ProfileError};
use serde_json::{Value, json};

const PRESET: &str = include_str!("../../../profiles/esp32s3/spotpear-touch-lcd-7.json");

fn catalog() -> DriverCatalog {
    DriverCatalog::esp32s3_v1()
}

fn parsed_preset() -> BoardProfile {
    BoardProfile::from_json(PRESET).expect("preset should parse")
}

fn preset_value() -> Value {
    serde_json::from_str(PRESET).expect("preset fixture should be JSON")
}

fn parse_and_validate(value: Value) -> Result<(), ProfileError> {
    BoardProfile::from_json(&value.to_string())?.validate(&catalog())
}

fn set(value: &mut Value, pointer: &str, replacement: Value) {
    *value.pointer_mut(pointer).expect("fixture pointer exists") = replacement;
}

fn validation_message(value: Value) -> String {
    parse_and_validate(value)
        .expect_err("mutated profile should be rejected")
        .to_string()
}

#[test]
fn checked_in_spotpear_profile_parses_and_validates() {
    let profile = parsed_preset();
    profile
        .validate(&catalog())
        .expect("preset should validate");

    assert_eq!(profile.schema_version, 1);
    assert_eq!(profile.id, "spotpear-esp32s3-touch-lcd-7-v1.2-n8r8");
    assert_eq!(profile.name, "Spotpear ESP32-S3 Touch LCD 7 V1.2");
    assert_eq!(profile.chip_family, "esp32s3");
    assert_eq!(profile.board_revision.as_deref(), Some("1.2"));
    assert_eq!(profile.hardware.flash_mb, 8);
    assert_eq!(profile.hardware.psram_mb, 8);
    assert_eq!(profile.hardware.psram_mode, "octal");
    assert_eq!(profile.display.driver, "esp-lcd-rgb");
    assert_eq!((profile.display.width, profile.display.height), (800, 480));
    assert_eq!(profile.display.color_format, "rgb565");
    assert_eq!(profile.display.pixel_clock_hz, 16_000_000);
    assert!(profile.display.pclk_active_negative);
    assert_eq!(
        (
            profile.display.hsync,
            profile.display.vsync,
            profile.display.de,
            profile.display.pclk,
        ),
        (46, 3, 5, 7)
    );
    assert_eq!(
        profile.display.data,
        vec![14, 38, 18, 17, 10, 39, 0, 45, 48, 47, 21, 1, 2, 42, 41, 40]
    );
    assert_eq!(
        (
            profile.display.timing.h_pulse,
            profile.display.timing.h_back,
            profile.display.timing.h_front,
            profile.display.timing.v_pulse,
            profile.display.timing.v_back,
            profile.display.timing.v_front,
        ),
        (4, 8, 8, 4, 8, 8)
    );
    assert_eq!(profile.touch.driver, "gt911");
    assert_eq!(
        (profile.touch.bus, profile.touch.sda, profile.touch.scl),
        (0, 8, 9)
    );
    assert_eq!((profile.touch.irq, profile.touch.reset_expander), (4, 1));
    assert!(!profile.touch.swap_xy && !profile.touch.mirror_x && !profile.touch.mirror_y);
    assert_eq!(profile.expander.driver, "ch422g");
    assert_eq!(
        (
            profile.expander.bus,
            profile.expander.sda,
            profile.expander.scl,
            profile.expander.touch_reset,
            profile.expander.backlight_enable,
        ),
        (0, 8, 9, 1, 2)
    );
    assert_eq!(profile.backlight.kind, "binary");
    assert_eq!(profile.backlight.enable_expander, 2);
    assert_eq!(profile.resources.framebuffers, 2);
    assert_eq!(profile.resources.bounce_buffer_lines, 10);
    assert!(profile.resources.prefer_psram);
    assert_eq!(profile.driver_catalog, "esp32s3-v1");
}

#[test]
fn rejects_unknown_schema_version() {
    let mut value = preset_value();
    set(&mut value, "/schemaVersion", json!(2));
    assert!(validation_message(value).contains("schema version"));
}

#[test]
fn rejects_future_schema_before_deserializing_v1_fields() {
    let error = BoardProfile::from_json(r#"{"schemaVersion":2}"#).unwrap_err();
    assert!(matches!(error, ProfileError::Validation(_)));
    assert!(error.to_string().contains("unsupported schema version 2"));
}

#[test]
fn rejects_unknown_catalog_and_unavailable_drivers() {
    for (pointer, replacement, expected) in [
        ("/driverCatalog", json!("esp32s3-v2"), "driver catalog"),
        ("/display/driver", json!("unknown-rgb"), "display driver"),
        ("/touch/driver", json!("unknown-touch"), "touch driver"),
    ] {
        let mut value = preset_value();
        set(&mut value, pointer, replacement);
        assert!(validation_message(value).contains(expected));
    }
}

#[test]
fn rejects_unavailable_expander_driver() {
    let mut value = preset_value();
    set(&mut value, "/expander/driver", json!("unknown-expander"));
    assert!(validation_message(value).contains("expander driver"));
}

#[test]
fn rejects_incompatible_memory() {
    for (pointer, replacement, expected) in [
        ("/hardware/flashMb", json!(4), "flash"),
        ("/hardware/psramMb", json!(4), "PSRAM"),
        ("/hardware/psramMode", json!("quad"), "PSRAM mode"),
    ] {
        let mut value = preset_value();
        set(&mut value, pointer, replacement);
        assert!(validation_message(value).contains(expected));
    }
}

#[test]
fn rejects_invalid_excluded_and_out_of_range_pins() {
    for (pointer, replacement) in [
        ("/display/hsync", json!(-1)),
        ("/display/vsync", json!(26)),
        ("/touch/irq", json!(49)),
    ] {
        let mut value = preset_value();
        set(&mut value, pointer, replacement);
        assert!(validation_message(value).contains("GPIO"));
    }
}

#[test]
fn rejects_accidental_pin_duplicates() {
    for (pointer, replacement) in [
        ("/display/vsync", json!(46)),
        ("/display/data/1", json!(14)),
        ("/touch/irq", json!(8)),
        ("/touch/sda", json!(14)),
    ] {
        let mut value = preset_value();
        set(&mut value, pointer, replacement);
        assert!(validation_message(value).contains("GPIO"));
    }
}

#[test]
fn permits_only_consistent_i2c_pin_sharing() {
    let mut value = preset_value();
    set(&mut value, "/expander/bus", json!(1));
    assert!(validation_message(value).contains("I2C"));

    let mut value = preset_value();
    set(&mut value, "/expander/sda", json!(6));
    assert!(validation_message(value).contains("I2C"));

    let mut value = preset_value();
    set(&mut value, "/backlight/enableExpander", json!(3));
    assert!(validation_message(value).contains("backlight"));

    let mut value = preset_value();
    set(&mut value, "/touch/resetExpander", json!(3));
    assert!(validation_message(value).contains("touch reset"));
}

#[test]
fn rejects_unsupported_i2c_buses_and_expander_outputs() {
    for (pointer, replacement, expected) in [
        ("/touch/bus", json!(255), "I2C bus"),
        ("/expander/bus", json!(255), "I2C bus"),
        ("/touch/resetExpander", json!(254), "expander output"),
        ("/expander/touchReset", json!(254), "expander output"),
        ("/backlight/enableExpander", json!(255), "expander output"),
        ("/expander/backlightEnable", json!(255), "expander output"),
    ] {
        let mut value = preset_value();
        set(&mut value, pointer, replacement);
        assert!(validation_message(value).contains(expected));
    }

    let mut alternate_bus_and_output_edges = preset_value();
    set(&mut alternate_bus_and_output_edges, "/touch/bus", json!(1));
    set(
        &mut alternate_bus_and_output_edges,
        "/expander/bus",
        json!(1),
    );
    set(
        &mut alternate_bus_and_output_edges,
        "/touch/resetExpander",
        json!(5),
    );
    set(
        &mut alternate_bus_and_output_edges,
        "/expander/touchReset",
        json!(5),
    );
    set(
        &mut alternate_bus_and_output_edges,
        "/backlight/enableExpander",
        json!(0),
    );
    set(
        &mut alternate_bus_and_output_edges,
        "/expander/backlightEnable",
        json!(0),
    );
    parse_and_validate(alternate_bus_and_output_edges)
        .expect("catalog should allow I2C bus 1 and CH422G outputs 0 through 5");
}

#[test]
fn rejects_invalid_display_geometry_timing_and_format() {
    for (pointer, replacement, expected) in [
        ("/display/width", json!(0), "width"),
        ("/display/width", json!(801), "width"),
        ("/display/height", json!(0), "height"),
        ("/display/height", json!(481), "height"),
        ("/display/timing/hPulse", json!(0), "timing"),
        ("/display/timing/vFront", json!(0), "timing"),
        ("/display/pixelClockHz", json!(0), "pixel clock"),
        ("/display/pixelClockHz", json!(21_000_001), "pixel clock"),
        ("/display/colorFormat", json!("rgb888"), "color format"),
    ] {
        let mut value = preset_value();
        set(&mut value, pointer, replacement);
        assert!(validation_message(value).contains(expected));
    }
}

#[test]
fn rejects_invalid_resource_policy() {
    for (pointer, replacement, expected) in [
        ("/resources/framebuffers", json!(0), "framebuffers"),
        ("/resources/framebuffers", json!(3), "framebuffers"),
        ("/resources/bounceBufferLines", json!(0), "bounce buffer"),
        ("/resources/bounceBufferLines", json!(81), "bounce buffer"),
        ("/resources/preferPsram", json!(false), "PSRAM"),
    ] {
        let mut value = preset_value();
        set(&mut value, pointer, replacement);
        assert!(validation_message(value).contains(expected));
    }
}

#[test]
fn calculates_framebuffer_memory_policy() {
    let mut nearly_full_size = preset_value();
    set(&mut nearly_full_size, "/display/width", json!(799));
    set(
        &mut nearly_full_size,
        "/resources/preferPsram",
        json!(false),
    );
    assert!(validation_message(nearly_full_size).contains("PSRAM"));

    let mut small = preset_value();
    set(&mut small, "/display/width", json!(320));
    set(&mut small, "/display/height", json!(240));
    set(&mut small, "/resources/framebuffers", json!(1));
    set(&mut small, "/resources/preferPsram", json!(false));
    parse_and_validate(small).expect("a small framebuffer may use internal memory");

    let mut exceeds_declared_psram = preset_value();
    set(&mut exceeds_declared_psram, "/hardware/psramMb", json!(1));
    assert!(validation_message(exceeds_declared_psram).contains("exceeds declared PSRAM"));
}

#[test]
fn accounts_for_two_internal_bounce_buffers() {
    let mut combined_internal_overflow = preset_value();
    set(
        &mut combined_internal_overflow,
        "/display/width",
        json!(340),
    );
    set(
        &mut combined_internal_overflow,
        "/display/height",
        json!(380),
    );
    set(
        &mut combined_internal_overflow,
        "/resources/framebuffers",
        json!(1),
    );
    set(
        &mut combined_internal_overflow,
        "/resources/bounceBufferLines",
        json!(80),
    );
    set(
        &mut combined_internal_overflow,
        "/resources/preferPsram",
        json!(false),
    );
    let message = validation_message(combined_internal_overflow);
    assert!(message.contains("367200"));
    assert!(message.contains("internal RAM"));

    let mut boundary_with_psram = preset_value();
    set(
        &mut boundary_with_psram,
        "/resources/bounceBufferLines",
        json!(80),
    );
    parse_and_validate(boundary_with_psram)
        .expect("two 800x80 bounce buffers use 256000 internal bytes");

    let mut boundary_without_psram = preset_value();
    set(
        &mut boundary_without_psram,
        "/resources/bounceBufferLines",
        json!(80),
    );
    set(
        &mut boundary_without_psram,
        "/resources/preferPsram",
        json!(false),
    );
    assert!(validation_message(boundary_without_psram).contains("internal RAM"));
}

#[test]
fn parse_and_semantic_errors_are_distinct_and_clear() {
    let parse = BoardProfile::from_json("{ definitely not json }").unwrap_err();
    assert!(matches!(parse, ProfileError::Parse(_)));
    assert!(
        parse
            .to_string()
            .starts_with("board profile JSON parse error:")
    );

    let mut value = preset_value();
    set(&mut value, "/schemaVersion", json!(99));
    let semantic = BoardProfile::from_json(&value.to_string()).unwrap_err();
    assert!(matches!(semantic, ProfileError::Validation(_)));
    assert_eq!(
        semantic.to_string(),
        "board profile validation error: unsupported schema version 99; expected 1"
    );
}

#[test]
fn arbitrary_json_never_panics() {
    let mut corpus = vec![
        "null".to_owned(),
        "[]".to_owned(),
        "true".to_owned(),
        "0".to_owned(),
        r#""Unicode: 上海 🖥️""#.to_owned(),
        "{}".to_owned(),
        r#"{"schemaVersion":"one","display":[],"touch":false}"#.to_owned(),
        r#"{"schemaVersion":18446744073709551616000000000000000000}"#.to_owned(),
        r#"{"unknown":{"nested":[null,true,-1,1e400,"\u0000"]}}"#.to_owned(),
    ];

    let mut deep = json!("leaf");
    for level in 0..64 {
        deep = json!({ "level": level, "next": deep });
    }
    corpus.push(deep.to_string());

    for seed in 0..32 {
        corpus.push(
            json!({
                "schemaVersion": seed,
                "id": [seed, seed * 2],
                "hardware": { "flashMb": seed * 1_000_000 },
                "unknown": { "unicode": format!("board-{seed}-☃") }
            })
            .to_string(),
        );
    }

    let mut preset_with_unknown_fields = preset_value();
    preset_with_unknown_fields["unknownField"] = json!({"unicode": "硬件配置"});
    corpus.push(preset_with_unknown_fields.to_string());

    for input in corpus {
        let outcome = std::panic::catch_unwind(|| {
            if let Ok(profile) = BoardProfile::from_json(&input) {
                let _ = profile.validate(&catalog());
            }
        });
        assert!(outcome.is_ok(), "profile handling panicked for {input:?}");
    }
}
