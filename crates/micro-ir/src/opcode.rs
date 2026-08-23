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
    Mod,
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
    /// Pushes the runtime-supplied string argument of the current handler
    /// onto the stack. Only valid inside a `Function` with
    /// `arg_count == 1` (today, only `ui.input` `onChange(s)`).
    LoadArg,
    /// Invokes the host request at `AppImage::host_requests[idx]`. Pops the
    /// request's argument count; for a synchronous read the host pushes the
    /// scalar result back; actions and async requests push nothing.
    HostCall(u32),
}
