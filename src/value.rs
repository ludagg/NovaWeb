use crate::ast::Stmt;
use anyhow::anyhow;
use std::collections::HashMap;
use std::fmt;

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(untagged)]
pub enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    String(String),
    List(Vec<Value>),
    Map(HashMap<String, Value>),
    #[serde(skip)]
    Function {
        params: Vec<String>,
        body: Vec<Stmt>,
    },
    #[serde(skip)]
    Builtin(fn(Vec<Value>) -> Value),
    Null,
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Int(i) => write!(f, "{}", i),
            Value::Float(fl) => write!(f, "{}", fl),
            Value::Bool(b) => write!(f, "{}", b),
            Value::String(s) => write!(f, "{}", s),
            Value::List(l) => {
                write!(f, "[")?;
                for (i, val) in l.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", val)?;
                }
                write!(f, "]")
            }
            Value::Map(m) => {
                write!(f, "{{")?;
                let mut entries: Vec<_> = m.iter().collect();
                entries.sort_by(|a, b| a.0.cmp(b.0));
                for (i, (k, v)) in entries.into_iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}: {}", k, v)?;
                }
                write!(f, "}}")
            }
            Value::Function { .. } => write!(f, "<function>"),
            Value::Builtin(_) => write!(f, "<builtin>"),
            Value::Null => write!(f, "null"),
        }
    }
}

impl PartialOrd for Value {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        match (self, other) {
            (Value::Int(a), Value::Int(b)) => a.partial_cmp(b),
            (Value::Float(a), Value::Float(b)) => a.partial_cmp(b),
            (Value::Int(a), Value::Float(b)) => (*a as f64).partial_cmp(b),
            (Value::Float(a), Value::Int(b)) => a.partial_cmp(&(*b as f64)),
            (Value::String(a), Value::String(b)) => a.partial_cmp(b),
            (Value::Bool(a), Value::Bool(b)) => a.partial_cmp(b),
            _ => None,
        }
    }
}

impl Value {
    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Bool(b) => *b,
            Value::Null => false,
            Value::Int(i) => *i != 0,
            Value::Float(f) => *f != 0.0,
            Value::String(s) => !s.is_empty(),
            Value::List(l) => !l.is_empty(),
            Value::Map(m) => !m.is_empty(),
            _ => true,
        }
    }

    pub fn add(&self, other: Value) -> anyhow::Result<Value> {
        match (self, other) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(*a + b)),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(*a + b)),
            (Value::Int(a), Value::Float(b)) => Ok(Value::Float(*a as f64 + b)),
            (Value::Float(a), Value::Int(b)) => Ok(Value::Float(*a + b as f64)),
            (Value::String(a), Value::String(b)) => Ok(Value::String(format!("{}{}", a, b))),
            _ => Err(anyhow!("Invalid types for addition")),
        }
    }

    pub fn sub(&self, other: Value) -> anyhow::Result<Value> {
        match (self, other) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(*a - b)),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(*a - b)),
            _ => Err(anyhow!("Invalid types for subtraction")),
        }
    }

    pub fn mul(&self, other: Value) -> anyhow::Result<Value> {
        match (self, other) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(*a * b)),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(*a * b)),
            _ => Err(anyhow!("Invalid types for multiplication")),
        }
    }

    pub fn div(&self, other: Value) -> anyhow::Result<Value> {
        match (self, other) {
            (Value::Int(a), Value::Int(b)) => {
                if b == 0 {
                    return Err(anyhow!("Division by zero"));
                }
                Ok(Value::Int(*a / b))
            }
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(*a / b)),
            _ => Err(anyhow!("Invalid types for division")),
        }
    }

    pub fn negate(&self) -> anyhow::Result<Value> {
        match self {
            Value::Int(i) => Ok(Value::Int(-*i)),
            Value::Float(f) => Ok(Value::Float(-*f)),
            _ => Err(anyhow!("Invalid type for negation")),
        }
    }
}
