use std::fmt;

use crate::{
    AnchorSpec, AppImage, BindingId, Constant, FontFamily, FontWeight, Function, FunctionId,
    FunctionKind, HandlerId, Instruction, LayoutSpec, NodeId, ScalarType, StateDecl, StateId,
    TextSource, TextStyle, UiKind, UiNodeSpec, ValidationError, ValueSource, validate,
};

const MAGIC: &[u8; 4] = b"MBC1";
/// MBC v15 splits the per-node layout into ltwh base geometry + anchor edges.
const VERSION: u16 = 15;
const HEADER_LEN: usize = 14;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EncodeError {
    InvalidImage(String),
    TooLarge,
}

impl fmt::Display for EncodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for EncodeError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    Truncated,
    BadMagic,
    UnsupportedVersion(u16),
    LengthMismatch,
    ChecksumMismatch,
    InvalidTag { section: &'static str, tag: u8 },
    InvalidUtf8,
    InvalidImage(String),
    TrailingBytes,
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for DecodeError {}

impl From<ValidationError> for DecodeError {
    fn from(value: ValidationError) -> Self {
        Self::InvalidImage(value.0)
    }
}

pub fn encode(image: &AppImage) -> Result<Vec<u8>, EncodeError> {
    validate(image).map_err(|error| EncodeError::InvalidImage(error.0))?;
    let mut payload = Vec::new();
    write_section(&mut payload, 1, encode_constants(&image.constants)?)?;
    write_section(&mut payload, 2, encode_states(&image.states)?)?;
    write_section(&mut payload, 3, encode_functions(&image.functions)?)?;
    write_section(&mut payload, 4, encode_ui(&image.nodes, image.root)?)?;
    let payload_len = u32::try_from(payload.len()).map_err(|_| EncodeError::TooLarge)?;

    let mut bytes = Vec::with_capacity(HEADER_LEN + payload.len());
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&VERSION.to_le_bytes());
    bytes.extend_from_slice(&payload_len.to_le_bytes());
    bytes.extend_from_slice(&crc32(&payload).to_le_bytes());
    bytes.extend_from_slice(&payload);
    Ok(bytes)
}

pub fn decode(bytes: &[u8]) -> Result<AppImage, DecodeError> {
    if bytes.len() < HEADER_LEN {
        return Err(DecodeError::Truncated);
    }
    if &bytes[..4] != MAGIC {
        return Err(DecodeError::BadMagic);
    }
    let version = u16::from_le_bytes([bytes[4], bytes[5]]);
    if version != VERSION {
        return Err(DecodeError::UnsupportedVersion(version));
    }
    let payload_len = u32::from_le_bytes(bytes[6..10].try_into().unwrap()) as usize;
    if bytes.len() != HEADER_LEN + payload_len {
        return Err(DecodeError::LengthMismatch);
    }
    let checksum = u32::from_le_bytes(bytes[10..14].try_into().unwrap());
    let payload = &bytes[HEADER_LEN..];
    if crc32(payload) != checksum {
        return Err(DecodeError::ChecksumMismatch);
    }

    let mut reader = Reader::new(payload);
    let constants = decode_constants(&mut section(&mut reader, 1)?)?;
    let states = decode_states(&mut section(&mut reader, 2)?)?;
    let functions = decode_functions(&mut section(&mut reader, 3)?)?;
    let (nodes, root) = decode_ui(&mut section(&mut reader, 4)?)?;
    reader.finish()?;
    let image = AppImage {
        constants,
        states,
        functions,
        nodes,
        root,
    };
    validate(&image)?;
    Ok(image)
}

fn write_section(output: &mut Vec<u8>, kind: u8, bytes: Vec<u8>) -> Result<(), EncodeError> {
    output.push(kind);
    put_u32(output, bytes.len())?;
    output.extend(bytes);
    Ok(())
}

fn section<'a>(reader: &mut Reader<'a>, expected: u8) -> Result<Reader<'a>, DecodeError> {
    let actual = reader.u8()?;
    if actual != expected {
        return Err(DecodeError::InvalidTag {
            section: "section",
            tag: actual,
        });
    }
    let length = reader.u32()? as usize;
    Ok(Reader::new(reader.take(length)?))
}

fn encode_constants(constants: &[Constant]) -> Result<Vec<u8>, EncodeError> {
    let mut out = Vec::new();
    put_u32(&mut out, constants.len())?;
    for constant in constants {
        match constant {
            Constant::Number(value) => {
                out.push(0);
                out.extend_from_slice(&value.to_le_bytes());
            }
            Constant::String(value) => {
                out.push(1);
                put_bytes(&mut out, value.as_bytes())?;
            }
            Constant::Bool(value) => {
                out.push(2);
                out.push(u8::from(*value));
            }
            Constant::Null => out.push(3),
        }
    }
    Ok(out)
}

fn decode_constants(reader: &mut Reader<'_>) -> Result<Vec<Constant>, DecodeError> {
    let count = reader.u32()? as usize;
    let mut constants = Vec::with_capacity(count);
    for _ in 0..count {
        constants.push(match reader.u8()? {
            0 => Constant::Number(f64::from_le_bytes(reader.take(8)?.try_into().unwrap())),
            1 => Constant::String(
                std::str::from_utf8(reader.bytes()?)
                    .map_err(|_| DecodeError::InvalidUtf8)?
                    .into(),
            ),
            2 => Constant::Bool(match reader.u8()? {
                0 => false,
                1 => true,
                tag => {
                    return Err(DecodeError::InvalidTag {
                        section: "bool",
                        tag,
                    });
                }
            }),
            3 => Constant::Null,
            tag => {
                return Err(DecodeError::InvalidTag {
                    section: "constant",
                    tag,
                });
            }
        });
    }
    reader.finish()?;
    Ok(constants)
}

fn encode_states(states: &[StateDecl]) -> Result<Vec<u8>, EncodeError> {
    let mut out = Vec::new();
    put_u32(&mut out, states.len())?;
    for state in states {
        out.push(scalar_tag(state.ty));
        out.extend_from_slice(&state.initial.to_le_bytes());
    }
    Ok(out)
}

fn decode_states(reader: &mut Reader<'_>) -> Result<Vec<StateDecl>, DecodeError> {
    let count = reader.u32()? as usize;
    let mut states = Vec::with_capacity(count);
    for _ in 0..count {
        states.push(StateDecl {
            ty: decode_scalar(reader.u8()?)?,
            initial: reader.u32()?,
        });
    }
    reader.finish()?;
    Ok(states)
}

fn encode_functions(functions: &[Function]) -> Result<Vec<u8>, EncodeError> {
    let mut out = Vec::new();
    put_u32(&mut out, functions.len())?;
    for function in functions {
        match function.kind {
            FunctionKind::Init => {
                out.push(0);
                out.extend_from_slice(&0_u32.to_le_bytes());
            }
            FunctionKind::Binding(id) => {
                out.push(1);
                out.extend_from_slice(&id.0.to_le_bytes());
            }
            FunctionKind::Handler(id) => {
                out.push(2);
                out.extend_from_slice(&id.0.to_le_bytes());
            }
        }
        out.push(function.arg_count);
        out.extend_from_slice(&function.locals.to_le_bytes());
        out.extend_from_slice(&function.max_stack.to_le_bytes());
        put_u32(&mut out, function.code.len())?;
        for instruction in &function.code {
            encode_instruction(&mut out, instruction);
        }
    }
    Ok(out)
}

fn decode_functions(reader: &mut Reader<'_>) -> Result<Vec<Function>, DecodeError> {
    let count = reader.u32()? as usize;
    let mut functions = Vec::with_capacity(count);
    for _ in 0..count {
        let tag = reader.u8()?;
        let id = reader.u32()?;
        let kind = match tag {
            0 => FunctionKind::Init,
            1 => FunctionKind::Binding(BindingId(id)),
            2 => FunctionKind::Handler(HandlerId(id)),
            tag => {
                return Err(DecodeError::InvalidTag {
                    section: "function",
                    tag,
                });
            }
        };
        let arg_count = reader.u8()?;
        let locals = reader.u16()?;
        let max_stack = reader.u16()?;
        let instruction_count = reader.u32()? as usize;
        let mut code = Vec::with_capacity(instruction_count);
        for _ in 0..instruction_count {
            code.push(decode_instruction(reader)?);
        }
        functions.push(Function {
            kind,
            arg_count,
            locals,
            max_stack,
            code,
        });
    }
    reader.finish()?;
    Ok(functions)
}

fn encode_ui(nodes: &[UiNodeSpec], root: NodeId) -> Result<Vec<u8>, EncodeError> {
    let mut out = Vec::new();
    put_u32(&mut out, nodes.len())?;
    for node in nodes {
        out.extend_from_slice(&node.id.0.to_le_bytes());
        out.push(match node.kind {
            UiKind::Column => 0,
            UiKind::Text => 1,
            UiKind::Button => 2,
            UiKind::Row => 3,
            UiKind::Progress => 4,
            UiKind::Switch => 5,
            UiKind::Input => 6,
            UiKind::Slider => 7,
            UiKind::Checkbox => 8,
            UiKind::Dropdown => 9,
            UiKind::Roller => 10,
            UiKind::Led => 11,
            UiKind::Spinner => 12,
            UiKind::Scale => 13,
            UiKind::List => 14,
            UiKind::Tabview => 15,
        });
        put_u32(&mut out, node.children.len())?;
        for child in &node.children {
            out.extend_from_slice(&child.0.to_le_bytes());
        }
        match node.text {
            None => {
                out.push(0);
                out.extend_from_slice(&0_u32.to_le_bytes());
            }
            Some(TextSource::Constant(id)) => {
                out.push(1);
                out.extend_from_slice(&id.to_le_bytes());
            }
            Some(TextSource::Binding(id)) => {
                out.push(2);
                out.extend_from_slice(&id.0.to_le_bytes());
            }
        }
        match node.value {
            None => {
                out.push(0);
                out.extend_from_slice(&0_u32.to_le_bytes());
            }
            Some(ValueSource::Constant(id)) => {
                out.push(1);
                out.extend_from_slice(&id.to_le_bytes());
            }
            Some(ValueSource::Binding(id)) => {
                out.push(2);
                out.extend_from_slice(&id.0.to_le_bytes());
            }
        }
        match node.on_click {
            None => {
                out.push(0);
                out.extend_from_slice(&0_u32.to_le_bytes());
            }
            Some(id) => {
                out.push(1);
                out.extend_from_slice(&id.0.to_le_bytes());
            }
        }
        match node.text_style {
            None => out.push(0),
            Some(style) => {
                out.push(1);
                out.push(match style.family {
                    FontFamily::UiSans => 0,
                });
                out.push(style.size_px);
                out.push(match style.weight {
                    FontWeight::Regular => 0,
                });
                out.push(style.line_height_px);
            }
        }
        match node.range {
            None => out.push(0),
            Some((min, max)) => {
                out.push(1);
                out.extend_from_slice(&min.to_le_bytes());
                out.extend_from_slice(&max.to_le_bytes());
            }
        }
        put_u32(&mut out, node.options.len())?;
        for option in &node.options {
            out.extend_from_slice(&option.to_le_bytes());
        }
        match node.layout {
            None => out.push(0),
            Some(layout) => {
                out.push(1);
                /* Mask bit0=left, bit1=top, bit2=width, bit3=height,
                 * bit4=anchor_left, bit5=anchor_top, bit6=anchor_right,
                 * bit7=anchor_bottom; the set values follow in that order. */
                let mut mask = 0u8;
                if layout.left.is_some() {
                    mask |= 1;
                }
                if layout.top.is_some() {
                    mask |= 2;
                }
                if layout.width.is_some() {
                    mask |= 4;
                }
                if layout.height.is_some() {
                    mask |= 8;
                }
                if layout.anchor.left.is_some() {
                    mask |= 16;
                }
                if layout.anchor.top.is_some() {
                    mask |= 32;
                }
                if layout.anchor.right.is_some() {
                    mask |= 64;
                }
                if layout.anchor.bottom.is_some() {
                    mask |= 128;
                }
                out.push(mask);
                for value in [
                    layout.left,
                    layout.top,
                    layout.width,
                    layout.height,
                    layout.anchor.left,
                    layout.anchor.top,
                    layout.anchor.right,
                    layout.anchor.bottom,
                ] {
                    if let Some(value) = value {
                        out.extend_from_slice(&value.to_le_bytes());
                    }
                }
            }
        }
    }
    out.extend_from_slice(&root.0.to_le_bytes());
    Ok(out)
}

fn decode_ui(reader: &mut Reader<'_>) -> Result<(Vec<UiNodeSpec>, NodeId), DecodeError> {
    let count = reader.u32()? as usize;
    let mut nodes = Vec::with_capacity(count);
    for _ in 0..count {
        let id = NodeId(reader.u32()?);
        let kind = match reader.u8()? {
            0 => UiKind::Column,
            1 => UiKind::Text,
            2 => UiKind::Button,
            3 => UiKind::Row,
            4 => UiKind::Progress,
            5 => UiKind::Switch,
            6 => UiKind::Input,
            7 => UiKind::Slider,
            8 => UiKind::Checkbox,
            9 => UiKind::Dropdown,
            10 => UiKind::Roller,
            11 => UiKind::Led,
            12 => UiKind::Spinner,
            13 => UiKind::Scale,
            14 => UiKind::List,
            15 => UiKind::Tabview,
            tag => {
                return Err(DecodeError::InvalidTag {
                    section: "ui kind",
                    tag,
                });
            }
        };
        let child_count = reader.u32()? as usize;
        let mut children = Vec::with_capacity(child_count);
        for _ in 0..child_count {
            children.push(NodeId(reader.u32()?));
        }
        let text_tag = reader.u8()?;
        let text_id = reader.u32()?;
        let text = match text_tag {
            0 => None,
            1 => Some(TextSource::Constant(text_id)),
            2 => Some(TextSource::Binding(FunctionId(text_id))),
            tag => {
                return Err(DecodeError::InvalidTag {
                    section: "text source",
                    tag,
                });
            }
        };
        let value_tag = reader.u8()?;
        let value_id = reader.u32()?;
        let value = match value_tag {
            0 => None,
            1 => Some(ValueSource::Constant(value_id)),
            2 => Some(ValueSource::Binding(FunctionId(value_id))),
            tag => {
                return Err(DecodeError::InvalidTag {
                    section: "value source",
                    tag,
                });
            }
        };
        let click_tag = reader.u8()?;
        let click_id = reader.u32()?;
        let on_click = match click_tag {
            0 => None,
            1 => Some(FunctionId(click_id)),
            tag => {
                return Err(DecodeError::InvalidTag {
                    section: "click",
                    tag,
                });
            }
        };
        let text_style = match reader.u8()? {
            0 => None,
            1 => {
                let family = match reader.u8()? {
                    0 => FontFamily::UiSans,
                    tag => {
                        return Err(DecodeError::InvalidTag {
                            section: "font family",
                            tag,
                        });
                    }
                };
                let size_px = reader.u8()?;
                let weight = match reader.u8()? {
                    0 => FontWeight::Regular,
                    tag => {
                        return Err(DecodeError::InvalidTag {
                            section: "font weight",
                            tag,
                        });
                    }
                };
                let line_height_px = reader.u8()?;
                Some(
                    TextStyle::new(family, size_px, weight, line_height_px)
                        .map_err(|error| DecodeError::InvalidImage(error.to_string()))?,
                )
            }
            tag => {
                return Err(DecodeError::InvalidTag {
                    section: "text style",
                    tag,
                });
            }
        };
        let range = match reader.u8()? {
            0 => None,
            1 => {
                let min = f64::from_le_bytes(reader.take(8)?.try_into().unwrap());
                let max = f64::from_le_bytes(reader.take(8)?.try_into().unwrap());
                Some((min, max))
            }
            tag => {
                return Err(DecodeError::InvalidTag {
                    section: "range",
                    tag,
                });
            }
        };
        let option_count = reader.u32()? as usize;
        let mut options = Vec::with_capacity(option_count);
        for _ in 0..option_count {
            options.push(reader.u32()?);
        }
        let layout = match reader.u8()? {
            0 => None,
            1 => {
                let mask = reader.u8()?;
                let mut next = || -> Result<Option<f64>, DecodeError> {
                    let value = f64::from_le_bytes(reader.take(8)?.try_into().unwrap());
                    Ok(Some(value))
                };
                Some(LayoutSpec {
                    left: if mask & 1 != 0 { next()? } else { None },
                    top: if mask & 2 != 0 { next()? } else { None },
                    width: if mask & 4 != 0 { next()? } else { None },
                    height: if mask & 8 != 0 { next()? } else { None },
                    anchor: AnchorSpec {
                        left: if mask & 16 != 0 { next()? } else { None },
                        top: if mask & 32 != 0 { next()? } else { None },
                        right: if mask & 64 != 0 { next()? } else { None },
                        bottom: if mask & 128 != 0 { next()? } else { None },
                    },
                })
            }
            tag => {
                return Err(DecodeError::InvalidTag {
                    section: "layout",
                    tag,
                });
            }
        };
        nodes.push(UiNodeSpec {
            id,
            kind,
            children,
            text,
            value,
            on_click,
            text_style,
            range,
            options,
            layout,
        });
    }
    let root = NodeId(reader.u32()?);
    reader.finish()?;
    Ok((nodes, root))
}

fn encode_instruction(out: &mut Vec<u8>, instruction: &Instruction) {
    let (tag, u32_operand, u16_operand) = match instruction {
        Instruction::Const(value) => (0, Some(*value), None),
        Instruction::LoadLocal(value) => (1, None, Some(*value)),
        Instruction::StoreLocal(value) => (2, None, Some(*value)),
        Instruction::LoadState(value) => (3, Some(value.0), None),
        Instruction::StoreState(value) => (4, Some(value.0), None),
        Instruction::Add => (5, None, None),
        Instruction::Sub => (6, None, None),
        Instruction::Mul => (7, None, None),
        Instruction::Div => (8, None, None),
        Instruction::Mod => (21, None, None),
        Instruction::Eq => (9, None, None),
        Instruction::Lt => (10, None, None),
        Instruction::Gt => (11, None, None),
        Instruction::Not => (12, None, None),
        Instruction::ToString => (13, None, None),
        Instruction::Concat => (14, None, None),
        Instruction::Pop => (15, None, None),
        Instruction::Dup => (16, None, None),
        Instruction::Jump(value) => (17, Some(*value), None),
        Instruction::JumpIfFalse(value) => (18, Some(*value), None),
        Instruction::Return => (19, None, None),
        Instruction::LoadArg => (20, None, None),
    };
    out.push(tag);
    if let Some(value) = u32_operand {
        out.extend_from_slice(&value.to_le_bytes());
    }
    if let Some(value) = u16_operand {
        out.extend_from_slice(&value.to_le_bytes());
    }
}

fn decode_instruction(reader: &mut Reader<'_>) -> Result<Instruction, DecodeError> {
    Ok(match reader.u8()? {
        0 => Instruction::Const(reader.u32()?),
        1 => Instruction::LoadLocal(reader.u16()?),
        2 => Instruction::StoreLocal(reader.u16()?),
        3 => Instruction::LoadState(StateId(reader.u32()?)),
        4 => Instruction::StoreState(StateId(reader.u32()?)),
        5 => Instruction::Add,
        6 => Instruction::Sub,
        7 => Instruction::Mul,
        8 => Instruction::Div,
        21 => Instruction::Mod,
        9 => Instruction::Eq,
        10 => Instruction::Lt,
        11 => Instruction::Gt,
        12 => Instruction::Not,
        13 => Instruction::ToString,
        14 => Instruction::Concat,
        15 => Instruction::Pop,
        16 => Instruction::Dup,
        17 => Instruction::Jump(reader.u32()?),
        18 => Instruction::JumpIfFalse(reader.u32()?),
        19 => Instruction::Return,
        20 => Instruction::LoadArg,
        tag => {
            return Err(DecodeError::InvalidTag {
                section: "instruction",
                tag,
            });
        }
    })
}

fn scalar_tag(value: ScalarType) -> u8 {
    match value {
        ScalarType::Number => 0,
        ScalarType::String => 1,
        ScalarType::Bool => 2,
        ScalarType::Null => 3,
    }
}
fn decode_scalar(tag: u8) -> Result<ScalarType, DecodeError> {
    match tag {
        0 => Ok(ScalarType::Number),
        1 => Ok(ScalarType::String),
        2 => Ok(ScalarType::Bool),
        3 => Ok(ScalarType::Null),
        tag => Err(DecodeError::InvalidTag {
            section: "scalar",
            tag,
        }),
    }
}
fn put_u32(out: &mut Vec<u8>, value: usize) -> Result<(), EncodeError> {
    out.extend_from_slice(
        &u32::try_from(value)
            .map_err(|_| EncodeError::TooLarge)?
            .to_le_bytes(),
    );
    Ok(())
}
fn put_bytes(out: &mut Vec<u8>, bytes: &[u8]) -> Result<(), EncodeError> {
    put_u32(out, bytes.len())?;
    out.extend_from_slice(bytes);
    Ok(())
}

struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}
impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }
    fn take(&mut self, length: usize) -> Result<&'a [u8], DecodeError> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or(DecodeError::Truncated)?;
        let result = self
            .bytes
            .get(self.offset..end)
            .ok_or(DecodeError::Truncated)?;
        self.offset = end;
        Ok(result)
    }
    fn u8(&mut self) -> Result<u8, DecodeError> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, DecodeError> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> Result<u32, DecodeError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn bytes(&mut self) -> Result<&'a [u8], DecodeError> {
        let length = self.u32()? as usize;
        self.take(length)
    }
    fn finish(&self) -> Result<(), DecodeError> {
        if self.offset == self.bytes.len() {
            Ok(())
        } else {
            Err(DecodeError::TrailingBytes)
        }
    }
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
