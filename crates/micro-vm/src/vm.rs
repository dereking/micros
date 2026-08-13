use micro_ir::{AppImage, FunctionId, FunctionKind, Instruction, StateId};

use crate::{StateError, Value, VmError};

pub trait StateAccess {
    fn read(&mut self, id: StateId) -> Result<Value, StateError>;
    fn write(&mut self, id: StateId, value: Value) -> Result<(), StateError>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct Execution {
    pub value: Option<Value>,
    pub executed: u64,
}

pub struct Vm<'image, 'state, S> {
    image: &'image AppImage,
    state: &'state mut S,
}

impl<'image, 'state, S: StateAccess> Vm<'image, 'state, S> {
    pub fn new(image: &'image AppImage, state: &'state mut S) -> Self {
        Self { image, state }
    }

    pub fn invoke(&mut self, function_id: FunctionId, budget: u64) -> Result<Execution, VmError> {
        let function = self
            .image
            .functions
            .get(function_id.0 as usize)
            .ok_or(VmError::FunctionOutOfRange(function_id))?;
        let mut stack = Vec::with_capacity(function.max_stack as usize);
        let mut locals = vec![Value::Null; function.locals as usize];
        let mut pc = 0_usize;
        let mut executed = 0_u64;

        loop {
            if executed == budget {
                return Err(VmError::BudgetExceeded {
                    function: function_id,
                    executed,
                });
            }
            let instruction = function
                .code
                .get(pc)
                .ok_or(VmError::InvalidReference("instruction"))?;
            executed += 1;
            pc += 1;

            match instruction {
                Instruction::Const(id) => {
                    let constant = self
                        .image
                        .constants
                        .get(*id as usize)
                        .ok_or(VmError::InvalidReference("constant"))?;
                    stack.push(Value::from(constant));
                }
                Instruction::LoadLocal(id) => stack.push(
                    locals
                        .get(*id as usize)
                        .cloned()
                        .ok_or(VmError::LocalOutOfRange(*id))?,
                ),
                Instruction::StoreLocal(id) => {
                    let value = pop(&mut stack)?;
                    *locals
                        .get_mut(*id as usize)
                        .ok_or(VmError::LocalOutOfRange(*id))? = value;
                }
                Instruction::LoadState(id) => stack.push(self.state.read(*id)?),
                Instruction::StoreState(id) => self.state.write(*id, pop(&mut stack)?)?,
                Instruction::Add => binary_number(&mut stack, |left, right| left + right)?,
                Instruction::Sub => binary_number(&mut stack, |left, right| left - right)?,
                Instruction::Mul => binary_number(&mut stack, |left, right| left * right)?,
                Instruction::Div => {
                    let right = number(pop(&mut stack)?)?;
                    if right == 0.0 {
                        return Err(VmError::DivisionByZero);
                    }
                    let left = number(pop(&mut stack)?)?;
                    stack.push(Value::Number(left / right));
                }
                Instruction::Eq => {
                    let right = pop(&mut stack)?;
                    let left = pop(&mut stack)?;
                    stack.push(Value::Bool(left == right));
                }
                Instruction::Lt => compare_number(&mut stack, |left, right| left < right)?,
                Instruction::Gt => compare_number(&mut stack, |left, right| left > right)?,
                Instruction::Not => {
                    let value = boolean(pop(&mut stack)?)?;
                    stack.push(Value::Bool(!value));
                }
                Instruction::ToString => {
                    let value = pop(&mut stack)?;
                    stack.push(Value::String(value.into_string()));
                }
                Instruction::Concat => {
                    let right = string(pop(&mut stack)?)?;
                    let left = string(pop(&mut stack)?)?;
                    stack.push(Value::String(left + &right));
                }
                Instruction::Pop => {
                    pop(&mut stack)?;
                }
                Instruction::Dup => {
                    stack.push(stack.last().cloned().ok_or(VmError::StackUnderflow)?)
                }
                Instruction::Jump(target) => pc = *target as usize,
                Instruction::JumpIfFalse(target) => {
                    if !boolean(pop(&mut stack)?)? {
                        pc = *target as usize;
                    }
                }
                Instruction::Return => {
                    let value = match function.kind {
                        FunctionKind::Binding(_) => Some(pop(&mut stack)?),
                        FunctionKind::Init | FunctionKind::Handler(_) => None,
                    };
                    return Ok(Execution { value, executed });
                }
            }
        }
    }
}

fn pop(stack: &mut Vec<Value>) -> Result<Value, VmError> {
    stack.pop().ok_or(VmError::StackUnderflow)
}

fn number(value: Value) -> Result<f64, VmError> {
    match value {
        Value::Number(value) => Ok(value),
        value => Err(VmError::TypeMismatch {
            expected: "number",
            found: value.type_name(),
        }),
    }
}

fn boolean(value: Value) -> Result<bool, VmError> {
    match value {
        Value::Bool(value) => Ok(value),
        value => Err(VmError::TypeMismatch {
            expected: "boolean",
            found: value.type_name(),
        }),
    }
}

fn string(value: Value) -> Result<String, VmError> {
    match value {
        Value::String(value) => Ok(value),
        value => Err(VmError::TypeMismatch {
            expected: "string",
            found: value.type_name(),
        }),
    }
}

fn binary_number(
    stack: &mut Vec<Value>,
    operation: impl FnOnce(f64, f64) -> f64,
) -> Result<(), VmError> {
    let right = number(pop(stack)?)?;
    let left = number(pop(stack)?)?;
    stack.push(Value::Number(operation(left, right)));
    Ok(())
}

fn compare_number(
    stack: &mut Vec<Value>,
    operation: impl FnOnce(f64, f64) -> bool,
) -> Result<(), VmError> {
    let right = number(pop(stack)?)?;
    let left = number(pop(stack)?)?;
    stack.push(Value::Bool(operation(left, right)));
    Ok(())
}
