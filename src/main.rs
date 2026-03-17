mod ast;
mod environment;
mod interpreter;
mod parser;
mod template;
mod value;

use crate::interpreter::Interpreter;
use crate::value::Value;
use clap::{Parser, Subcommand};
use std::fs;

#[derive(Parser)]
#[command(name = "novaw")]
#[command(about = "NovaWeb Programming Language CLI", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Run { path: String },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Run { path } => {
            let content = fs::read_to_string(path)?;
            let statements = parser::parse(&content)?;

            let mut interp = Interpreter::new();

            // Register built-ins
            interp.globals.borrow_mut().define(
                "print".to_string(),
                Value::Builtin(|args| {
                    for arg in args {
                        print!("{} ", arg);
                    }
                    println!();
                    Value::Null
                }),
            );

            interp.globals.borrow_mut().define(
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

            interp.globals.borrow_mut().define(
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

            interp.globals.borrow_mut().define(
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

            interp.interpret(&statements)?;
        }
    }

    Ok(())
}
