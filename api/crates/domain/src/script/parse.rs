//! Statements and expressions of the script language (see the module docs
//! of `script`), parsed line by line with recursive descent.

use super::VarScope;
use super::lex::{Tok, strip_comment, tokens};
use serde_json::Value;

/// Nesting limit of an expression.
const MAX_DEPTH: usize = 48;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Op {
    Add,
    And,
    Or,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Contains,
    NotContains,
    Matches,
    StartsWith,
    EndsWith,
}

impl Op {
    pub(super) const fn text(self) -> &'static str {
        match self {
            Self::Add => "+",
            Self::And => "and",
            Self::Or => "or",
            Self::Eq => "==",
            Self::Ne => "!=",
            Self::Lt => "<",
            Self::Le => "<=",
            Self::Gt => ">",
            Self::Ge => ">=",
            Self::Contains => "contains",
            Self::NotContains => "not contains",
            Self::Matches => "matches",
            Self::StartsWith => "startswith",
            Self::EndsWith => "endswith",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum Expr {
    Lit(Value),
    /// A string literal; `{{variables}}` are resolved when evaluated.
    Str(String),
    /// `status`, `time`, `size`, `body`, `json`, `headers`.
    Root(&'static str),
    /// `env.x`, `global.x`, `var.x`, `vars.x`: the variable's current value.
    Var(String),
    Field(Box<Expr>, String),
    Index(Box<Expr>, Box<Expr>),
    Call(String, Vec<Expr>),
    Not(Box<Expr>),
    Neg(Box<Expr>),
    Bin(Op, Box<Expr>, Box<Expr>),
    Exists(Box<Expr>, bool),
    Is(Box<Expr>, String, bool),
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum Stmt {
    Set(VarScope, String, Expr),
    Unset(VarScope, String),
    /// A named check (`test "name": expr`, or `assert expr` named after its text).
    Test(String, Expr),
    Log(Expr),
}

const ROOTS: [&str; 6] = ["status", "time", "size", "body", "json", "headers"];
const VAR_ROOTS: [&str; 4] = ["env", "global", "var", "vars"];

/// One statement per non-blank line; `None` for blank / comment lines.
pub(super) fn statement(line: &str) -> Result<Option<Stmt>, String> {
    let line = strip_comment(line).trim();
    if line.is_empty() {
        return Ok(None);
    }
    let (word, rest) = line
        .split_once(char::is_whitespace)
        .map_or((line, ""), |(w, r)| (w, r.trim()));
    let stmt = match word {
        "set" => {
            let at = assignment(rest).ok_or("expected `set scope.name = value`")?;
            let (scope, name) = target(&rest[..at])?;
            Stmt::Set(scope, name, expression(&rest[at + 1..])?)
        }
        "unset" => {
            let (scope, name) = target(rest)?;
            Stmt::Unset(scope, name)
        }
        "test" => {
            let mut toks = tokens(rest)?;
            let (Some(Tok::Str(name)), Some(Tok::P(":"))) = (toks.first(), toks.get(1)) else {
                return Err("expected `test \"name\": expression`".into());
            };
            let name = name.clone();
            toks.drain(..2);
            Stmt::Test(name, parse_tokens(toks)?)
        }
        "assert" => Stmt::Test(rest.to_owned(), expression(rest)?),
        "log" => Stmt::Log(expression(rest)?),
        other => {
            return Err(format!(
                "unknown statement `{other}` (use set, unset, test, assert or log)"
            ));
        }
    };
    Ok(Some(stmt))
}

/// Byte index of the assignment `=` (not part of `==`, `!=`, `<=`, `>=`),
/// outside quotes.
fn assignment(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut quote = None;
    for (i, &b) in bytes.iter().enumerate() {
        match quote {
            Some(q) if b == q => quote = None,
            Some(_) => {}
            None if b == b'"' || b == b'\'' => quote = Some(b),
            None if b == b'=' => {
                let prev = i.checked_sub(1).and_then(|p| bytes.get(p)).copied();
                let next = bytes.get(i + 1).copied();
                if next != Some(b'=') && !matches!(prev, Some(b'=' | b'!' | b'<' | b'>')) {
                    return Some(i);
                }
            }
            None => {}
        }
    }
    None
}

/// `env.name` / `global.name` / `var.name` (the name may be quoted).
fn target(s: &str) -> Result<(VarScope, String), String> {
    let s = s.trim();
    let (scope, name) = s
        .split_once('.')
        .ok_or("expected env.name, global.name or var.name")?;
    let scope = match scope.trim() {
        "env" => VarScope::Environment,
        "global" | "globals" => VarScope::Global,
        "var" | "vars" => VarScope::Run,
        other => return Err(format!("unknown scope `{other}` (use env, global or var)")),
    };
    let name = name.trim();
    let name = name
        .strip_prefix('"')
        .and_then(|n| n.strip_suffix('"'))
        .or_else(|| name.strip_prefix('\'').and_then(|n| n.strip_suffix('\'')))
        .unwrap_or(name);
    if name.is_empty() {
        return Err("missing variable name".into());
    }
    Ok((scope, name.to_owned()))
}

fn expression(s: &str) -> Result<Expr, String> {
    parse_tokens(tokens(s)?)
}

fn parse_tokens(toks: Vec<Tok>) -> Result<Expr, String> {
    if toks.is_empty() {
        return Err("missing expression".into());
    }
    let mut p = Parser {
        toks,
        pos: 0,
        depth: 0,
    };
    let e = p.or()?;
    match p.peek() {
        None => Ok(e),
        Some(t) => Err(format!("unexpected {}", describe(t))),
    }
}

fn describe(t: &Tok) -> String {
    match t {
        Tok::Num(n) => format!("number {n}"),
        Tok::Str(s) => format!("string \"{s}\""),
        Tok::Ident(i) => format!("`{i}`"),
        Tok::P(p) => format!("`{p}`"),
    }
}

struct Parser {
    toks: Vec<Tok>,
    pos: usize,
    depth: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }

    fn word(&self, offset: usize) -> Option<&str> {
        match self.toks.get(self.pos + offset) {
            Some(Tok::Ident(w)) => Some(w),
            _ => None,
        }
    }

    fn eat_p(&mut self, p: &str) -> bool {
        if matches!(self.peek(), Some(Tok::P(q)) if *q == p) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn eat_word(&mut self, w: &str) -> bool {
        if self.word(0) == Some(w) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn nest(&mut self) -> Result<(), String> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err("expression nested too deeply".into());
        }
        Ok(())
    }

    fn or(&mut self) -> Result<Expr, String> {
        let mut left = self.and()?;
        while self.eat_word("or") || self.eat_p("||") {
            left = Expr::Bin(Op::Or, Box::new(left), Box::new(self.and()?));
        }
        Ok(left)
    }

    fn and(&mut self) -> Result<Expr, String> {
        let mut left = self.not()?;
        while self.eat_word("and") || self.eat_p("&&") {
            left = Expr::Bin(Op::And, Box::new(left), Box::new(self.not()?));
        }
        Ok(left)
    }

    fn not(&mut self) -> Result<Expr, String> {
        if self.eat_word("not") || self.eat_p("!") {
            self.nest()?;
            let e = self.not()?;
            self.depth -= 1;
            return Ok(Expr::Not(Box::new(e)));
        }
        self.comparison()
    }

    fn comparison(&mut self) -> Result<Expr, String> {
        let left = self.sum()?;
        if self.eat_word("exists") {
            return Ok(Expr::Exists(Box::new(left), true));
        }
        if self.word(0) == Some("not") && self.word(1) == Some("exists") {
            self.pos += 2;
            return Ok(Expr::Exists(Box::new(left), false));
        }
        if self.eat_word("is") {
            let negate = self.eat_word("not");
            let ty = self.word(0).ok_or("expected a type after `is`")?.to_owned();
            self.pos += 1;
            return Ok(Expr::Is(Box::new(left), ty, !negate));
        }
        let op = match (self.peek(), self.word(1)) {
            (Some(Tok::P("==")), _) => Op::Eq,
            (Some(Tok::P("!=")), _) => Op::Ne,
            (Some(Tok::P("<")), _) => Op::Lt,
            (Some(Tok::P("<=")), _) => Op::Le,
            (Some(Tok::P(">")), _) => Op::Gt,
            (Some(Tok::P(">=")), _) => Op::Ge,
            (Some(Tok::Ident(w)), _) if w == "contains" => Op::Contains,
            (Some(Tok::Ident(w)), Some("contains")) if w == "not" => {
                self.pos += 1;
                Op::NotContains
            }
            (Some(Tok::Ident(w)), _) if w == "matches" => Op::Matches,
            (Some(Tok::Ident(w)), _) if w == "startswith" => Op::StartsWith,
            (Some(Tok::Ident(w)), _) if w == "endswith" => Op::EndsWith,
            _ => return Ok(left),
        };
        self.pos += 1;
        let right = self.sum()?;
        Ok(Expr::Bin(op, Box::new(left), Box::new(right)))
    }

    fn sum(&mut self) -> Result<Expr, String> {
        let mut left = self.unary()?;
        while self.eat_p("+") {
            left = Expr::Bin(Op::Add, Box::new(left), Box::new(self.unary()?));
        }
        Ok(left)
    }

    fn unary(&mut self) -> Result<Expr, String> {
        if self.eat_p("-") {
            self.nest()?;
            let e = self.unary()?;
            self.depth -= 1;
            return Ok(Expr::Neg(Box::new(e)));
        }
        self.postfix()
    }

    fn postfix(&mut self) -> Result<Expr, String> {
        let mut e = self.primary()?;
        loop {
            if self.eat_p(".") {
                let Some(name) = self.word(0).map(str::to_owned) else {
                    return Err("expected a name after `.`".into());
                };
                self.pos += 1;
                e = Expr::Field(Box::new(e), name);
            } else if self.eat_p("[") {
                self.nest()?;
                let index = self.or()?;
                self.depth -= 1;
                if !self.eat_p("]") {
                    return Err("expected `]`".into());
                }
                e = Expr::Index(Box::new(e), Box::new(index));
            } else {
                return Ok(e);
            }
        }
    }

    fn primary(&mut self) -> Result<Expr, String> {
        let Some(tok) = self.peek().cloned() else {
            return Err("unexpected end of expression".into());
        };
        self.pos += 1;
        match tok {
            Tok::Num(n) => Ok(Expr::Lit(super::eval::number(n))),
            Tok::Str(s) => Ok(Expr::Str(s)),
            Tok::P("(") => {
                self.nest()?;
                let e = self.or()?;
                self.depth -= 1;
                if !self.eat_p(")") {
                    return Err("expected `)`".into());
                }
                Ok(e)
            }
            Tok::Ident(w) => self.ident(w),
            Tok::P(p) => Err(format!("unexpected `{p}`")),
        }
    }

    fn ident(&mut self, w: String) -> Result<Expr, String> {
        match w.as_str() {
            "true" => return Ok(Expr::Lit(Value::Bool(true))),
            "false" => return Ok(Expr::Lit(Value::Bool(false))),
            "null" => return Ok(Expr::Lit(Value::Null)),
            _ => {}
        }
        if let Some(root) = ROOTS.iter().find(|r| **r == w) {
            return Ok(Expr::Root(root));
        }
        if VAR_ROOTS.contains(&w.as_str()) {
            // `env.name`, or `env["odd-name"]` for names that are not words.
            let name = if self.eat_p(".") {
                let name = self.word(0).map(str::to_owned);
                if name.is_some() {
                    self.pos += 1;
                }
                name
            } else if self.eat_p("[") {
                match self.peek().cloned() {
                    Some(Tok::Str(s)) => {
                        self.pos += 1;
                        if !self.eat_p("]") {
                            return Err("expected `]`".into());
                        }
                        Some(s)
                    }
                    _ => None,
                }
            } else {
                None
            };
            return name
                .map(Expr::Var)
                .ok_or_else(|| format!("expected `{w}.name`"));
        }
        if self.eat_p("(") {
            let mut args = Vec::new();
            if !self.eat_p(")") {
                loop {
                    self.nest()?;
                    args.push(self.or()?);
                    self.depth -= 1;
                    if self.eat_p(")") {
                        break;
                    }
                    if !self.eat_p(",") {
                        return Err("expected `,` or `)`".into());
                    }
                }
            }
            return Ok(Expr::Call(w, args));
        }
        Err(format!(
            "unknown name `{w}` (use status, time, size, body, json, headers, env.x, var.x or a function)"
        ))
    }
}
