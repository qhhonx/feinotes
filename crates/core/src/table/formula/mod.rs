//! 公式: an Excel-style expression over one record's fields, the way 飞书's
//! and Airtable's formulas read. `{字段}`, `[字段]` or the bare name is a
//! field; operators, loosest first: comparison (`= != <> < <= > >=`), `&`,
//! `+ -`, `* / %`, unary `- +`, `^`. Field names are resolved to ids when
//! the formula is compiled, so evaluating never looks at the source again.

mod eval;
mod funcs;
mod lex;
mod parse;
#[cfg(test)]
mod tests;

use super::model::*;
use funcs::F;
use lex::{Lexed, Tok};

/// A parsed formula, field references resolved to ids.
#[derive(Clone, Debug, PartialEq)]
pub struct Expr(pub(crate) Node);

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Node {
    Lit(Value),
    Field(FieldId),
    Neg(Box<Node>),
    Bin(Bin, Box<Node>, Box<Node>),
    Call(F, Vec<Node>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Bin { Eq, Ne, Lt, Le, Gt, Ge, Cat, Add, Sub, Mul, Div, Mod, Pow }

/// What a formula is computed over: one record of one table.
pub struct Ctx<'a> {
    pub base: &'a Base,
    pub table: &'a Table,
    pub record: &'a Record,
    /// Wall-clock milliseconds (`NOW()`, `TODAY()`).
    pub now: i64,
}

/// One function of the language, for the editor's list.
pub struct FnDoc { pub name: &'static str, pub group: &'static str, pub sig: &'static str, pub desc: &'static str }

/// Parse `src` against `table`'s field names. Err: a message for the editor.
pub fn compile(src: &str, table: &Table) -> Result<Expr, String> { parse::parse(src, table).map(Expr).map_err(|b| b.msg) }

/// What is wrong with `src`, if anything: the bytes of the source at fault, and the message `compile` gives.
pub fn check(src: &str, table: &Table) -> Option<(usize, usize, String)> { parse::parse(src, table).err().map(|b| (b.start, b.end, b.msg)) }

/// The fields of the record's own table the formula reads.
pub fn deps(e: &Expr) -> Vec<FieldId> {
    fn walk(n: &Node, out: &mut Vec<FieldId>) {
        match n {
            Node::Lit(_) => {}
            Node::Field(id) => if !out.contains(id) { out.push(*id); },
            Node::Neg(x) => walk(x, out),
            Node::Bin(_, a, b) => { walk(a, out); walk(b, out); }
            Node::Call(_, args) => for a in args { walk(a, out); },
        }
    }
    let mut out = vec![];
    walk(&e.0, &mut out);
    out
}

/// The formula's value for `ctx.record`. What can't be computed is a
/// `Value::Error`: `#DIV/0!`, `#VALUE!` (a value of the wrong kind), `#NUM!`
/// (a number out of range), `#REF!` (a field that is gone).
pub fn eval(e: &Expr, ctx: &Ctx) -> Value {
    match eval::eval(&e.0, ctx) {
        Value::Number(n) if !n.is_finite() => Value::Error(eval::E_NUM.into()),
        v => v,
    }
}

/// `src` after field `old` was renamed `new`: only references change
/// (`{old}`, `[old]`, a bare `old`), text in quotes and everything else stays.
pub fn rename_field(src: &str, old: &str, new: &str) -> String {
    let (toks, _) = lex::lex(src);
    let braces = || if new.contains('}') && !new.contains(']') { format!("[{new}]") } else { format!("{{{new}}}") };
    let mut out = String::with_capacity(src.len() + new.len());
    let mut at = 0;
    for (i, t) in toks.iter().enumerate() {
        let with = match &t.tok {
            Tok::Ref(n) if n == old || n.trim() == old => if src[t.start..].starts_with('[') && !new.contains(']') { format!("[{new}]") } else { braces() },
            Tok::Ident(n) if n == old && !matches!(toks.get(i + 1), Some(Lexed { tok: Tok::LParen, .. })) => if lex::bare(new) { new.to_string() } else { braces() },
            _ => continue,
        };
        out.push_str(&src[at..t.start]);
        out.push_str(&with);
        at = t.end;
    }
    out.push_str(&src[at..]);
    out
}

pub fn functions() -> &'static [FnDoc] { funcs::docs() }
