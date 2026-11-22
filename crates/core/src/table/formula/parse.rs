//! 公式 source → `Node`: recursive descent, loosest operators first.
//! Comparison, `&`, `+ -`, `* / %`, unary `- +`, `^` (right to left).

use super::funcs;
use super::lex::{Bad, Lexed, Tok, lex};
use super::{Bin, Node};
use crate::table::model::*;

/// Brackets, calls and signs nested deeper than this are refused (the parser recurses).
const MAX_DEPTH: usize = 64;

const LEVELS: &[&[(&str, Bin)]] = &[
    &[("=", Bin::Eq), ("!=", Bin::Ne), ("<", Bin::Lt), ("<=", Bin::Le), (">", Bin::Gt), (">=", Bin::Ge)],
    &[("&", Bin::Cat)],
    &[("+", Bin::Add), ("-", Bin::Sub)],
    &[("*", Bin::Mul), ("/", Bin::Div), ("%", Bin::Mod)],
];

struct Parser<'a> {
    toks: &'a [Lexed],
    i: usize,
    table: &'a Table,
    /// The source's length: where an error past the last token points.
    len: usize,
    depth: usize,
}

pub(super) fn parse(src: &str, table: &Table) -> Result<Node, Bad> {
    let (toks, bad) = lex(src);
    if let Some(b) = bad { return Err(b); }
    let mut p = Parser { toks: &toks, i: 0, table, len: src.len(), depth: 0 };
    // a leading `=`, the way a spreadsheet formula starts
    if p.peek() == Some(&Tok::Op("=")) { p.i += 1; }
    if p.peek().is_none() { return Err(Bad { start: 0, end: src.len(), msg: "公式为空".into() }); }
    let n = p.expr()?;
    match p.peek() {
        None => Ok(n),
        Some(Tok::RParen) => Err(p.bad(p.i, "多余的右括号")),
        Some(Tok::Comma) => Err(p.bad(p.i, "逗号只能用在函数的参数之间")),
        Some(_) => Err(p.bad(p.i, "缺少运算符")),
    }
}

impl Parser<'_> {
    fn peek(&self) -> Option<&Tok> { self.toks.get(self.i).map(|t| &t.tok) }

    /// An error at token `i` (at the end of the source when there is none).
    fn bad(&self, i: usize, msg: impl Into<String>) -> Bad {
        let (start, end) = self.toks.get(i).map_or((self.len, self.len), |t| (t.start, t.end));
        Bad { start, end, msg: msg.into() }
    }

    fn deeper(&mut self, f: fn(&mut Self) -> Result<Node, Bad>) -> Result<Node, Bad> {
        self.depth += 1;
        if self.depth > MAX_DEPTH { return Err(self.bad(self.i, "公式嵌套太深")); }
        let n = f(self);
        self.depth -= 1;
        n
    }

    fn expr(&mut self) -> Result<Node, Bad> { self.binary(0) }

    fn binary(&mut self, level: usize) -> Result<Node, Bad> {
        let Some(ops) = LEVELS.get(level) else { return self.unary() };
        let mut left = self.binary(level + 1)?;
        while let Some(Tok::Op(o)) = self.peek() && let Some((_, op)) = ops.iter().find(|(s, _)| s == o) {
            self.i += 1;
            left = Node::Bin(*op, Box::new(left), Box::new(self.binary(level + 1)?));
        }
        Ok(left)
    }

    fn unary(&mut self) -> Result<Node, Bad> {
        let Some(Tok::Op(op @ ("-" | "+"))) = self.peek() else { return self.power() };
        let neg = *op == "-";
        self.i += 1;
        let x = self.deeper(Self::unary)?;
        Ok(match x {
            x if !neg => x,
            Node::Lit(Value::Number(n)) => Node::Lit(Value::Number(-n)),
            x => Node::Neg(Box::new(x)),
        })
    }

    fn power(&mut self) -> Result<Node, Bad> {
        let base = self.primary()?;
        if self.peek() != Some(&Tok::Op("^")) { return Ok(base); }
        self.i += 1;
        // `2^-1`, and `2^3^2` is `2^(3^2)`
        Ok(Node::Bin(Bin::Pow, Box::new(base), Box::new(self.deeper(Self::unary)?)))
    }

    fn primary(&mut self) -> Result<Node, Bad> {
        let at = self.i;
        let Some(t) = self.toks.get(at) else { return Err(self.bad(at, "公式不完整：缺少运算数")) };
        self.i += 1;
        match &t.tok {
            Tok::Num(n) => Ok(Node::Lit(Value::Number(*n))),
            Tok::Str(s) => Ok(Node::Lit(Value::Text(s.clone()))),
            Tok::Ref(name) => match self.table.field_named(name).or_else(|| self.table.field_named(name.trim())) {
                Some(f) => Ok(Node::Field(f.id)),
                None => Err(self.bad(at, format!("未知字段：{name}"))),
            },
            Tok::Ident(name) if self.peek() == Some(&Tok::LParen) => self.call(name, at),
            Tok::Ident(name) => match self.table.field_named(name) {
                Some(f) => Ok(Node::Field(f.id)),
                None if name.eq_ignore_ascii_case("true") => Ok(Node::Lit(Value::Bool(true))),
                None if name.eq_ignore_ascii_case("false") => Ok(Node::Lit(Value::Bool(false))),
                None if funcs::lookup(name).is_some() => Err(self.bad(at, format!("函数 {} 后面缺少括号", name.to_ascii_uppercase()))),
                None => Err(self.bad(at, format!("未知字段：{name}"))),
            },
            Tok::LParen => {
                let n = self.deeper(Self::expr)?;
                if self.peek() != Some(&Tok::RParen) { return Err(self.bad(if self.i < self.toks.len() { self.i } else { at }, "缺少右括号")); }
                self.i += 1;
                Ok(n)
            }
            Tok::RParen if at > 0 && self.toks[at - 1].tok == Tok::LParen => Err(self.bad(at, "括号里是空的")),
            Tok::RParen | Tok::Comma | Tok::Op(_) => Err(self.bad(at, "缺少运算数")),
        }
    }

    /// `name(…)`: the cursor is on the `(`.
    fn call(&mut self, name: &str, at: usize) -> Result<Node, Bad> {
        let Some((f, min, max)) = funcs::lookup(name) else { return Err(self.bad(at, format!("未知函数：{}", name.to_uppercase()))) };
        self.i += 1;
        let mut args = vec![];
        if self.peek() == Some(&Tok::RParen) {
            self.i += 1;
        } else {
            loop {
                if self.peek().is_none() { return Err(self.bad(at + 1, "缺少右括号")); }
                // an argument left out (`IF(a,,b)`) is empty
                args.push(if matches!(self.peek(), Some(Tok::Comma | Tok::RParen)) { Node::Lit(Value::Empty) } else { self.deeper(Self::expr)? });
                self.i += 1;
                match self.toks.get(self.i - 1).map(|t| &t.tok) {
                    Some(Tok::Comma) => {}
                    Some(Tok::RParen) => break,
                    None => return Err(self.bad(at + 1, "缺少右括号")),
                    Some(_) => return Err(self.bad(self.i - 1, "缺少逗号或右括号")),
                }
            }
        }
        match funcs::arity_error(name, f, min, max, args.len()) {
            Some(msg) => Err(Bad { start: self.toks[at].start, end: self.toks[self.i - 1].end, msg }),
            None => Ok(Node::Call(f, args)),
        }
    }
}
