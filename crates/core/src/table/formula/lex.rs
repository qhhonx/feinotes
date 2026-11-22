//! 公式 tokens. Full-width brackets, commas and quotes read as their ASCII twins.

#[derive(Clone, Debug, PartialEq)]
pub(super) enum Tok {
    Num(f64),
    Str(String),
    /// `{名字}` or `[名字]`.
    Ref(String),
    /// A bare word: a function name, a field name, TRUE / FALSE.
    Ident(String),
    /// An operator, spelled one way (`==` is `=`, `<>` is `!=`).
    Op(&'static str),
    LParen,
    RParen,
    Comma,
}

/// A token and the bytes of the source it came from.
pub(super) struct Lexed { pub tok: Tok, pub start: usize, pub end: usize }

/// What is wrong with a formula's source, and where (bytes).
#[derive(Debug)]
pub(super) struct Bad { pub start: usize, pub end: usize, pub msg: String }

/// Whether `name` can be written without braces.
pub(super) fn bare(name: &str) -> bool {
    let mut cs = name.chars();
    cs.next().is_some_and(|c| c.is_alphabetic() || c == '_') && cs.all(|c| c.is_alphanumeric() || c == '_')
        && !name.eq_ignore_ascii_case("true") && !name.eq_ignore_ascii_case("false")
}

/// The tokens of `src`, up to the first thing that isn't one.
pub(super) fn lex(src: &str) -> (Vec<Lexed>, Option<Bad>) {
    let cs: Vec<(usize, char)> = src.char_indices().collect();
    let at = |i: usize| cs.get(i).map_or(src.len(), |c| c.0);
    let ch = |i: usize| cs.get(i).map(|c| c.1);
    let digit = |i: usize| ch(i).is_some_and(|c| c.is_ascii_digit());
    let mut out = vec![];
    let mut i = 0;
    while let Some(c) = ch(i) {
        let start = at(i);
        let bad = |msg: String| Some(Bad { start, end: src.len(), msg });
        let tok = match c {
            c if c.is_whitespace() || c == '\u{feff}' => { i += 1; continue; }
            '(' | '（' => { i += 1; Tok::LParen }
            ')' | '）' => { i += 1; Tok::RParen }
            ',' | '，' | ';' | '；' => { i += 1; Tok::Comma }
            '{' | '[' => {
                let close = if c == '{' { '}' } else { ']' };
                let Some(n) = (i + 1..cs.len()).find(|j| cs[*j].1 == close) else { return (out, bad(format!("字段引用缺少 {close}"))) };
                let name = src[at(i + 1)..at(n)].to_string();
                i = n + 1;
                Tok::Ref(name)
            }
            '"' | '\'' | '“' | '”' | '‘' | '’' => {
                let closes: &[char] = match c { '“' | '”' => &['”', '“'], '‘' | '’' => &['’', '‘'], _ => &[c] };
                let mut s = String::new();
                let mut j = i + 1;
                loop {
                    match (ch(j), ch(j + 1)) {
                        (None, _) => return (out, bad("字符串缺少结束引号".into())),
                        (Some('\\'), Some(e)) => {
                            match e {
                                'n' => s.push('\n'),
                                't' => s.push('\t'),
                                '\\' | '"' | '\'' | '“' | '”' | '‘' | '’' => s.push(e),
                                // anything else keeps its backslash (a regex's `\d`)
                                _ => { s.push('\\'); s.push(e); }
                            }
                            j += 2;
                        }
                        // a spreadsheet's doubled quote
                        (Some(d), Some(e)) if d == c && e == c && closes.len() == 1 => { s.push(d); j += 2; }
                        (Some(d), _) if closes.contains(&d) => break,
                        (Some(d), _) => { s.push(d); j += 1; }
                    }
                }
                i = j + 1;
                Tok::Str(s)
            }
            c if c.is_ascii_digit() || (c == '.' && digit(i + 1)) => {
                let mut j = i;
                while digit(j) { j += 1; }
                if ch(j) == Some('.') { j += 1; while digit(j) { j += 1; } }
                if matches!(ch(j), Some('e' | 'E')) {
                    let k = if matches!(ch(j + 1), Some('+' | '-')) { j + 2 } else { j + 1 };
                    if digit(k) { j = k; while digit(j) { j += 1; } }
                }
                i = j;
                match src[start..at(j)].parse::<f64>() { Ok(n) if n.is_finite() => Tok::Num(n), _ => return (out, Some(Bad { start, end: at(j), msg: "数字无效".into() })) }
            }
            c if c.is_alphabetic() || c == '_' => {
                while ch(i).is_some_and(|c| c.is_alphanumeric() || c == '_') { i += 1; }
                Tok::Ident(src[start..at(i)].to_string())
            }
            _ => {
                let two = match (c, ch(i + 1)) { ('=', Some('=')) => Some("="), ('!', Some('=')) | ('<', Some('>')) => Some("!="), ('<', Some('=')) => Some("<="), ('>', Some('=')) => Some(">="), _ => None };
                let one = match c {
                    '=' => "=", '<' => "<", '>' => ">", '&' => "&", '+' => "+", '-' => "-", '*' => "*", '/' => "/", '%' => "%", '^' => "^",
                    '≠' => "!=", '≤' => "<=", '≥' => ">=",
                    _ if two.is_some() => "",
                    _ => return (out, Some(Bad { start, end: at(i + 1), msg: format!("无法识别的字符：{c}") })),
                };
                i += if two.is_some() { 2 } else { 1 };
                Tok::Op(two.unwrap_or(one))
            }
        };
        out.push(Lexed { tok, start, end: at(i) });
    }
    (out, None)
}
