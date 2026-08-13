use micro_ir::{Constant, ScalarType};

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Number(f64),
    String(String),
    Bool(bool),
    Null,
}

impl Value {
    pub fn scalar_type(&self) -> ScalarType {
        match self {
            Self::Number(_) => ScalarType::Number,
            Self::String(_) => ScalarType::String,
            Self::Bool(_) => ScalarType::Bool,
            Self::Null => ScalarType::Null,
        }
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Number(_) => "number",
            Self::String(_) => "string",
            Self::Bool(_) => "boolean",
            Self::Null => "null",
        }
    }

    pub fn into_string(self) -> String {
        match self {
            Self::Number(value) => value.to_string(),
            Self::String(value) => value,
            Self::Bool(value) => value.to_string(),
            Self::Null => "null".into(),
        }
    }
}

impl From<&Constant> for Value {
    fn from(value: &Constant) -> Self {
        match value {
            Constant::Number(value) => Self::Number(*value),
            Constant::String(value) => Self::String(value.clone()),
            Constant::Bool(value) => Self::Bool(*value),
            Constant::Null => Self::Null,
        }
    }
}
