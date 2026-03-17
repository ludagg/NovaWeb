use crate::ast::{Expr, Stmt};
use crate::environment::Environment;
use crate::value::Value;
use anyhow::anyhow;
use std::cell::RefCell;
use std::rc::Rc;

pub struct Interpreter {
    pub globals: Rc<RefCell<Environment>>,
    pub environment: Rc<RefCell<Environment>>,
}

pub enum ControlFlow {
    None,
    Return(Value),
}

impl Interpreter {
    pub fn new() -> Self {
        let globals = Rc::new(RefCell::new(Environment::new()));
        let mut interp = Interpreter {
            globals: globals.clone(),
            environment: globals,
        };
        interp.register_builtins();
        interp
    }

    pub fn with_env(globals: Rc<RefCell<Environment>>) -> Self {
        let mut interp = Interpreter {
            globals: globals.clone(),
            environment: globals,
        };
        interp.register_builtins();
        interp
    }

    fn register_builtins(&mut self) {
        use crate::template;
        use std::fs;

        self.globals.borrow_mut().define(
            "print".to_string(),
            Value::Builtin(|args| {
                for arg in args {
                    print!("{} ", arg);
                }
                println!();
                Value::Null
            }),
        );

        self.globals.borrow_mut().define(
            "render".to_string(),
            Value::Builtin(|args| {
                if args.len() < 2 {
                    return Value::Null;
                }
                if let (Value::String(tmpl), Value::Map(ctx)) = (&args[0], &args[1]) {
                    Value::String(template::render(tmpl, ctx))
                } else {
                    Value::Null
                }
            }),
        );

        self.globals.borrow_mut().define(
            "read_file".to_string(),
            Value::Builtin(|args| {
                if args.is_empty() {
                    return Value::Null;
                }
                if let Value::String(path) = &args[0] {
                    match fs::read_to_string(path) {
                        Ok(s) => Value::String(s),
                        Err(_) => Value::Null,
                    }
                } else {
                    Value::Null
                }
            }),
        );

        self.globals.borrow_mut().define(
            "len".to_string(),
            Value::Builtin(|args| {
                if args.is_empty() {
                    return Value::Null;
                }
                match &args[0] {
                    Value::String(s) => Value::Int(s.len() as i64),
                    Value::List(l) => Value::Int(l.len() as i64),
                    Value::Map(m) => Value::Int(m.len() as i64),
                    _ => Value::Int(0),
                }
            }),
        );
    }

    pub fn interpret(&mut self, statements: &[Stmt]) -> anyhow::Result<Value> {
        let mut result = Value::Null;
        for stmt in statements {
            if let ControlFlow::Return(v) = self.execute(stmt)? {
                result = v;
                break;
            }
        }
        Ok(result)
    }

    fn execute(&mut self, stmt: &Stmt) -> anyhow::Result<ControlFlow> {
        match stmt {
            Stmt::Let { name, init } => {
                let value = self.evaluate(init)?;
                self.environment.borrow_mut().define(name.clone(), value);
                Ok(ControlFlow::None)
            }
            Stmt::Assign { name, value } => {
                let val = self.evaluate(value)?;
                if !self.environment.borrow_mut().assign(name, val) {
                    return Err(anyhow!("Undefined variable '{}'", name));
                }
                Ok(ControlFlow::None)
            }
            Stmt::If {
                condition,
                then_branch,
                else_branch,
            } => {
                if self.evaluate(condition)?.is_truthy() {
                    self.execute_block(then_branch)
                } else if let Some(else_branch) = else_branch {
                    self.execute_block(else_branch)
                } else {
                    Ok(ControlFlow::None)
                }
            }
            Stmt::While { condition, body } => {
                while self.evaluate(condition)?.is_truthy() {
                    if let ControlFlow::Return(v) = self.execute_block(body)? {
                        return Ok(ControlFlow::Return(v));
                    }
                }
                Ok(ControlFlow::None)
            }
            Stmt::For {
                var,
                iterable,
                body,
            } => {
                let iter_val = self.evaluate(iterable)?;
                match iter_val {
                    Value::List(l) => {
                        for val in l {
                            let previous = self.environment.clone();
                            self.environment = Rc::new(RefCell::new(Environment::with_enclosing(
                                previous.clone(),
                            )));
                            self.environment.borrow_mut().define(var.clone(), val);
                            let res = self.execute_block(body);
                            self.environment = previous;
                            if let ControlFlow::Return(v) = res? {
                                return Ok(ControlFlow::Return(v));
                            }
                        }
                    }
                    _ => return Err(anyhow!("Can only iterate over lists")),
                }
                Ok(ControlFlow::None)
            }
            Stmt::Return(expr) => {
                let value = match expr {
                    Some(e) => self.evaluate(e)?,
                    None => Value::Null,
                };
                Ok(ControlFlow::Return(value))
            }
            Stmt::FnDecl { name, params, body } => {
                let function = Value::Function {
                    params: params.clone(),
                    body: body.clone(),
                };
                self.environment.borrow_mut().define(name.clone(), function);
                Ok(ControlFlow::None)
            }
            Stmt::Expr(expr) => {
                self.evaluate(expr)?;
                Ok(ControlFlow::None)
            }
        }
    }

    fn execute_block(&mut self, statements: &[Stmt]) -> anyhow::Result<ControlFlow> {
        let previous = self.environment.clone();
        self.environment = Rc::new(RefCell::new(Environment::with_enclosing(previous.clone())));

        let mut result = Ok(ControlFlow::None);
        for stmt in statements {
            match self.execute(stmt) {
                Ok(ControlFlow::Return(v)) => {
                    result = Ok(ControlFlow::Return(v));
                    break;
                }
                Ok(ControlFlow::None) => continue,
                Err(e) => {
                    result = Err(e);
                    break;
                }
            }
        }

        self.environment = previous;
        result
    }

    fn evaluate(&mut self, expr: &Expr) -> anyhow::Result<Value> {
        match expr {
            Expr::Literal(v) => Ok(v.clone()),
            Expr::Ident(name) => self
                .environment
                .borrow()
                .get(name)
                .ok_or_else(|| anyhow!("Undefined variable '{}'", name)),
            Expr::Binary { op, left, right } => {
                let l = self.evaluate(left)?;
                let r = self.evaluate(right)?;
                match op.as_str() {
                    "+" => l.add(r),
                    "-" => l.sub(r),
                    "*" => l.mul(r),
                    "/" => l.div(r),
                    "==" => Ok(Value::Bool(l == r)),
                    "!=" => Ok(Value::Bool(l != r)),
                    "<" => Ok(Value::Bool(l < r)),
                    ">" => Ok(Value::Bool(l > r)),
                    "<=" => Ok(Value::Bool(l <= r)),
                    ">=" => Ok(Value::Bool(l >= r)),
                    "&&" => Ok(Value::Bool(l.is_truthy() && r.is_truthy())),
                    "||" => Ok(Value::Bool(l.is_truthy() || r.is_truthy())),
                    _ => Err(anyhow!("Unknown operator '{}'", op)),
                }
            }
            Expr::Unary { op, expr } => {
                let val = self.evaluate(expr)?;
                match op.as_str() {
                    "-" => val.negate(),
                    "!" => Ok(Value::Bool(!val.is_truthy())),
                    _ => Err(anyhow!("Unknown unary operator '{}'", op)),
                }
            }
            Expr::Get { object, name } => {
                let obj = self.evaluate(object)?;
                match obj {
                    Value::Map(map) => Ok(map.get(name).cloned().unwrap_or(Value::Null)),
                    _ => Err(anyhow!("Can only access properties on maps")),
                }
            }
            Expr::Index { object, index } => {
                let obj = self.evaluate(object)?;
                let idx = self.evaluate(index)?;
                match (obj, idx) {
                    (Value::List(list), Value::Int(i)) => {
                        if i < 0 {
                            let len = list.len() as i64;
                            let idx = len + i;
                            if idx >= 0 && idx < len {
                                Ok(list[idx as usize].clone())
                            } else {
                                Ok(Value::Null)
                            }
                        } else if i >= 0 && (i as usize) < list.len() {
                            Ok(list[i as usize].clone())
                        } else {
                            Ok(Value::Null)
                        }
                    }
                    (Value::Map(map), Value::String(key)) => Ok(map.get(&key).cloned().unwrap_or(Value::Null)),
                    _ => Err(anyhow!("Invalid index operation")),
                }
            }
            Expr::Call { callee, args } => {
                let function = self
                    .environment
                    .borrow()
                    .get(callee)
                    .ok_or_else(|| anyhow!("Undefined function '{}'", callee))?;

                let mut evaluated_args = Vec::new();
                for arg in args {
                    evaluated_args.push(self.evaluate(arg)?);
                }

                match function {
                    Value::Function { params, body } => {
                        if params.len() != evaluated_args.len() {
                            return Err(anyhow!(
                                "Expected {} arguments, got {}",
                                params.len(),
                                evaluated_args.len()
                            ));
                        }
                        let mut env = Environment::with_enclosing(self.globals.clone());
                        for (param, arg) in params.iter().zip(evaluated_args) {
                            env.define(param.clone(), arg);
                        }

                        let old_env = self.environment.clone();
                        self.environment = Rc::new(RefCell::new(env));
                        let result = self.execute_block(&body);
                        self.environment = old_env;

                        match result? {
                            ControlFlow::Return(v) => Ok(v),
                            ControlFlow::None => Ok(Value::Null),
                        }
                    }
                    Value::Builtin(f) => Ok(f(evaluated_args)),
                    _ => Err(anyhow!("'{}' is not a function", callee)),
                }
            }
            Expr::List(elements) => {
                let mut vals = Vec::new();
                for e in elements {
                    vals.push(self.evaluate(e)?);
                }
                Ok(Value::List(vals))
            }
            Expr::Map(pairs) => {
                let mut m = std::collections::HashMap::new();
                for (k_expr, v_expr) in pairs {
                    let k = self.evaluate(k_expr)?;
                    let v = self.evaluate(v_expr)?;
                    if let Value::String(s) = k {
                        m.insert(s, v);
                    } else {
                        return Err(anyhow!("Map keys must be strings"));
                    }
                }
                Ok(Value::Map(m))
            }
        }
    }
}
