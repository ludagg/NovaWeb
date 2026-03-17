use crate::ast::{Expr, Stmt};
use crate::value::Value;
use pest::Parser;
use pest_derive::Parser;

#[derive(Parser)]
#[grammar = "grammar.pest"]
pub struct NovaParser;

pub fn parse(input: &str) -> anyhow::Result<Vec<Stmt>> {
    let pairs = NovaParser::parse(Rule::program, input)?;
    let mut statements = Vec::new();

    for pair in pairs {
        match pair.as_rule() {
            Rule::program => {
                for inner_pair in pair.into_inner() {
                    if inner_pair.as_rule() == Rule::statement {
                        statements.push(parse_statement(inner_pair.into_inner().next().unwrap())?);
                    }
                }
            }
            Rule::EOI => (),
            _ => unreachable!(),
        }
    }

    Ok(statements)
}

pub fn parse_expression_only(input: &str) -> anyhow::Result<Expr> {
    let pairs = NovaParser::parse(Rule::expr, input)?;
    for pair in pairs {
        if pair.as_rule() == Rule::expr {
            let mut inner = pair.into_inner();
            let mut current_expr = if let Some(p) = inner.next() {
                if p.as_rule() == Rule::prefix {
                    let op = p.as_str().to_string();
                    let primary = parse_primary(inner.next().unwrap())?;
                    Expr::Unary {
                        op,
                        expr: Box::new(primary),
                    }
                } else {
                    parse_primary(p)?
                }
            } else {
                return Err(anyhow::anyhow!("Empty expression"));
            };

            // Parse suffixes (get/index) for the current expression
            while let Some(suffix_pair) = inner.next() {
                if suffix_pair.as_rule() == Rule::suffix {
                    current_expr = parse_suffix(current_expr, suffix_pair)?;
                } else {
                    // Binary operator
                    let op = suffix_pair.as_str().to_string();
                    let next_primary_pair = inner.next().unwrap();
                    let mut next_expr = if next_primary_pair.as_rule() == Rule::prefix {
                        let op = next_primary_pair.as_str().to_string();
                        let primary = parse_primary(inner.next().unwrap())?;
                        Expr::Unary {
                            op,
                            expr: Box::new(primary),
                        }
                    } else {
                        parse_primary(next_primary_pair)?
                    };
                    // Parse suffixes for the right operand
                    while let Some(suffix_pair) = inner.next() {
                        if suffix_pair.as_rule() == Rule::suffix {
                            next_expr = parse_suffix(next_expr, suffix_pair)?;
                        } else {
                            break;
                        }
                    }
                    current_expr = Expr::Binary {
                        op,
                        left: Box::new(current_expr),
                        right: Box::new(next_expr),
                    };
                }
            }

            return Ok(current_expr);
        }
    }
    Err(anyhow::anyhow!("Failed to parse expression"))
}

fn parse_statement(pair: pest::iterators::Pair<Rule>) -> anyhow::Result<Stmt> {
    match pair.as_rule() {
        Rule::import_stmt => {
            let mut inner = pair.into_inner();
            let module = inner.next().unwrap().as_str().to_string();
            let path = inner.next().map(|p| p.into_inner().next().unwrap().as_str().to_string());
            Ok(Stmt::Import { module, path })
        }
        Rule::let_stmt => {
            let mut inner = pair.into_inner();
            let name = inner.next().unwrap().as_str().to_string();

            let mut type_annotation = None;
            let mut next = inner.next().unwrap();

            if next.as_rule() == Rule::type_annotation {
                type_annotation = Some(next.as_str().to_string());
                next = inner.next().unwrap();
            }

            let init = parse_expr(next)?;
            Ok(Stmt::Let { name, type_annotation, init })
        }
        Rule::assign_stmt => {
            let mut inner = pair.into_inner();
            let name = inner.next().unwrap().as_str().to_string();
            let value = parse_expr(inner.next().unwrap())?;
            Ok(Stmt::Assign { name, value })
        }
        Rule::if_stmt => {
            let mut inner = pair.into_inner();
            let condition = parse_expr(inner.next().unwrap())?;
            let then_branch = parse_block(inner.next().unwrap())?;
            let mut else_branch = None;
            if let Some(next) = inner.next() {
                match next.as_rule() {
                    Rule::block => {
                        else_branch = Some(parse_block(next)?);
                    }
                    Rule::if_stmt => {
                        else_branch = Some(vec![parse_statement(next)?]);
                    }
                    _ => unreachable!(),
                }
            }
            Ok(Stmt::If {
                condition,
                then_branch,
                else_branch,
            })
        }
        Rule::while_stmt => {
            let mut inner = pair.into_inner();
            let condition = parse_expr(inner.next().unwrap())?;
            let body = parse_block(inner.next().unwrap())?;
            Ok(Stmt::While { condition, body })
        }
        Rule::for_stmt => {
            let mut inner = pair.into_inner();
            let var = inner.next().unwrap().as_str().to_string();
            let iterable = parse_expr(inner.next().unwrap())?;
            let body = parse_block(inner.next().unwrap())?;
            Ok(Stmt::For {
                var,
                iterable,
                body,
            })
        }
        Rule::return_stmt => {
            let mut inner = pair.into_inner();
            let expr = inner.next().map(parse_expr).transpose()?;
            Ok(Stmt::Return(expr))
        }
        Rule::fn_decl => {
            let mut inner = pair.into_inner();

            // Skip decorators for now
            let mut next = inner.next().unwrap();
            while next.as_rule() == Rule::decorator {
                next = inner.next().unwrap();
            }

            let name = next.as_str().to_string();
            let mut params = Vec::new();

            next = inner.next().unwrap();
            while next.as_rule() == Rule::fn_param {
                let mut param_inner = next.into_inner();
                let param_name = param_inner.next().unwrap().as_str().to_string();
                let param_type = param_inner.next().map(|t| t.as_str().to_string());
                params.push((param_name, param_type));
                next = inner.next().unwrap();
            }

            let mut return_type = None;
            if next.as_rule() == Rule::ident { // Return type part
                 let mut ret_type_str = next.as_str().to_string();
                 let mut peek_iter = inner.clone();
                 while let Some(n) = peek_iter.next() {
                    if n.as_rule() == Rule::ident {
                        ret_type_str.push_str(" | ");
                        ret_type_str.push_str(inner.next().unwrap().as_str());
                    } else {
                        break;
                    }
                 }
                 return_type = Some(ret_type_str);
                 next = inner.next().unwrap();
            }

            let body = parse_block(next)?;
            Ok(Stmt::FnDecl { name, params, return_type, body })
        }
        Rule::expr_stmt => {
            let expr = parse_expr(pair.into_inner().next().unwrap())?;
            Ok(Stmt::Expr(expr))
        }
        _ => unreachable!("{:?}", pair.as_rule()),
    }
}

fn parse_block(pair: pest::iterators::Pair<Rule>) -> anyhow::Result<Vec<Stmt>> {
    let mut stmts = Vec::new();
    for inner in pair.into_inner() {
        if inner.as_rule() == Rule::statement {
            stmts.push(parse_statement(inner.into_inner().next().unwrap())?);
        }
    }
    Ok(stmts)
}

fn parse_expr(pair: pest::iterators::Pair<Rule>) -> anyhow::Result<Expr> {
    let mut inner = pair.into_inner();
    let mut current_expr = if let Some(p) = inner.next() {
        if p.as_rule() == Rule::prefix {
            let op = p.as_str().to_string();
            let primary = parse_primary(inner.next().unwrap())?;
            Expr::Unary {
                op,
                expr: Box::new(primary),
            }
        } else {
            parse_primary(p)?
        }
    } else {
        return Err(anyhow::anyhow!("Empty expression"));
    };

    // Parse suffixes (get/index) for the current expression
    while let Some(suffix_pair) = inner.next() {
        if suffix_pair.as_rule() == Rule::suffix {
            current_expr = parse_suffix(current_expr, suffix_pair)?;
        } else {
            // Binary operator
            let op = suffix_pair.as_str().to_string();
            let next_primary_pair = inner.next().unwrap();
            let mut next_expr = if next_primary_pair.as_rule() == Rule::prefix {
                let op = next_primary_pair.as_str().to_string();
                let primary = parse_primary(inner.next().unwrap())?;
                Expr::Unary {
                    op,
                    expr: Box::new(primary),
                }
            } else {
                parse_primary(next_primary_pair)?
            };
            // Parse suffixes for the right operand
            while let Some(suffix_pair) = inner.next() {
                if suffix_pair.as_rule() == Rule::suffix {
                    next_expr = parse_suffix(next_expr, suffix_pair)?;
                } else {
                    // This is actually the next binary operator, put it back
                    // We can't put it back, so we need to handle this differently
                    // For now, break and continue with the binary operation
                    break;
                }
            }
            current_expr = Expr::Binary {
                op,
                left: Box::new(current_expr),
                right: Box::new(next_expr),
            };
        }
    }

    Ok(current_expr)
}

fn parse_suffix(object: Expr, pair: pest::iterators::Pair<Rule>) -> anyhow::Result<Expr> {
    let suffix_str = pair.as_str().to_string();
    let mut inner = pair.into_inner();
    let first = inner.next().unwrap();
    match first.as_rule() {
        Rule::ident => {
            if suffix_str.starts_with("?.") {
                Ok(Expr::OptionalGet {
                    object: Box::new(object),
                    name: first.as_str().to_string(),
                })
            } else {
                Ok(Expr::Get {
                    object: Box::new(object),
                    name: first.as_str().to_string(),
                })
            }
        }
        Rule::expr => {
            // This could be either a call or an index
            // Both have expr as the first element
            // We need to check if there are more expr elements or check the pair's string
            if suffix_str.starts_with("?[") {
                Ok(Expr::OptionalIndex {
                    object: Box::new(object),
                    index: Box::new(parse_expr(first)?),
                })
            } else if suffix_str.starts_with('[') {
                // This is an index: [expr]
                Ok(Expr::Index {
                    object: Box::new(object),
                    index: Box::new(parse_expr(first)?),
                })
            } else {
                // This is a call: (expr, expr, ...)
                let mut args = Vec::new();
                args.push(parse_expr(first)?);
                for arg_pair in inner {
                    if arg_pair.as_rule() == Rule::expr {
                        args.push(parse_expr(arg_pair)?);
                    }
                }
                Ok(Expr::Call {
                    callee: Box::new(object),
                    args,
                })
            }
        }
        _ => {
            // Empty parentheses: obj()
            Ok(Expr::Call {
                callee: Box::new(object),
                args: Vec::new(),
            })
        }
    }
}

fn parse_primary(pair: pest::iterators::Pair<Rule>) -> anyhow::Result<Expr> {
    match pair.as_rule() {
        Rule::primary => parse_primary(pair.into_inner().next().unwrap()),
        Rule::literal => parse_literal(pair.into_inner().next().unwrap()),
        Rule::ident => Ok(Expr::Ident(pair.as_str().to_string())),
        Rule::expr => parse_expr(pair),
        _ => unreachable!("{:?}", pair.as_rule()),
    }
}

fn parse_literal(pair: pest::iterators::Pair<Rule>) -> anyhow::Result<Expr> {
    match pair.as_rule() {
        Rule::int => Ok(Expr::Literal(Value::Int(pair.as_str().parse()?))),
        Rule::float => Ok(Expr::Literal(Value::Float(pair.as_str().parse()?))),
        Rule::bool => Ok(Expr::Literal(Value::Bool(pair.as_str() == "true"))),
        Rule::string => {
            let inner = pair.into_inner().next().unwrap().as_str();
            Ok(Expr::Literal(Value::String(inner.to_string())))
        }
        Rule::fstring => {
            let mut parts = Vec::new();
            for inner in pair.into_inner() {
                match inner.as_rule() {
                    Rule::fstring_inner => {
                        let inner_most = inner.into_inner().next().unwrap();
                        match inner_most.as_rule() {
                            Rule::fstring_text => {
                                parts.push(Expr::Literal(Value::String(inner_most.as_str().to_string())));
                            }
                            Rule::fstring_expr => {
                                parts.push(parse_expr(inner_most.into_inner().next().unwrap())?);
                            }
                            _ => unreachable!("{:?}", inner_most.as_rule()),
                        }
                    }
                    _ => unreachable!("{:?}", inner.as_rule()),
                }
            }
            Ok(Expr::StringInterpolation(parts))
        }
        Rule::list => {
            let mut elements = Vec::new();
            for p in pair.into_inner() {
                elements.push(parse_expr(p)?);
            }
            Ok(Expr::List(elements))
        }
        Rule::map => {
            let mut pairs = Vec::new();
            for pair_pair in pair.into_inner() {
                let mut inner = pair_pair.into_inner();
                let key_pair = inner.next().unwrap();
                let key = if key_pair.as_rule() == Rule::string {
                    parse_literal(key_pair)?
                } else {
                    Expr::Literal(Value::String(key_pair.as_str().to_string()))
                };
                let val = parse_expr(inner.next().unwrap())?;
                pairs.push((key, val));
            }
            Ok(Expr::Map(pairs))
        }
        Rule::null => Ok(Expr::Literal(Value::Null)),
        _ => unreachable!("{:?}", pair.as_rule()),
    }
}
