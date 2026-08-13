use std::collections::VecDeque;
use std::fmt;

use crate::Instruction;

macro_rules! id_type {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(pub u32);
    };
}

id_type!(StateId);
id_type!(FunctionId);
id_type!(BindingId);
id_type!(HandlerId);
id_type!(NodeId);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScalarType {
    Number,
    String,
    Bool,
    Null,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Constant {
    Number(f64),
    String(String),
    Bool(bool),
    Null,
}

impl Constant {
    pub fn scalar_type(&self) -> ScalarType {
        match self {
            Self::Number(_) => ScalarType::Number,
            Self::String(_) => ScalarType::String,
            Self::Bool(_) => ScalarType::Bool,
            Self::Null => ScalarType::Null,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateDecl {
    pub ty: ScalarType,
    pub initial: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionKind {
    Init,
    Binding(BindingId),
    Handler(HandlerId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Function {
    pub kind: FunctionKind,
    pub locals: u16,
    pub max_stack: u16,
    pub code: Vec<Instruction>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiKind {
    Column,
    Text,
    Button,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextSource {
    Constant(u32),
    Binding(FunctionId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiNodeSpec {
    pub id: NodeId,
    pub kind: UiKind,
    pub children: Vec<NodeId>,
    pub text: Option<TextSource>,
    pub on_click: Option<FunctionId>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AppImage {
    pub constants: Vec<Constant>,
    pub states: Vec<StateDecl>,
    pub functions: Vec<Function>,
    pub nodes: Vec<UiNodeSpec>,
    pub root: NodeId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError(pub String);

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ValidationError {}

fn invalid(message: impl Into<String>) -> ValidationError {
    ValidationError(message.into())
}

pub fn validate(image: &AppImage) -> Result<(), ValidationError> {
    if image.nodes.is_empty() {
        return Err(invalid("UI tree is empty"));
    }
    if image.root.0 as usize >= image.nodes.len() {
        return Err(invalid("root node is out of range"));
    }

    for (index, state) in image.states.iter().enumerate() {
        let constant = image
            .constants
            .get(state.initial as usize)
            .ok_or_else(|| invalid(format!("state {index} has an invalid initial constant")))?;
        if constant.scalar_type() != state.ty {
            return Err(invalid(format!(
                "state {index} initial type does not match"
            )));
        }
    }

    for (index, function) in image.functions.iter().enumerate() {
        validate_function(image, index, function)?;
    }

    for (index, node) in image.nodes.iter().enumerate() {
        if node.id.0 as usize != index {
            return Err(invalid(format!("node id {} is not canonical", node.id.0)));
        }
        for child in &node.children {
            if child.0 as usize >= image.nodes.len() {
                return Err(invalid(format!("node {index} has an invalid child")));
            }
        }
        if let Some(text) = node.text {
            match text {
                TextSource::Constant(id) if id as usize >= image.constants.len() => {
                    return Err(invalid(format!(
                        "node {index} has an invalid text constant"
                    )));
                }
                TextSource::Binding(id) => match image.functions.get(id.0 as usize) {
                    Some(Function {
                        kind: FunctionKind::Binding(_),
                        ..
                    }) => {}
                    _ => return Err(invalid(format!("node {index} has an invalid binding"))),
                },
                TextSource::Constant(_) => {}
            }
        }
        if let Some(id) = node.on_click {
            match image.functions.get(id.0 as usize) {
                Some(Function {
                    kind: FunctionKind::Handler(_),
                    ..
                }) => {}
                _ => {
                    return Err(invalid(format!(
                        "node {index} has an invalid click handler"
                    )));
                }
            }
        }
    }
    Ok(())
}

fn validate_function(
    image: &AppImage,
    index: usize,
    function: &Function,
) -> Result<(), ValidationError> {
    if function.code.is_empty() {
        return Err(invalid(format!("function {index} is empty")));
    }
    let mut depths = vec![None; function.code.len()];
    let mut queue = VecDeque::from([(0_usize, 0_i32)]);
    let mut observed_max = 0_i32;

    while let Some((pc, depth)) = queue.pop_front() {
        if pc >= function.code.len() {
            return Err(invalid(format!("function {index} falls off the end")));
        }
        if let Some(previous) = depths[pc] {
            if previous != depth {
                return Err(invalid(format!(
                    "function {index} has inconsistent stack depth"
                )));
            }
            continue;
        }
        depths[pc] = Some(depth);
        let instruction = &function.code[pc];
        validate_operand(image, function, index, instruction)?;
        let (required, delta) = stack_effect(instruction);
        if depth < required {
            return Err(invalid(format!("function {index} underflows its stack")));
        }
        let next_depth = depth + delta;
        observed_max = observed_max.max(next_depth);

        match instruction {
            Instruction::Return => {
                let expected = match function.kind {
                    FunctionKind::Binding(_) => 1,
                    FunctionKind::Init | FunctionKind::Handler(_) => 0,
                };
                if depth != expected {
                    return Err(invalid(format!(
                        "function {index} returns with wrong stack depth"
                    )));
                }
            }
            Instruction::Jump(target) => {
                push_target(index, &function.code, &mut queue, *target, next_depth)?
            }
            Instruction::JumpIfFalse(target) => {
                push_target(index, &function.code, &mut queue, *target, next_depth)?;
                queue.push_back((pc + 1, next_depth));
            }
            _ => queue.push_back((pc + 1, next_depth)),
        }
    }

    if observed_max > i32::from(function.max_stack) {
        return Err(invalid(format!(
            "function {index} declares too little stack"
        )));
    }
    Ok(())
}

fn push_target(
    index: usize,
    code: &[Instruction],
    queue: &mut VecDeque<(usize, i32)>,
    target: u32,
    depth: i32,
) -> Result<(), ValidationError> {
    if target as usize >= code.len() {
        return Err(invalid(format!("function {index} has an invalid jump")));
    }
    queue.push_back((target as usize, depth));
    Ok(())
}

fn validate_operand(
    image: &AppImage,
    function: &Function,
    index: usize,
    instruction: &Instruction,
) -> Result<(), ValidationError> {
    match instruction {
        Instruction::Const(id) if *id as usize >= image.constants.len() => {
            Err(invalid(format!("function {index} has an invalid constant")))
        }
        Instruction::LoadState(id) | Instruction::StoreState(id)
            if id.0 as usize >= image.states.len() =>
        {
            Err(invalid(format!("function {index} has an invalid state")))
        }
        Instruction::LoadLocal(id) | Instruction::StoreLocal(id) if *id >= function.locals => {
            Err(invalid(format!("function {index} has an invalid local")))
        }
        _ => Ok(()),
    }
}

fn stack_effect(instruction: &Instruction) -> (i32, i32) {
    match instruction {
        Instruction::Const(_) | Instruction::LoadLocal(_) | Instruction::LoadState(_) => (0, 1),
        Instruction::StoreLocal(_) | Instruction::StoreState(_) | Instruction::Pop => (1, -1),
        Instruction::Add
        | Instruction::Sub
        | Instruction::Mul
        | Instruction::Div
        | Instruction::Eq
        | Instruction::Lt
        | Instruction::Gt
        | Instruction::Concat => (2, -1),
        Instruction::Not | Instruction::ToString => (1, 0),
        Instruction::JumpIfFalse(_) => (1, -1),
        Instruction::Dup => (1, 1),
        Instruction::Jump(_) | Instruction::Return => (0, 0),
    }
}
