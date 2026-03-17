use crate::value::Value;

#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Literal(Value),
    Ident(String),
    Binary {
        op: String,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Unary {
        op: String,
        expr: Box<Expr>,
    },
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
    },
    List(Vec<Expr>),
    Map(Vec<(Expr, Expr)>),
    Get {
        object: Box<Expr>,
        name: String,
    },
    OptionalGet {
        object: Box<Expr>,
        name: String,
    },
    Index {
        object: Box<Expr>,
        index: Box<Expr>,
    },
    OptionalIndex {
        object: Box<Expr>,
        index: Box<Expr>,
    },
    StringInterpolation(Vec<Expr>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Stmt {
    Import {
        module: String,
        path: Option<String>,
    },
    Let {
        name: String,
        type_annotation: Option<String>,
        init: Expr,
    },
    Assign {
        name: String,
        value: Expr,
    },
    If {
        condition: Expr,
        then_branch: Vec<Stmt>,
        else_branch: Option<Vec<Stmt>>,
    },
    While {
        condition: Expr,
        body: Vec<Stmt>,
    },
    For {
        var: String,
        iterable: Expr,
        body: Vec<Stmt>,
    },
    Return(Option<Expr>),
    FnDecl {
        name: String,
        params: Vec<(String, Option<String>)>,
        return_type: Option<String>,
        body: Vec<Stmt>,
    },
    Expr(Expr),
}
