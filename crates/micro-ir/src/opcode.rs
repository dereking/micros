use crate::StateId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Instruction {
    Const(u32),
    LoadLocal(u16),
    StoreLocal(u16),
    LoadState(StateId),
    StoreState(StateId),
    Add,
    Sub,
    Mul,
    Div,
    Eq,
    Lt,
    Gt,
    Not,
    ToString,
    Concat,
    Pop,
    Dup,
    Jump(u32),
    JumpIfFalse(u32),
    Return,
}
