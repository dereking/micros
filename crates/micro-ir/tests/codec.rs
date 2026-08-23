use micro_ir::{
    AppImage, BindingId, Constant, DecodeError, FontFamily, FontWeight, Function, FunctionId,
    FunctionKind, Instruction, NodeId, ScalarType, StateDecl, StateId, TextSource, TextStyle,
    TextStyleError, UiKind, UiNodeSpec, ValueSource, decode, encode, validate,
};

fn fixture() -> AppImage {
    AppImage {
        constants: vec![Constant::Number(0.0), Constant::String("Count: ".into())],
        states: vec![StateDecl {
            ty: ScalarType::Number,
            initial: 0,
        }],
        functions: vec![Function {
            kind: FunctionKind::Binding(BindingId(0)),
            arg_count: 0,
            locals: 0,
            max_stack: 2,
            code: vec![
                Instruction::Const(1),
                Instruction::LoadState(StateId(0)),
                Instruction::ToString,
                Instruction::Concat,
                Instruction::Return,
            ],
        }],
        nodes: vec![UiNodeSpec {
            id: NodeId(0),
            kind: UiKind::Text,
            children: vec![],
            text: Some(TextSource::Binding(FunctionId(0))),
            value: None,
            on_click: None,
            text_style: None,
            range: None,
        }],
        root: NodeId(0),
    }
}

#[test]
fn round_trips_a_valid_image() {
    let image = fixture();
    assert_eq!(decode(&encode(&image).unwrap()).unwrap(), image);
}

#[test]
fn round_trips_an_exact_text_style() {
    let mut image = fixture();
    image.nodes[0].text_style =
        Some(TextStyle::new(FontFamily::UiSans, 18, FontWeight::Regular, 24).unwrap());

    let decoded = decode(&encode(&image).unwrap()).unwrap();

    assert_eq!(decoded.nodes[0].text_style, image.nodes[0].text_style);
    assert_eq!(decoded, image);
}

#[test]
fn round_trips_progress_and_switch_values() {
    let image = AppImage {
        constants: vec![
            Constant::Number(0.5),
            Constant::Bool(true),
            Constant::String("on".into()),
        ],
        states: vec![],
        functions: vec![
            Function {
                kind: FunctionKind::Binding(BindingId(0)),
            arg_count: 0,
                locals: 0,
                max_stack: 2,
                code: vec![Instruction::Const(0), Instruction::Return],
            },
            Function {
                kind: FunctionKind::Handler(micro_ir::HandlerId(0)),
            arg_count: 0,
                locals: 0,
                max_stack: 1,
                code: vec![Instruction::Return],
            },
        ],
        nodes: vec![
            UiNodeSpec {
                id: NodeId(0),
                kind: UiKind::Column,
                children: vec![NodeId(1), NodeId(2)],
                text: None,
                value: None,
                on_click: None,
                text_style: None,
                range: None,
            },
            UiNodeSpec {
                id: NodeId(1),
                kind: UiKind::Progress,
                children: vec![],
                text: None,
                value: Some(ValueSource::Binding(FunctionId(0))),
                on_click: None,
                text_style: None,
                range: None,
            },
            UiNodeSpec {
                id: NodeId(2),
                kind: UiKind::Switch,
                children: vec![],
                text: None,
                value: Some(ValueSource::Constant(1)),
                on_click: Some(FunctionId(1)),
                text_style: None,
                range: None,
            },
        ],
        root: NodeId(0),
    };
    let decoded = decode(&encode(&image).unwrap()).unwrap();
    assert_eq!(decoded, image);
    assert_eq!(decoded.nodes[1].kind, UiKind::Progress);
    assert_eq!(decoded.nodes[1].value, Some(ValueSource::Binding(FunctionId(0))));
    assert_eq!(decoded.nodes[2].kind, UiKind::Switch);
    assert_eq!(decoded.nodes[2].value, Some(ValueSource::Constant(1)));
}

#[test]
fn text_style_rejects_unsupported_sizes() {
    assert_eq!(
        TextStyle::ui_sans(16, FontWeight::Regular, 20),
        Err(TextStyleError::UnsupportedSize(16))
    );
}

#[test]
fn text_style_accepts_only_generated_size_and_line_height_pairs() {
    for (size_px, line_height_px) in [(12, 14), (14, 18), (18, 24), (24, 32), (32, 40)] {
        assert!(TextStyle::ui_sans(size_px, FontWeight::Regular, line_height_px).is_ok());
    }
    for (size_px, line_height_px) in [(12, 12), (14, 14), (18, 18), (24, 24), (32, 32), (18, 25)] {
        assert_eq!(
            TextStyle::ui_sans(size_px, FontWeight::Regular, line_height_px),
            Err(TextStyleError::UnsupportedLineHeight {
                size_px,
                line_height_px,
                supported_line_height_px: match size_px {
                    12 => 14,
                    14 => 18,
                    18 => 24,
                    24 => 32,
                    32 => 40,
                    _ => unreachable!(),
                },
            })
        );
    }
}

#[test]
fn rejects_checksum_corruption() {
    let mut bytes = encode(&fixture()).unwrap();
    *bytes.last_mut().unwrap() ^= 0xff;
    assert_eq!(decode(&bytes), Err(DecodeError::ChecksumMismatch));
}

#[test]
fn rejects_bad_magic_and_version() {
    let mut bad_magic = encode(&fixture()).unwrap();
    bad_magic[0] = b'X';
    assert_eq!(decode(&bad_magic), Err(DecodeError::BadMagic));

    let mut bad_version = encode(&fixture()).unwrap();
    bad_version[4..6].copy_from_slice(&1_u16.to_le_bytes());
    assert_eq!(
        decode(&bad_version),
        Err(DecodeError::UnsupportedVersion(1))
    );
}

#[test]
fn rejects_a_bad_text_style_tag() {
    let mut bytes = encode(&fixture()).unwrap();
    let style_tag = bytes.len() - 6;
    bytes[style_tag] = 2;
    refresh_checksum(&mut bytes);

    assert_eq!(
        decode(&bytes),
        Err(DecodeError::InvalidTag {
            section: "text style",
            tag: 2,
        })
    );
}

#[test]
fn rejects_an_unsupported_serialized_text_size() {
    let mut image = fixture();
    image.nodes[0].text_style = Some(TextStyle::ui_sans(18, FontWeight::Regular, 24).unwrap());
    let mut bytes = encode(&image).unwrap();
    let size_px = bytes.len() - 8;
    bytes[size_px] = 16;
    refresh_checksum(&mut bytes);

    assert_eq!(
        decode(&bytes),
        Err(DecodeError::InvalidImage(
            "unsupported text size 16px".into()
        ))
    );
}

#[test]
fn rejects_non_regular_serialized_font_weight() {
    let mut image = fixture();
    image.nodes[0].text_style = Some(TextStyle::ui_sans(18, FontWeight::Regular, 24).unwrap());
    let mut bytes = encode(&image).unwrap();
    let weight = bytes.len() - 7;
    bytes[weight] = 1;
    refresh_checksum(&mut bytes);
    assert_eq!(
        decode(&bytes),
        Err(DecodeError::InvalidTag {
            section: "font weight",
            tag: 1
        })
    );
}

#[test]
fn rejects_an_unsupported_serialized_line_height_pair() {
    let mut image = fixture();
    image.nodes[0].text_style = Some(TextStyle::ui_sans(18, FontWeight::Regular, 24).unwrap());
    let mut bytes = encode(&image).unwrap();
    let line_height_px = bytes.len() - 6;
    bytes[line_height_px] = 17;
    refresh_checksum(&mut bytes);

    assert_eq!(
        decode(&bytes),
        Err(DecodeError::InvalidImage(
            "unsupported 17px line height for 18px text; use 24px".into()
        ))
    );
}

#[test]
fn rejects_truncated_input_without_panicking() {
    let bytes = encode(&fixture()).unwrap();
    for length in 0..bytes.len() {
        assert!(decode(&bytes[..length]).is_err(), "length {length}");
    }
}

#[test]
fn validates_references_jumps_and_stack_depth() {
    let mut bad_state = fixture();
    bad_state.functions[0].code[1] = Instruction::LoadState(StateId(99));
    assert!(validate(&bad_state).is_err());

    let mut bad_jump = fixture();
    bad_jump.functions[0].code[0] = Instruction::Jump(99);
    assert!(validate(&bad_jump).is_err());

    let mut bad_stack = fixture();
    bad_stack.functions[0].max_stack = 1;
    assert!(validate(&bad_stack).is_err());
}

#[test]
fn conditional_jump_consumes_its_condition() {
    let mut image = fixture();
    image.constants.push(Constant::Bool(true));
    image.functions[0] = Function {
        kind: FunctionKind::Handler(micro_ir::HandlerId(0)),
            arg_count: 0,
        locals: 0,
        max_stack: 1,
        code: vec![
            Instruction::Const(2),
            Instruction::JumpIfFalse(3),
            Instruction::Jump(3),
            Instruction::Return,
        ],
    };
    image.nodes[0].text = Some(TextSource::Constant(1));
    image.nodes[0].on_click = Some(FunctionId(0));
    assert!(validate(&image).is_ok());
}

fn refresh_checksum(bytes: &mut [u8]) {
    let checksum = crc32(&bytes[14..]);
    bytes[10..14].copy_from_slice(&checksum.to_le_bytes());
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0_u32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}
