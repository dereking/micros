use micro_ir::{
    AppImage, BindingId, Constant, DecodeError, Function, FunctionId, FunctionKind, Instruction,
    NodeId, ScalarType, StateDecl, StateId, TextSource, UiKind, UiNodeSpec, decode, encode,
    validate,
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
            on_click: None,
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
    bad_version[4..6].copy_from_slice(&2_u16.to_le_bytes());
    assert_eq!(
        decode(&bad_version),
        Err(DecodeError::UnsupportedVersion(2))
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
