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
        Interpreter {
            globals: globals.clone(),
            environment: globals,
        }
    }

    pub fn interpret(&mut self, statements: &[Stmt]) -> anyhow::Result<()> {
        for stmt in statements {
            if let ControlFlow::Return(_) = self.execute(stmt)? {
                break;
            }
        }
        Ok(())
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
