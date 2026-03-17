use crate::ast::{Expr, Stmt};
use crate::environment::Environment;
use crate::value::Value;
use anyhow::anyhow;
use rusqlite::{Connection, params, types::ToSql};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Mutex;

pub struct Interpreter {
    pub globals: Rc<RefCell<Environment>>,
    pub environment: Rc<RefCell<Environment>>,
}

static DB_CONN: Mutex<Option<Connection>> = Mutex::new(None);

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

        // Register database functions
        let mut db_map = std::collections::HashMap::new();
        db_map.insert(
            "connect".to_string(),
            Value::Builtin(db_connect),
        );
        db_map.insert(
            "query".to_string(),
            Value::Builtin(db_query),
        );
        db_map.insert(
            "execute".to_string(),
            Value::Builtin(db_execute),
        );
        self.globals.borrow_mut().define("db".to_string(), Value::Map(db_map));
    }
}

// Helper function to convert Value to rusqlite type
fn value_to_sqlite(value: &Value) -> Box<dyn ToSql> {
    match value {
        Value::Null => Box::new(Option::<i64>::None),
        Value::Int(i) => Box::new(*i),
        Value::Float(f) => Box::new(*f),
        Value::String(s) => Box::new(s.clone()),
        Value::Bool(b) => Box::new(*b),
        _ => Box::new(Option::<i64>::None),
    }
}

// Helper function to convert rusqlite row to Value::Map
fn row_to_value(row: &rusqlite::Row) -> anyhow::Result<Value> {
    let mut map = std::collections::HashMap::new();
    for i in 0..row.as_ref().column_count() {
        let name = row.as_ref().column_name(i)?.to_string();
        let value = match row.get::<_, rusqlite::types::Value>(i) {
            Ok(rusqlite::types::Value::Null) => Value::Null,
            Ok(rusqlite::types::Value::Integer(i)) => Value::Int(i),
            Ok(rusqlite::types::Value::Real(f)) => Value::Float(f),
            Ok(rusqlite::types::Value::Text(s)) => Value::String(s),
            Ok(rusqlite::types::Value::Blob(b)) => Value::String(format!("<blob: {} bytes>", b.len())),
            Err(_) => Value::Null,
        };
        map.insert(name, value);
    }
    Ok(Value::Map(map))
}

// Database built-in function: db.connect(url)
fn db_connect(args: Vec<Value>) -> Value {
    if args.is_empty() {
        return Value::Null;
    }
    if let Value::String(url) = &args[0] {
        match Connection::open(url) {
            Ok(conn) => {
                let mut db_guard = DB_CONN.lock().unwrap();
                *db_guard = Some(conn);
                Value::Bool(true)
            }
            Err(_) => Value::Null,
        }
    } else {
        Value::Null
    }
}

// Database built-in function: db.query(sql, params)
fn db_query(args: Vec<Value>) -> Value {
    if args.is_empty() {
        return Value::Null;
    }
    if let Value::String(sql) = &args[0] {
        let params: Vec<Box<dyn ToSql>> = if args.len() > 1 {
            match &args[1] {
                Value::List(list) => list.iter().map(value_to_sqlite).collect(),
                _ => vec![value_to_sqlite(&args[1])],
            }
        } else {
            Vec::new()
        };

        let db_guard = DB_CONN.lock().unwrap();
        if let Some(conn) = db_guard.as_ref() {
            let params_refs: Vec<&dyn ToSql> = params.iter().map(|p| p.as_ref()).collect();

            match conn.prepare(sql) {
                Ok(mut stmt) => {
                    let rows = match stmt.query(params_refs.as_slice()) {
                        Ok(rows) => rows,
                        Err(_) => return Value::Null,
                    };

                    let mut results = Vec::new();
                    for row_result in rows {
                        match row_result {
                            Ok(row) => {
                                if let Ok(val) = row_to_value(&row) {
                                    results.push(val);
                                }
                            }
                            Err(_) => continue,
                        }
                    }
                    Value::List(results)
                }
                Err(_) => Value::Null,
            }
        } else {
            Value::Null
        }
    } else {
        Value::Null
    }
}

// Database built-in function: db.execute(sql, params)
fn db_execute(args: Vec<Value>) -> Value {
    if args.is_empty() {
        return Value::Null;
    }
    if let Value::String(sql) = &args[0] {
        let params: Vec<Box<dyn ToSql>> = if args.len() > 1 {
            match &args[1] {
                Value::List(list) => list.iter().map(value_to_sqlite).collect(),
                _ => vec![value_to_sqlite(&args[1])],
            }
        } else {
            Vec::new()
        };

        let db_guard = DB_CONN.lock().unwrap();
        if let Some(conn) = db_guard.as_ref() {
            let params_refs: Vec<&dyn ToSql> = params.iter().map(|p| p.as_ref()).collect();

            match conn.execute(sql, params_refs.as_slice()) {
                Ok(rows_affected) => Value::Int(rows_affected as i64),
                Err(_) => Value::Null,
            }
        } else {
            Value::Null
        }
    } else {
        Value::Null
    }
}

impl Interpreter {
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
                let callee_value = self.evaluate(callee)?;

                let mut evaluated_args = Vec::new();
                for arg in args {
                    evaluated_args.push(self.evaluate(arg)?);
                }

                match callee_value {
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
                    _ => Err(anyhow!("Value is not callable")),
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
