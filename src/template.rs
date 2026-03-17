use crate::ast::Expr;
use crate::interpreter::Interpreter;
use crate::value::Value;
use anyhow::anyhow;
use pest::Parser;
use pest_derive::Parser;
use std::collections::HashMap;

#[derive(Parser)]
#[grammar = "template.pest"]
pub struct TemplateParser;

#[derive(Debug, Clone)]
pub enum TemplateNode {
    Text(String),
    Expr(Expr),
    If {
        condition: Expr,
        then_nodes: Vec<TemplateNode>,
        else_nodes: Vec<TemplateNode>,
    },
    For {
        var: String,
        iterable: Expr,
        body: Vec<TemplateNode>,
    },
}

pub fn parse_template(input: &str) -> anyhow::Result<Vec<TemplateNode>> {
    let pairs = TemplateParser::parse(Rule::template, input)?;
    let mut nodes = Vec::new();

    for pair in pairs {
        match pair.as_rule() {
            Rule::template => {
                for inner_pair in pair.into_inner() {
                    match inner_pair.as_rule() {
                        Rule::node => {
                            nodes.push(parse_node(inner_pair.into_inner().next().unwrap())?);
                        }
                        Rule::text => {
                            nodes.push(TemplateNode::Text(inner_pair.as_str().to_string()));
                        }
                        _ => {}
                    }
                }
            }
            Rule::EOI => (),
            _ => unreachable!("{:?}", pair.as_rule()),
        }
    }

    Ok(nodes)
}

fn parse_node(pair: pest::iterators::Pair<Rule>) -> anyhow::Result<TemplateNode> {
    match pair.as_rule() {
        Rule::expr_node => {
            let mut inner = pair.into_inner();
            let expr_pair = inner.next().unwrap();
            Ok(TemplateNode::Expr(parse_template_expr(expr_pair)?))
        }
        Rule::if_node => {
            let mut inner = pair.into_inner();
            let condition = parse_template_expr(inner.next().unwrap())?;

            let mut then_nodes = Vec::new();
            let mut else_nodes = Vec::new();

            for node_pair in inner {
                match node_pair.as_rule() {
                    Rule::node => {
                        let node = parse_node(node_pair.into_inner().next().unwrap())?;
                        then_nodes.push(node);
                    }
                    Rule::text => {
                        then_nodes.push(TemplateNode::Text(node_pair.as_str().to_string()));
                    }
                    Rule::else_branch => {
                        for else_node_pair in node_pair.into_inner() {
                            match else_node_pair.as_rule() {
                                Rule::node => {
                                    let node = parse_node(else_node_pair.into_inner().next().unwrap())?;
                                    else_nodes.push(node);
                                }
                                Rule::text => {
                                    else_nodes.push(TemplateNode::Text(else_node_pair.as_str().to_string()));
                                }
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }
            }

            Ok(TemplateNode::If {
                condition,
                then_nodes,
                else_nodes,
            })
        }
        Rule::for_node => {
            let mut inner = pair.into_inner();
            let var = inner.next().unwrap().as_str().to_string();
            let iterable = parse_template_expr(inner.next().unwrap())?;

            let mut body = Vec::new();
            for node_pair in inner {
                match node_pair.as_rule() {
                    Rule::node => {
                        body.push(parse_node(node_pair.into_inner().next().unwrap())?);
                    }
                    Rule::text => {
                        body.push(TemplateNode::Text(node_pair.as_str().to_string()));
                    }
                    _ => {}
                }
            }

            Ok(TemplateNode::For { var, iterable, body })
        }
        _ => unreachable!("{:?}", pair.as_rule()),
    }
}

fn parse_template_expr(pair: pest::iterators::Pair<Rule>) -> anyhow::Result<Expr> {
    // Import parser functions from the main parser
    // For now, we'll use the main parser's parse_expr function
    let expr_str = pair.as_str();
    crate::parser::parse_expression_only(expr_str)
}

pub fn render(template: &str, context: &HashMap<String, Value>) -> String {
    match parse_template(template) {
        Ok(nodes) => render_nodes(&nodes, context),
        Err(_) => template.to_string(),
    }
}

pub fn render_nodes(nodes: &[TemplateNode], context: &HashMap<String, Value>) -> String {
    let mut result = String::new();
    let mut interp = Interpreter::new();

    // Define all context variables in the interpreter
    for (key, value) in context {
        interp.globals.borrow_mut().define(key.clone(), value.clone());
    }

    for node in nodes {
        result.push_str(&render_node(node, &mut interp));
    }

    result
}

fn render_node(node: &TemplateNode, interp: &mut Interpreter) -> String {
    match node {
        TemplateNode::Text(s) => s.clone(),
        TemplateNode::Expr(expr) => match interp.evaluate(expr) {
            Ok(value) => value.to_string(),
            Err(_) => String::new(),
        },
        TemplateNode::If {
            condition,
            then_nodes,
            else_nodes,
        } => {
            match interp.evaluate(condition) {
                Ok(value) if value.is_truthy() => render_nodes(then_nodes, &HashMap::new()),
                _ => render_nodes(else_nodes, &HashMap::new()),
            }
        }
        TemplateNode::For {
            var,
            iterable,
            body,
        } => {
            match interp.evaluate(iterable) {
                Ok(Value::List(items)) => {
                    let mut result = String::new();
                    for item in items {
                        let previous = interp.environment.clone();
                        interp.environment = std::rc::Rc::new(std::cell::RefCell::new(
                            crate::environment::Environment::with_enclosing(previous.clone()),
                        ));
                        interp.environment.borrow_mut().define(var.clone(), item);
                        result.push_str(&render_nodes(body, &HashMap::new()));
                        interp.environment = previous;
                    }
                    result
                }
                _ => String::new(),
            }
        }
    }
}
