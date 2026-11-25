//! 公式 evaluation: what a field reads as, how values turn into numbers,
//! text and dates, the operators, and the functions.

use super::funcs::F;
use super::{Bin, Ctx, Node};
use crate::table::field::{self, date_ms, date_parts, plain_number};
use crate::table::model::*;
use crate::time::{days_from_civil, days_in_month};
use std::borrow::Cow;
use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fmt::Write;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

pub(super) const E_VALUE: &str = "#VALUE!";
pub(super) const E_NUM: &str = "#NUM!";
pub(super) const E_DIV0: &str = "#DIV/0!";
pub(super) const E_REF: &str = "#REF!";
pub(super) const E_ERROR: &str = "#ERROR!";

const DAY_MS: i64 = 86_400_000;
/// Dates further from 1970 than this are `#NUM!` (about ±270,000 years).
const MAX_DATE: i64 = 8_640_000_000_000_000;
/// The slack rounding gives a binary fraction that fell just short (`1.005`).
const EPS: f64 = 1e-12;

/// The value, or what the function returns instead: an error, or empty
/// for a date function given no date.
type Res<T> = Result<T, Value>;

fn err(code: &str) -> Value { Value::Error(code.into()) }
fn is_err(v: &Value) -> bool { matches!(v, Value::Error(_)) }

pub(super) fn eval(n: &Node, ctx: &Ctx) -> Value {
    match n {
        Node::Lit(v) => v.clone(),
        Node::Field(id) => field_value(ctx, *id),
        Node::Neg(x) => match eval(x, ctx) {
            e @ Value::Error(_) => e,
            v => num(&v).map_or_else(|e| e, |n| number(-n)),
        },
        Node::Bin(op, a, b) => {
            let a = eval(a, ctx);
            if is_err(&a) { return a; }
            let b = eval(b, ctx);
            if is_err(&b) { return b; }
            binary(*op, &a, &b)
        }
        Node::Call(f, args) => call(*f, args, ctx),
    }
}

/// A cell as a formula reads it: options, linked records and files by
/// name, several of them as a list.
fn field_value(ctx: &Ctx, id: FieldId) -> Value {
    let Some(f) = ctx.table.field(id) else { return err(E_REF) };
    let many = |mut vs: Vec<Value>| match vs.len() { 0 => Value::Empty, 1 => vs.remove(0), _ => Value::List(vs) };
    match ctx.record.get(id) {
        Value::Select(ids) => many(ids.iter().filter_map(|i| f.option(*i)).map(|o| Value::Text(o.name.clone())).collect()),
        Value::Links(ids) => many(ids.iter().map(|i| Value::Text(field::display_in(ctx.base, f, &Value::Links(vec![*i])))).collect()),
        Value::Files(fs) => many(fs.iter().map(|a| Value::Text(a.name.clone())).collect()),
        Value::Text(s) if f.ty == FieldType::Note => Value::Text(field::note_title(s).to_string()),
        v => v.clone(),
    }
}

// —— coercions ——

/// A number result: out of range is `#NUM!`, and the noise binary
/// fractions leave behind (`0.1 + 0.2`) is rounded away at 15 digits.
fn number(n: f64) -> Value {
    if !n.is_finite() { return err(E_NUM); }
    if n == n.trunc() || n.abs() >= 1e15 { return Value::Number(n + 0.); }
    let p = 10f64.powf(14. - n.abs().log10().floor());
    let r = (n * p).round() / p;
    Value::Number(if r.is_finite() { r } else { n })
}

fn dated(ms: i64) -> Value { if ms.abs() > MAX_DATE { err(E_NUM) } else { Value::Date(ms) } }

/// `ms` milliseconds after `d`.
fn later(d: i64, ms: f64) -> Value { dated(d.saturating_add(ms.round() as i64)) }

/// Empty counts as 0, text as the number it spells.
fn num(v: &Value) -> Res<f64> {
    match v {
        Value::Empty => Ok(0.),
        Value::Number(n) => Ok(*n),
        Value::Bool(b) => Ok(*b as u8 as f64),
        Value::Text(s) if s.trim().is_empty() => Ok(0.),
        Value::Text(s) => field::parse_number(s).ok_or_else(|| err(E_VALUE)),
        Value::List(l) if l.len() == 1 => num(&l[0]),
        Value::Error(_) => Err(v.clone()),
        _ => Err(err(E_VALUE)),
    }
}

fn text(v: &Value) -> Cow<'_, str> {
    match v {
        Value::Text(s) => Cow::Borrowed(s),
        Value::Number(n) => plain_number(*n).into(),
        Value::Bool(b) => if *b { "TRUE" } else { "FALSE" }.into(),
        Value::Date(d) => field::iso(*d, false).into(),
        Value::List(l) => l.iter().map(text).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(", ").into(),
        Value::Files(fs) => fs.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(", ").into(),
        Value::Error(e) => Cow::Borrowed(e),
        Value::Empty | Value::Select(_) | Value::Links(_) => "".into(),
    }
}

/// A date, or text that reads as one. No date at all is the empty result.
fn date(v: &Value) -> Res<i64> {
    match v {
        Value::Date(d) => Ok(*d),
        Value::Text(s) if !s.trim().is_empty() => field::parse_date(s).ok_or_else(|| err(E_VALUE)),
        Value::List(l) if l.len() == 1 => date(&l[0]),
        Value::Error(_) => Err(v.clone()),
        v if v.is_empty() => Err(Value::Empty),
        _ => Err(err(E_VALUE)),
    }
}

fn truthy(v: &Value) -> bool {
    match v {
        Value::Bool(b) => *b,
        Value::Number(n) => *n != 0.,
        Value::Date(_) => true,
        v => !v.is_empty(),
    }
}

/// Nothing there (`FALSE` is something).
fn blank(v: &Value) -> bool { !matches!(v, Value::Bool(_)) && v.is_empty() }

/// Numbers by value, dates by time, the rest as text without case; text
/// against a number or a date is read as one when it can be.
fn compare(a: &Value, b: &Value) -> Ordering {
    use Value::*;
    let by = |x: f64, y: f64| x.partial_cmp(&y).unwrap_or(Ordering::Equal);
    match (a, b) {
        (Number(_) | Bool(_) | Empty, Number(_) | Bool(_) | Empty) => return by(num(a).unwrap_or(0.), num(b).unwrap_or(0.)),
        (Date(x), Date(y)) => return x.cmp(y),
        (Date(x), Text(s)) => if let Some(y) = field::parse_date(s) { return x.cmp(&y); },
        (Text(s), Date(y)) => if let Some(x) = field::parse_date(s) { return x.cmp(y); },
        (Number(x), Text(s)) => if let Some(y) = field::parse_number(s) { return by(*x, y); },
        (Text(s), Number(y)) => if let Some(x) = field::parse_number(s) { return by(x, *y); },
        _ => {}
    }
    let (a, b) = (text(a), text(b));
    a.chars().flat_map(char::to_lowercase).cmp(b.chars().flat_map(char::to_lowercase))
}

fn holds(op: Bin, o: Ordering) -> bool {
    match op { Bin::Eq => o.is_eq(), Bin::Ne => o.is_ne(), Bin::Lt => o.is_lt(), Bin::Le => o.is_le(), Bin::Gt => o.is_gt(), _ => o.is_ge() }
}

// —— operators ——

fn binary(op: Bin, a: &Value, b: &Value) -> Value {
    use Value::*;
    let arith = |f: fn(f64, f64) -> Res<f64>| match (num(a), num(b)) {
        (Ok(x), Ok(y)) => f(x, y).map_or_else(|e| e, number),
        (Err(e), _) | (_, Err(e)) => e,
    };
    let plain = |v: &Value| matches!(v, Empty | Number(_) | Bool(_));
    let cat = || Text(format!("{}{}", text(a), text(b)));
    match op {
        Bin::Cat => cat(),
        Bin::Add => match (a, b) {
            // a date and a number of days
            (Date(d), n) | (n, Date(d)) if plain(n) => later(*d, num(n).unwrap_or(0.) * DAY_MS as f64),
            _ if plain(a) && plain(b) => arith(|x, y| Ok(x + y)),
            _ if matches!(a, Text(_) | List(_)) || matches!(b, Text(_) | List(_)) => cat(),
            _ => err(E_VALUE),
        },
        Bin::Sub => match (a, b) {
            (Date(x), Date(y)) => number((x - y) as f64 / DAY_MS as f64),
            (Date(d), n) if plain(n) => later(*d, -num(n).unwrap_or(0.) * DAY_MS as f64),
            (Date(x), Text(s)) => field::parse_date(s).map_or(err(E_VALUE), |y| number((x - y) as f64 / DAY_MS as f64)),
            _ => arith(|x, y| Ok(x - y)),
        },
        Bin::Mul => arith(|x, y| Ok(x * y)),
        Bin::Div => arith(|x, y| if y == 0. { Err(err(E_DIV0)) } else { Ok(x / y) }),
        Bin::Mod => arith(modulo),
        Bin::Pow => arith(|x, y| Ok(x.powf(y))),
        _ => Bool(holds(op, compare(a, b))),
    }
}

/// The remainder, with the divisor's sign (a spreadsheet's MOD).
fn modulo(x: f64, y: f64) -> Res<f64> { if y == 0. { Err(err(E_DIV0)) } else { Ok(x - y * (x / y).floor()) } }

// —— functions ——

fn call(f: F, args: &[Node], ctx: &Ctx) -> Value {
    use F::*;
    let arg = |i: usize| args.get(i).map_or(Value::Empty, |a| eval(a, ctx));
    // these look at an argument only when they need it
    match f {
        IF => {
            let c = arg(0);
            return if is_err(&c) { c } else if truthy(&c) { arg(1) } else { arg(2) };
        }
        IFS => {
            for pair in args.chunks(2) {
                let c = eval(&pair[0], ctx);
                if is_err(&c) { return c; }
                if truthy(&c) { return pair.get(1).map_or(Value::Empty, |v| eval(v, ctx)); }
            }
            return Value::Empty;
        }
        SWITCH => {
            let v = arg(0);
            if is_err(&v) { return v; }
            for pair in args[1..].chunks(2) {
                // the odd one at the end is the default
                let [key, then] = pair else { return eval(&pair[0], ctx) };
                let k = eval(key, ctx);
                if is_err(&k) { return k; }
                if compare(&v, &k).is_eq() { return eval(then, ctx); }
            }
            return Value::Empty;
        }
        AND | OR => {
            for a in args {
                let v = eval(a, ctx);
                if is_err(&v) { return v; }
                if truthy(&v) == (f == OR) { return Value::Bool(f == OR); }
            }
            return Value::Bool(f == AND);
        }
        IFERROR => {
            let v = arg(0);
            return if is_err(&v) { arg(1) } else { v };
        }
        ISERROR => return Value::Bool(is_err(&arg(0))),
        IFBLANK => {
            let v = arg(0);
            return if blank(&v) { arg(1) } else { v };
        }
        _ => {}
    }
    let mut a = Vec::with_capacity(args.len());
    for n in args {
        let v = eval(n, ctx);
        if is_err(&v) { return v; }
        a.push(v);
    }
    eager(f, &a, ctx).unwrap_or_else(|e| e)
}

fn arg(a: &[Value], i: usize) -> &Value { a.get(i).unwrap_or(&EMPTY) }

/// The optional number at `i`.
fn num_or(a: &[Value], i: usize, default: f64) -> Res<f64> {
    match a.get(i) { None | Some(Value::Empty) => Ok(default), Some(v) => num(v) }
}

/// The optional count at `i`: a number that isn't negative.
fn count_or(a: &[Value], i: usize, default: f64) -> Res<usize> {
    let n = num_or(a, i, default)?;
    if n < 0. { Err(err(E_VALUE)) } else { Ok(n as usize) }
}

/// The values of `a` one by one, lists opened up, nothing empty.
fn flat<'a>(a: &'a [Value], out: &mut Vec<&'a Value>) {
    for v in a {
        match v {
            Value::List(l) => flat(l, out),
            v if blank(v) => {}
            v => out.push(v),
        }
    }
}

fn flatten(a: &[Value]) -> Vec<&Value> {
    let mut out = vec![];
    flat(a, &mut out);
    out
}

fn eager(f: F, a: &[Value], ctx: &Ctx) -> Res<Value> {
    use F::*;
    Ok(Value::Bool(match f {
        NOT => !truthy(arg(a, 0)),
        TRUE => true,
        FALSE => false,
        XOR => a.iter().filter(|v| truthy(v)).count() % 2 == 1,
        ISBLANK => blank(arg(a, 0)),
        ISNUMBER => matches!(arg(a, 0), Value::Number(_)),
        ISTEXT => matches!(arg(a, 0), Value::Text(_)),
        _ => return math(f, a, ctx),
    }))
}

fn math(f: F, a: &[Value], ctx: &Ctx) -> Res<Value> {
    use F::*;
    let n = |i: usize| num(arg(a, i));
    Ok(number(match f {
        SUM | AVERAGE | MAX | MIN | MEDIAN | PRODUCT => return aggregate(f, a),
        COUNT => flatten(a).iter().filter(|v| matches!(v, Value::Number(_) | Value::Date(_))).count() as f64,
        COUNTA => flatten(a).len() as f64,
        ABS => n(0)?.abs(),
        ROUND | ROUNDUP | ROUNDDOWN | TRUNC => round_to(n(0)?, num_or(a, 1, 0.)?.trunc().clamp(-15., 15.) as i32, f),
        INT => n(0)?.floor(),
        CEILING | FLOOR => {
            let (x, step) = (n(0)?, num_or(a, 1, 1.)?);
            if step == 0. { 0. } else {
                let q = x / step;
                (if f == CEILING { (q - q.abs() * EPS).ceil() } else { (q + q.abs() * EPS).floor() }) * step
            }
        }
        MOD => modulo(n(0)?, n(1)?)?,
        QUOTIENT => { let (x, y) = (n(0)?, n(1)?); if y == 0. { return Err(err(E_DIV0)); } (x / y).trunc() }
        POWER => n(0)?.powf(n(1)?),
        SQRT => n(0)?.sqrt(),
        EXP => n(0)?.exp(),
        LN => n(0)?.ln(),
        LOG => n(0)?.ln() / num_or(a, 1, 10.)?.ln(),
        LOG10 => n(0)?.log10(),
        SIGN => { let x = n(0)?; if x > 0. { 1. } else if x < 0. { -1. } else { 0. } }
        VALUE => n(0)?,
        RAND => random(ctx.now),
        RANDBETWEEN => {
            let (lo, hi) = (n(0)?.ceil(), n(1)?.floor());
            if hi < lo { return Err(err(E_NUM)); }
            lo + (random(ctx.now) * (hi - lo + 1.)).floor()
        }
        PI => std::f64::consts::PI,
        _ => return texts(f, a, ctx),
    }))
}

/// `x` to `d` decimal places (negative: tens, hundreds…), halves away from zero.
fn round_to(x: f64, d: i32, how: F) -> f64 {
    let p = 10f64.powi(d.abs());
    let s = if d >= 0 { x.abs() * p } else { x.abs() / p };
    if !s.is_finite() { return x; }
    let r = match how { F::ROUND => (s * (1. + EPS)).round(), F::ROUNDUP => (s * (1. - EPS)).ceil(), _ => (s * (1. + EPS)).floor() };
    x.signum() * if d >= 0 { r / p } else { r * p }
}

/// SUM and its kin: over every number in the arguments, empties skipped.
/// All dates in, a date out (MAX of two dates).
fn aggregate(f: F, a: &[Value]) -> Res<Value> {
    let vs = flatten(a);
    let dates = !vs.is_empty() && vs.iter().all(|v| matches!(v, Value::Date(_)));
    let mut ns = vs.iter().map(|v| match v { Value::Date(d) if dates => Ok(*d as f64), v => num(v) }).collect::<Res<Vec<f64>>>()?;
    let r = match f {
        F::SUM | F::PRODUCT if dates => return Err(err(E_VALUE)),
        F::SUM => ns.iter().sum(),
        F::PRODUCT => if ns.is_empty() { 0. } else { ns.iter().product() },
        F::AVERAGE => if ns.is_empty() { return Err(err(E_DIV0)); } else { ns.iter().sum::<f64>() / ns.len() as f64 },
        F::MAX => ns.iter().copied().reduce(f64::max).unwrap_or(0.),
        F::MIN => ns.iter().copied().reduce(f64::min).unwrap_or(0.),
        _ => {
            if ns.is_empty() { return Err(err(E_NUM)); }
            ns.sort_by(f64::total_cmp);
            let mid = ns.len() / 2;
            if ns.len() % 2 == 1 { ns[mid] } else { (ns[mid - 1] + ns[mid]) / 2. }
        }
    };
    Ok(if dates { dated(r as i64) } else { number(r) })
}

/// A number in `0..1`. No `rand` crate here: splitmix64 over a counter.
fn random(seed: i64) -> f64 {
    static STATE: AtomicU64 = AtomicU64::new(0);
    let mut x = STATE.fetch_add(0x9E37_79B9_7F4A_7C15, Relaxed).wrapping_add(seed as u64).wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    ((x ^ (x >> 31)) >> 11) as f64 / (1u64 << 53) as f64
}

fn texts(f: F, a: &[Value], ctx: &Ctx) -> Res<Value> {
    use F::*;
    let s = |i: usize| text(arg(a, i));
    Ok(Value::Text(match f {
        CONCATENATE | CONCAT => a.iter().map(text).collect(),
        LEN => return Ok(Value::Number(s(0).chars().count() as f64)),
        LEFT => s(0).chars().take(count_or(a, 1, 1.)?).collect(),
        RIGHT => {
            let (t, n) = (s(0), count_or(a, 1, 1.)?);
            t.chars().skip(t.chars().count().saturating_sub(n)).collect()
        }
        MID => {
            let (start, n) = (count_or(a, 1, 1.)?, count_or(a, 2, 0.)?);
            if start < 1 { return Err(err(E_VALUE)); }
            s(0).chars().skip(start - 1).take(n).collect()
        }
        UPPER => s(0).to_uppercase(),
        LOWER => s(0).to_lowercase(),
        TRIM => s(0).trim().split(' ').filter(|w| !w.is_empty()).collect::<Vec<_>>().join(" "),
        SUBSTITUTE => {
            let (t, old, new) = (s(0), s(1), s(2));
            match a.get(3).filter(|v| !blank(v)) {
                _ if old.is_empty() => t.into_owned(),
                None => t.replace(&*old, &new),
                Some(nth) => {
                    let nth = num(nth)?;
                    if nth < 1. { return Err(err(E_VALUE)); }
                    match t.match_indices(&*old).nth(nth as usize - 1) {
                        Some((i, _)) => format!("{}{new}{}", &t[..i], &t[i + old.len()..]),
                        None => t.into_owned(),
                    }
                }
            }
        }
        REPLACE => {
            let (t, start, n) = (s(0), count_or(a, 1, 1.)?, count_or(a, 2, 0.)?);
            if start < 1 { return Err(err(E_VALUE)); }
            let mut out: String = t.chars().take(start - 1).collect();
            out.push_str(&s(3));
            out.extend(t.chars().skip((start - 1).saturating_add(n)));
            out
        }
        FIND | SEARCH => {
            let (mut needle, mut hay) = (s(0), s(1));
            if f == SEARCH { (needle, hay) = (needle.to_lowercase().into(), hay.to_lowercase().into()); }
            let start = count_or(a, 2, 1.)?;
            if start < 1 { return Err(err(E_VALUE)); }
            let from = hay.char_indices().nth(start - 1).map_or(hay.len(), |c| c.0);
            return Ok(Value::Number(hay[from..].find(&*needle).map_or(0., |i| (hay[..from + i].chars().count() + 1) as f64)));
        }
        CONTAINS => return Ok(Value::Bool(s(0).contains(&*s(1)))),
        EXACT => return Ok(Value::Bool(s(0) == s(1))),
        REPT => {
            let (t, n) = (s(0), count_or(a, 1, 0.)?);
            if t.len().saturating_mul(n) > 1_000_000 { return Err(err(E_VALUE)); }
            t.repeat(n)
        }
        TEXT => match arg(a, 0) {
            Value::Date(d) => format_date(*d, &s(1)),
            v @ (Value::Number(_) | Value::Bool(_)) => format_num(num(v)?, &s(1)),
            v => text(v).into_owned(),
        },
        T => match arg(a, 0) { Value::Text(t) => t.clone(), _ => String::new() },
        ENCODEURL => {
            let mut out = String::new();
            for b in s(0).bytes() {
                if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { out.push(b as char); } else { let _ = write!(out, "%{b:02X}"); }
            }
            out
        }
        SPLIT => {
            let (t, sep) = (s(0), s(1));
            return Ok(if t.is_empty() { Value::Empty } else if sep.is_empty() { Value::List(vec![Value::Text(t.into_owned())]) } else { Value::List(t.split(&*sep).map(|p| Value::Text(p.to_string())).collect()) });
        }
        JOIN | TEXTJOIN => {
            // a spreadsheet's TEXTJOIN has an ignore-empty flag second: skipped, empties always are
            let rest = match a.get(1) { Some(Value::Bool(_)) if f == TEXTJOIN => &a[2..], _ => &a[1..] };
            flatten(rest).iter().map(|v| text(v)).collect::<Vec<_>>().join(&s(0))
        }
        REGEXMATCH => return Ok(Value::Bool(regex(&s(1), |re| re.is_match(&s(0)))?)),
        REGEXREPLACE => regex(&s(1), |re| re.replace_all(&s(0), &*s(2)).into_owned())?,
        REGEXEXTRACT => match regex(&s(1), |re| re.captures(&s(0)).and_then(|c| c.get(1).or(c.get(0))).map(|m| m.as_str().to_string()))? {
            Some(found) => found,
            None => return Ok(Value::Empty),
        },
        _ => return dates(f, a, ctx),
    }))
}

/// `f` over the compiled `pattern`; compiling is remembered, a formula
/// runs the same pattern for every record.
fn regex<T>(pattern: &str, f: impl FnOnce(&regex::Regex) -> T) -> Res<T> {
    thread_local! { static CACHE: RefCell<HashMap<String, Option<regex::Regex>>> = RefCell::new(HashMap::new()); }
    CACHE.with_borrow_mut(|c| {
        if !c.contains_key(pattern) {
            if c.len() >= 64 { c.clear(); }
            c.insert(pattern.to_string(), regex::Regex::new(pattern).ok());
        }
        c[pattern].as_ref().map(f).ok_or_else(|| err(E_VALUE))
    })
}

/// A number the way a spreadsheet's format code asks: `0.00`, `#,##0`,
/// `0.0%`, `000`; whatever surrounds the digits is kept (`¥0.00`, `0 元`).
fn format_num(x: f64, fmt: &str) -> String {
    let slot = |c: char| matches!(c, '0' | '#');
    let (Some(first), Some(last)) = (fmt.find(slot), fmt.rfind(slot)) else { return plain_number(x) };
    let (pre, core, post) = (&fmt[..first], &fmt[first..last + 1], &fmt[last + 1..]);
    let (int, frac) = core.split_once('.').unwrap_or((core, ""));
    let (least, most) = (frac.matches('0').count(), frac.chars().filter(|c| slot(*c)).count());
    let x = round_to(if fmt.contains('%') { x * 100. } else { x }, most as i32, F::ROUND);
    let mut digits = format!("{:.*}", most, x.abs());
    // `#` places show only when there is something in them
    if most > 0 {
        let keep = digits.len() - (most - least);
        while digits.len() > keep && digits.ends_with('0') { digits.pop(); }
        if digits.ends_with('.') { digits.pop(); }
    }
    let (whole, part) = digits.split_once('.').unwrap_or((&digits, ""));
    let mut whole = format!("{whole:0>width$}", width = int.matches('0').count());
    if int.contains(',') {
        let n = whole.len();
        whole = whole.chars().enumerate().flat_map(|(i, c)| (i > 0 && (n - i).is_multiple_of(3)).then_some(',').into_iter().chain([c])).collect();
    }
    let sign = if x < 0. && digits.contains(|c: char| ('1'..='9').contains(&c)) { "-" } else { "" };
    let dot = if part.is_empty() { "" } else { "." };
    format!("{sign}{pre}{whole}{dot}{part}{post}")
}

/// A date by pattern: `YYYY` `YY` `MM` `M` `DD` `D` `HH` `H` `mm` `m` `ss`
/// `s`, `DDD` 周一, `DDDD` 星期一. An `M` after the hour (or before the
/// seconds) is minutes, whatever its case.
fn format_date(ms: i64, fmt: &str) -> String {
    const WEEK: [char; 7] = ['一', '二', '三', '四', '五', '六', '日'];
    let (y, month, d, h, min) = date_parts(ms);
    let cs: Vec<char> = fmt.chars().collect();
    let mut out = String::new();
    let (mut i, mut clock) = (0, false);
    while i < cs.len() {
        let c = cs[i];
        let run = cs[i..].iter().take_while(|x| **x == c).count();
        let put = |out: &mut String, v: i64| { let _ = if run >= 2 { write!(out, "{v:02}") } else { write!(out, "{v}") }; };
        match c {
            'Y' | 'y' => { let _ = if run <= 2 { write!(out, "{:02}", y.rem_euclid(100)) } else { write!(out, "{y:04}") }; clock = false; }
            'M' | 'm' if clock || matches!(cs[i + run..].iter().find(|c| c.is_ascii_alphabetic()), Some('s' | 'S')) => { put(&mut out, min as i64); clock = false; }
            'M' | 'm' => put(&mut out, month as i64),
            'D' | 'd' if run >= 3 => { out.push_str(if run == 3 { "周" } else { "星期" }); out.push(WEEK[weekday(day_no(ms)) as usize]); clock = false; }
            'D' | 'd' => { put(&mut out, d as i64); clock = false; }
            'H' | 'h' => { put(&mut out, h as i64); clock = true; }
            'S' | 's' => put(&mut out, ms.rem_euclid(60_000) / 1000),
            _ => { out.push(c); i += 1; continue; }
        }
        i += run;
    }
    out
}

// —— dates ——

fn day_no(ms: i64) -> i64 { ms.div_euclid(DAY_MS) }

/// 0 = Monday … 6 = Sunday, of a day number.
fn weekday(day: i64) -> i64 { (day + 3).rem_euclid(7) }

/// `months` later, on the same day of the month or its last.
fn add_months(ms: i64, months: f64) -> Res<i64> {
    if months.abs() > 120_000. { return Err(err(E_NUM)); }
    let (y, m, d, ..) = date_parts(ms);
    let total = y * 12 + m as i64 - 1 + months.trunc() as i64;
    let (y, m) = (total.div_euclid(12), total.rem_euclid(12) as u32 + 1);
    Ok(date_ms(y, m, d.min(days_in_month(y, m)), 0, 0) + ms.rem_euclid(DAY_MS))
}

/// The day numbers of an optional list of 节假日.
fn holidays(a: &[Value]) -> Res<Vec<i64>> {
    let mut days = flatten(a.get(2..).unwrap_or(&[])).into_iter().map(|v| date(v).map(day_no)).collect::<Res<Vec<i64>>>()?;
    days.sort();
    days.dedup();
    Ok(days)
}

fn datedif(start: i64, end: i64, unit: &str) -> Res<i64> {
    let (s, e) = (day_no(start), day_no(end));
    if e < s { return Err(err(E_NUM)); }
    let ((y1, m1, d1, ..), (y2, m2, d2, ..)) = (date_parts(start), date_parts(end));
    let months = (y2 - y1) * 12 + m2 as i64 - m1 as i64 - (d2 < d1) as i64;
    // the start's month and day in year `y`
    let in_year = |y: i64| days_from_civil(y, m1, d1.min(days_in_month(y, m1)));
    Ok(match unit.trim().to_ascii_uppercase().as_str() {
        "Y" => months / 12,
        "M" => months,
        "D" => e - s,
        "W" => (e - s) / 7,
        "H" => (end - start) / 3_600_000,
        "YM" => months % 12,
        "MD" if d2 >= d1 => (d2 - d1) as i64,
        "MD" => {
            let (py, pm) = if m2 == 1 { (y2 - 1, 12) } else { (y2, m2 - 1) };
            (days_in_month(py, pm) as i64 - d1 as i64 + d2 as i64).max(0)
        }
        "YD" => e - if in_year(y2) <= e { in_year(y2) } else { in_year(y2 - 1) },
        _ => return Err(err(E_VALUE)),
    })
}

fn dates(f: F, a: &[Value], ctx: &Ctx) -> Res<Value> {
    use F::*;
    let d = |i: usize| date(arg(a, i));
    let n = |i: usize| num(arg(a, i));
    let part = |i: usize| d(i).map(date_parts);
    let int = |n: i64| Value::Number(n as f64);
    Ok(match f {
        TODAY => Value::Date(day_no(ctx.now) * DAY_MS),
        NOW => Value::Date(ctx.now),
        DATE => {
            let (y, m, day) = (n(0)?.trunc(), n(1)?.trunc(), n(2)?.trunc());
            if !(0. ..=9999.).contains(&y) || m.abs() > 1e5 || day.abs() > 1e7 { return Err(err(E_NUM)); }
            // month 13 is next January, day 0 the last of the month before
            let months = y as i64 * 12 + m as i64 - 1;
            let first = date_ms(months.div_euclid(12), months.rem_euclid(12) as u32 + 1, 1, 0, 0);
            later(first + (day as i64 - 1) * DAY_MS, (num_or(a, 3, 0.)? * 60. + num_or(a, 4, 0.)?) * 60_000.)
        }
        YEAR => int(part(0)?.0),
        MONTH => int(part(0)?.1 as i64),
        DAY => int(part(0)?.2 as i64),
        HOUR => int(part(0)?.3 as i64),
        MINUTE => int(part(0)?.4 as i64),
        SECOND => int(d(0)?.rem_euclid(60_000) / 1000),
        QUARTER => int((part(0)?.1 as i64 - 1) / 3 + 1),
        WEEKDAY => {
            let wd = weekday(day_no(d(0)?));
            int(match num_or(a, 1, 1.)? as i64 { 1 => (wd + 1) % 7 + 1, 2 => wd + 1, 3 => wd, _ => return Err(err(E_NUM)) })
        }
        WEEKNUM => {
            let day = day_no(d(0)?);
            let jan1 = |day: i64| days_from_civil(date_parts(day * DAY_MS).0, 1, 1);
            // the week of January 1st is week 1, weeks starting on `first`
            let from = |first: i64| (day - jan1(day) + (weekday(jan1(day)) - first).rem_euclid(7)) / 7 + 1;
            int(match num_or(a, 1, 1.)? as i64 {
                1 => from(6),
                2 => from(0),
                // ISO 8601: a week belongs to the year its Thursday is in
                21 => { let thu = day - weekday(day) + 3; (thu - jan1(thu)) / 7 + 1 }
                _ => return Err(err(E_NUM)),
            })
        }
        DATEDIF => int(datedif(d(0)?, d(1)?, &text(arg(a, 2)))?),
        DAYS => int(day_no(d(0)?) - day_no(d(1)?)),
        DATEADD => {
            let (t, k) = (d(0)?, n(1)?);
            let ms = |per: i64| later(t, k * per as f64);
            match text(arg(a, 2)).trim().to_lowercase().as_str() {
                "days" | "day" | "d" | "天" | "日" => ms(DAY_MS),
                "weeks" | "week" | "w" | "周" => ms(7 * DAY_MS),
                "hours" | "hour" | "h" | "小时" | "时" => ms(3_600_000),
                "minutes" | "minute" | "min" | "分钟" | "分" => ms(60_000),
                "seconds" | "second" | "s" | "秒" => ms(1000),
                "months" | "month" | "月" => dated(add_months(t, k)?),
                "quarters" | "quarter" | "季度" => dated(add_months(t, k * 3.)?),
                "years" | "year" | "y" | "年" => dated(add_months(t, k * 12.)?),
                _ => return Err(err(E_VALUE)),
            }
        }
        EDATE => dated(add_months(d(0)?, n(1)?)?),
        EOMONTH => {
            let (y, m, ..) = date_parts(add_months(d(0)?, n(1)?)?);
            Value::Date(date_ms(y, m, days_in_month(y, m), 0, 0))
        }
        WORKDAY => {
            let (start, k, off) = (d(0)?, n(1)?.trunc(), holidays(a)?);
            if k.abs() > 1e6 { return Err(err(E_NUM)); }
            let (mut day, step, mut left) = (day_no(start), if k < 0. { -1 } else { 1 }, k.abs() as i64);
            if off.is_empty() && left > 0 {
                // whole weeks at once; a weekend start counts from the working day before it
                while weekday(day) > 4 { day -= step; }
                day += step * (left / 5) * 7;
                left %= 5;
            }
            while left > 0 {
                day += step;
                if weekday(day) < 5 && !off.contains(&day) { left -= 1; }
            }
            Value::Date(day * DAY_MS)
        }
        NETWORKDAYS => {
            let (s, e) = (day_no(d(0)?), day_no(d(1)?));
            let (lo, hi) = (s.min(e), s.max(e));
            let span = hi - lo + 1;
            let free = holidays(a)?.iter().filter(|h| (lo..=hi).contains(*h) && weekday(**h) < 5).count() as i64;
            let days = span / 7 * 5 + (0..span % 7).filter(|i| weekday(lo + i) < 5).count() as i64 - free;
            int(if s <= e { days } else { -days })
        }
        DATEVALUE => Value::Date(d(0)?),
        TIMESTR => {
            let t = d(0)?;
            let (.., h, min) = date_parts(t);
            Value::Text(format!("{h:02}:{min:02}:{:02}", t.rem_euclid(60_000) / 1000))
        }
        DATESTR => { let (y, m, day, ..) = part(0)?; Value::Text(format!("{y:04}-{m:02}-{day:02}")) }
        ISTODAY => Value::Bool(day_no(d(0)?) == day_no(ctx.now)),
        _ => return other(f, a, ctx),
    })
}

fn other(f: F, a: &[Value], ctx: &Ctx) -> Res<Value> {
    use F::*;
    Ok(match f {
        RECORD_ID => Value::Text(ctx.record.id.to_string()),
        CREATED_TIME => Value::Date(ctx.record.created),
        LAST_MODIFIED_TIME => Value::Date(ctx.record.modified),
        ARRAYJOIN => {
            let sep = match a.get(1) { None | Some(Value::Empty) => ", ".into(), Some(v) => text(v) };
            Value::Text(flatten(&a[..1]).iter().map(|v| text(v)).collect::<Vec<_>>().join(&sep))
        }
        ARRAYUNIQUE => {
            let mut seen: Vec<Value> = vec![];
            for v in flatten(a) { if !seen.contains(v) { seen.push(v.clone()); } }
            Value::List(seen)
        }
        COUNTIF => {
            // `">5"` and the like compare; anything else has to equal
            let (op, want) = match arg(a, 1) {
                Value::Text(s) => match [(">=", Bin::Ge), ("<=", Bin::Le), ("<>", Bin::Ne), ("!=", Bin::Ne), (">", Bin::Gt), ("<", Bin::Lt), ("=", Bin::Eq)].iter().find_map(|(p, op)| s.strip_prefix(p).map(|rest| (*op, rest.trim()))) {
                    Some((op, rest)) => (op, field::parse_number(rest).map_or_else(|| Value::Text(rest.to_string()), Value::Number)),
                    None => (Bin::Eq, Value::Text(s.clone())),
                },
                v => (Bin::Eq, v.clone()),
            };
            Value::Number(flatten(&a[..1]).iter().filter(|v| holds(op, compare(v, &want))).count() as f64)
        }
        FIRST => flatten(a).first().map_or(Value::Empty, |v| (*v).clone()),
        LAST => flatten(a).last().map_or(Value::Empty, |v| (*v).clone()),
        _ => err(E_ERROR),
    })
}
