use super::*;
use crate::table::field::date_ms;

/// 2026-11-17 10:30, a Tuesday.
const NOW: i64 = 1_794_911_400_000;

/// A blank table made 文本 / 单选 / 日期 / 附件 (飞书's blank table), with more columns, a filled
/// first record, and a second table to link to.
fn base() -> Base {
    let mut b = Base::blank(0);
    let mut other = b.blank_table("客户", 0);
    let title = other.fields[0].id;
    other.records[0].set(title, Value::Text("甲公司".into()));
    other.records[1].set(title, Value::Text("乙公司".into()));
    let other_id = other.id;
    b.tables.push(other);
    let t = &mut b.tables[0];
    t.fields.truncate(4);
    for (f, (name, ty)) in t.fields.iter_mut().zip([("文本", FieldType::Text), ("单选", FieldType::SingleSelect), ("日期", FieldType::Date), ("附件", FieldType::Attachment)]) { f.name = name.into(); f.ty = ty; }
    let opt = |id: u32, name: &str| SelectOption { id, name: name.into(), color: 0 };
    t.fields[1].opts.options = vec![opt(301, "待办"), opt(302, "完成")];
    let mut add = |id: FieldId, name: &str, ty: FieldType, opts: FieldOpts| t.fields.push(Field { id, name: name.into(), ty, opts, desc: String::new() });
    add(100, "数量", FieldType::Number, FieldOpts::default());
    add(101, "单价", FieldType::Number, FieldOpts::default());
    add(102, "标签", FieldType::MultiSelect, FieldOpts { options: vec![opt(201, "红"), opt(202, "蓝")], ..Default::default() });
    add(103, "完成", FieldType::Checkbox, FieldOpts::default());
    add(104, "My Field", FieldType::Text, FieldOpts::default());
    add(105, "客户", FieldType::Link, FieldOpts { link_table: Some(other_id), ..Default::default() });
    add(106, "引用", FieldType::Lookup, FieldOpts::default());
    add(107, "备注", FieldType::Text, FieldOpts::default());
    add(108, "坏", FieldType::Formula, FieldOpts::default());
    let (text, select, date) = (t.fields[0].id, t.fields[1].id, t.fields[2].id);
    let r = &mut t.records[0];
    (r.created, r.modified) = (date_ms(2026, 1, 1, 8, 0), date_ms(2026, 2, 2, 9, 0));
    r.set(text, Value::Text("Hello".into()));
    r.set(select, Value::Select(vec![301]));
    r.set(date, Value::Date(date_ms(2026, 3, 15, 9, 30)));
    r.set(100, Value::Number(3.));
    r.set(101, Value::Number(1.5));
    r.set(102, Value::Select(vec![201, 202]));
    r.set(103, Value::Bool(true));
    r.set(104, Value::Text("x y".into()));
    r.set(105, Value::Links(vec![1, 2]));
    r.set(106, Value::List(vec![Value::Number(1.), Value::Number(2.), Value::Empty, Value::Number(3.)]));
    r.set(108, Value::Error("#循环引用".into()));
    b
}

fn ev(src: &str) -> Value {
    let b = base();
    let t = &b.tables[0];
    let e = compile(src, t).unwrap_or_else(|m| panic!("{src}: {m}"));
    eval(&e, &Ctx { base: &b, table: t, record: &t.records[0], now: NOW })
}

fn bad(src: &str) -> String { compile(src, &base().tables[0]).expect_err(src) }

fn n(x: f64) -> Value { Value::Number(x) }
fn s(x: &str) -> Value { Value::Text(x.into()) }
fn d(y: i64, m: u32, day: u32) -> Value { Value::Date(date_ms(y, m, day, 0, 0)) }
fn dt(y: i64, m: u32, day: u32, h: u32, min: u32) -> Value { Value::Date(date_ms(y, m, day, h, min)) }
fn e(code: &str) -> Value { Value::Error(code.into()) }
const T: Value = Value::Bool(true);
const F: Value = Value::Bool(false);

/// Each formula and what it comes to.
fn all(cases: &[(&str, Value)]) {
    for (src, want) in cases { assert_eq!(ev(src), *want, "{src}"); }
}

#[test]
fn now_is_a_tuesday() { assert_eq!(NOW, date_ms(2026, 11, 17, 10, 30)); }

#[test]
fn literals() {
    all(&[
        ("1", n(1.)), ("1.5", n(1.5)), (".5", n(0.5)), ("1e3", n(1000.)), ("1.5E-2", n(0.015)), ("=1+2", n(3.)),
        (r#""a\"b""#, s("a\"b")), (r#""a\\b""#, s("a\\b")), (r#""a\nb""#, s("a\nb")), ("'it''s'", s("it's")), (r#""say ""hi""""#, s("say \"hi\"")),
        ("'单引号'", s("单引号")), ("“中文引号”", s("中文引号")), ("‘中文’", s("中文")), (r#""""#, s("")),
        ("TRUE", T), ("true", T), ("False", F), ("  1  ", n(1.)),
    ]);
}

#[test]
fn arithmetic() {
    all(&[
        ("1+2", n(3.)), ("5-7", n(-2.)), ("2*3", n(6.)), ("7/2", n(3.5)), ("7%3", n(1.)), ("-7%3", n(2.)), ("2^10", n(1024.)),
        ("1/0", e("#DIV/0!")), ("5%0", e("#DIV/0!")), ("0.1+0.2", n(0.3)), ("1-0.9", n(0.1)), ("+5", n(5.)), ("--5", n(5.)), ("-{数量}", n(-3.)),
        ("\"3\"*2", n(6.)), ("\"x\"*2", e("#VALUE!")), ("-\"a\"", e("#VALUE!")), ("TRUE+1", n(2.)), ("{备注}+1", n(1.)), ("{备注}*5", n(0.)),
        ("1e308*10", e("#NUM!")), ("(-8)^0.5", e("#NUM!")), ("0-0", n(0.)),
    ]);
}

#[test]
fn precedence() {
    all(&[
        ("1+2*3", n(7.)), ("(1+2)*3", n(9.)), ("（1+2）*3", n(9.)), ("2^3^2", n(512.)), ("-2^2", n(-4.)), ("2^-1", n(0.5)), ("2*3^2", n(18.)),
        ("10-2-3", n(5.)), ("100/10/5", n(2.)), ("1+2&3", s("33")), ("1&2+3", s("15")), ("1+1=2", T), ("1&1=\"11\"", T), ("1<2=TRUE", T),
        ("2*-3", n(-6.)), ("-(1+2)", n(-3.)), ("7-4%3", n(6.)),
    ]);
}

#[test]
fn plus_joins_text_and_ampersand_always_does() {
    all(&[
        ("\"a\"&\"b\"", s("ab")), ("1&2", s("12")), ("\"a\"+1", s("a1")), ("1+\"a\"", s("1a")), ("\"a\"+\"b\"", s("ab")), ("1.50&\"\"", s("1.5")),
        ("TRUE&\"\"", s("TRUE")), ("{备注}&\"x\"", s("x")), ("DATE(2026,1,2)&\"\"", s("2026-01-02")), ("{日期}&\"\"", s("2026-03-15 09:30")),
        ("{标签}&\"\"", s("红, 蓝")), ("{客户}&\"!\"", s("甲公司, 乙公司!")), ("\"n=\"&0.1+0.2", s("n=0.3")),
    ]);
}

#[test]
fn comparison() {
    all(&[
        ("1=1", T), ("1==1", T), ("1!=2", T), ("1<>1", F), ("1≠2", T), ("1<2", T), ("2<=2", T), ("2≤1", F), ("3>2", T), ("2>=3", F), ("3≥3", T),
        ("\"abc\"=\"ABC\"", T), ("\"a\"<\"b\"", T), ("\"b\">\"A\"", T), ("\"a\"=\"b\"", F), ("{备注}=\"\"", T), ("{备注}=0", T), ("{备注}={附件}", T),
        ("{数量}=\"3\"", T), ("{单选}=\"待办\"", T), ("{标签}=\"红, 蓝\"", T), ("{完成}=TRUE", T), ("{附件}=FALSE", T),
        ("DATE(2026,1,2)>DATE(2026,1,1)", T), ("DATE(2026,1,1)=DATE(2026,1,1)", T), ("{日期}>\"2026-01-01\"", T), ("{日期}<\"2026-01-01\"", F),
        ("1>2", F),
    ]);
}

#[test]
fn date_arithmetic() {
    all(&[
        ("{日期}+1", dt(2026, 3, 16, 9, 30)), ("1+{日期}", dt(2026, 3, 16, 9, 30)), ("{日期}-15", dt(2026, 2, 28, 9, 30)), ("DATE(2026,1,1)+0.5", dt(2026, 1, 1, 12, 0)),
        ("DATE(2026,3,15)-DATE(2026,3,10)", n(5.)), ("DATE(2026,3,10)-DATE(2026,3,15)", n(-5.)), ("{日期}-DATE(2026,3,15)", n(0.395833333333333)),
        ("DATE(2026,3,15)-\"2026-03-10\"", n(5.)), ("DATE(2026,1,1)+DATE(2026,1,1)", e("#VALUE!")), ("DATE(2026,1,1)*2", e("#VALUE!")),
        ("{日期}+{备注}", dt(2026, 3, 15, 9, 30)), ("DATE(2026,1,1)+1e300", e("#NUM!")),
    ]);
}

#[test]
fn field_references() {
    all(&[
        ("{数量}*{单价}", n(4.5)), ("[数量]*[单价]", n(4.5)), ("数量*单价", n(4.5)), ("{ 数量 }", n(3.)), ("{My Field}", s("x y")), ("[My Field]", s("x y")),
        ("{文本}", s("Hello")), ("文本&\"!\"", s("Hello!")), ("{单选}", s("待办")), ("{标签}", Value::List(vec![s("红"), s("蓝")])),
        ("{客户}", Value::List(vec![s("甲公司"), s("乙公司")])), ("{完成}", T), ("{附件}", Value::Empty), ("{备注}", Value::Empty),
        ("{日期}", dt(2026, 3, 15, 9, 30)), ("{引用}", Value::List(vec![n(1.), n(2.), Value::Empty, n(3.)])),
    ]);
    // a field named like a keyword wins over the keyword
    let mut b = base();
    b.tables[0].fields[0].name = "TRUE".into();
    let t = &b.tables[0];
    assert_eq!(eval(&compile("TRUE", t).unwrap(), &Ctx { base: &b, table: t, record: &t.records[0], now: NOW }), s("Hello"));
    assert_eq!(eval(&compile("TRUE()", t).unwrap(), &Ctx { base: &b, table: t, record: &t.records[0], now: NOW }), T);
}

#[test]
fn a_deleted_field_is_a_ref_error() {
    let mut b = base();
    let expr = compile("{数量}+1", &b.tables[0]).unwrap();
    b.tables[0].fields.retain(|f| f.id != 100);
    let t = &b.tables[0];
    assert_eq!(eval(&expr, &Ctx { base: &b, table: t, record: &t.records[0], now: NOW }), e("#REF!"));
}

#[test]
fn deps_are_the_fields_read_once_each() {
    let b = base();
    let t = &b.tables[0];
    let expr = compile("{数量}*数量+[单价]+LEN({文本})+IF(完成, 1, {数量})", t).unwrap();
    assert_eq!(deps(&expr), vec![100, 101, t.fields[0].id, 103]);
    assert_eq!(deps(&compile("1+NOW()", t).unwrap()), Vec::<FieldId>::new());
    assert_eq!(expr.clone(), expr);
    assert_ne!(expr, compile("1", t).unwrap());
    assert!(format!("{expr:?}").contains("Field(100)"));
}

#[test]
fn compile_errors_say_what_is_wrong() {
    for (src, msg) in [
        ("{不存在}", "未知字段：不存在"), ("[不存在]", "未知字段：不存在"), ("foo", "未知字段：foo"), ("1+foo*2", "未知字段：foo"),
        ("FOO(1)", "未知函数：FOO"), ("foo(1)", "未知函数：FOO"), ("SUM", "函数 SUM 后面缺少括号"),
        ("(1+2", "缺少右括号"), ("SUM(1,2", "缺少右括号"), ("SUM(1,", "缺少右括号"), ("SUM(", "缺少右括号"), ("IF((1,2,3)", "缺少右括号"),
        ("1+2)", "多余的右括号"), ("\"abc", "字符串缺少结束引号"), ("“abc", "字符串缺少结束引号"), ("{数量", "字段引用缺少 }"), ("[数量", "字段引用缺少 ]"),
        ("1 +", "公式不完整：缺少运算数"), ("* 2", "缺少运算数"), ("1 + * 2", "缺少运算数"), ("()", "括号里是空的"), ("1 2", "缺少运算符"), ("{数量} {单价}", "缺少运算符"),
        ("1, 2", "逗号只能用在函数的参数之间"), ("", "公式为空"), ("   ", "公式为空"), ("=", "公式为空"), ("1 # 2", "无法识别的字符：#"), ("1 ! 2", "无法识别的字符：!"),
        ("IF(1)", "IF 至少需要 2 个参数"), ("IF(1,2,3,4)", "IF 最多接受 3 个参数"), ("PI(1)", "PI 不需要参数"), ("MID(\"a\",1)", "MID 需要 3 个参数"),
        ("IFS(1,2,3)", "IFS 的参数要成对出现（条件, 值）"), ("not()", "NOT 需要 1 个参数"), ("SUM()", "SUM 至少需要 1 个参数"), ("SUM(1 2)", "缺少逗号或右括号"),
        ("1e999", "数字无效"),
    ] {
        assert_eq!(bad(src), msg, "{src}");
    }
    assert_eq!(bad(&format!("{}1{}", "(".repeat(100), ")".repeat(100))), "公式嵌套太深");
    assert_eq!(bad(&format!("{}1", "-".repeat(100))), "公式嵌套太深");
    assert!(compile(&format!("{}1{}", "(".repeat(30), ")".repeat(30)), &base().tables[0]).is_ok());
}

#[test]
fn check_points_at_the_fault() {
    let b = base();
    let t = &b.tables[0];
    assert_eq!(check("1 + {数量}", t), None);
    assert_eq!(check("1 + foo", t), Some((4, 7, "未知字段：foo".into())));
    assert_eq!(check("1 + 中文 + 2", t), Some((4, 10, "未知字段：中文".into())));
    assert_eq!(check("1 + (2", t), Some((4, 5, "缺少右括号".into())));
    assert_eq!(check("1 +", t), Some((3, 3, "公式不完整：缺少运算数".into())));
    assert_eq!(check("1 + IF(1) + 2", t), Some((4, 9, "IF 至少需要 2 个参数".into())));
    assert_eq!(check("1 + \"abc", t), Some((4, 8, "字符串缺少结束引号".into())));
}

#[test]
fn errors_spread_until_caught() {
    all(&[
        ("1/0+1", e("#DIV/0!")), ("-(1/0)", e("#DIV/0!")), ("\"a\"&1/0", e("#DIV/0!")), ("1/0=1/0", e("#DIV/0!")), ("SUM(1, 1/0)", e("#DIV/0!")),
        ("LEN(1/0)", e("#DIV/0!")), ("IF(1/0, 1, 2)", e("#DIV/0!")), ("NOT(1/0)", e("#DIV/0!")), ("ISBLANK(1/0)", e("#DIV/0!")),
        ("IFERROR(1/0, \"x\")", s("x")), ("IFERROR(5, \"x\")", n(5.)), ("IFERROR(1/0)", Value::Empty), ("IFERROR(\"a\"*1, 1/0)", e("#DIV/0!")),
        ("ISERROR(1/0)", T), ("ISERROR(1)", F), ("ISERROR(SQRT(-1))", T), ("SQRT(-1)", e("#NUM!")), ("LN(0)", e("#NUM!")),
        ("{坏}+1", e("#循环引用")), ("IFERROR({坏}, 0)", n(0.)), ("ISERROR({坏})", T), ("IFBLANK(1/0, 1)", e("#DIV/0!")),
    ]);
}

#[test]
fn logic_only_evaluates_what_it_needs() {
    all(&[
        ("IF(TRUE, 1, 1/0)", n(1.)), ("IF(FALSE, 1/0, 2)", n(2.)), ("IFS(FALSE, 1/0, TRUE, 2, 1/0, 3)", n(2.)), ("SWITCH(1, 1, \"a\", 1/0, \"b\")", s("a")),
        ("SWITCH(2, 1, 1/0, 2, \"b\", 1/0)", s("b")), ("AND(FALSE, 1/0)", F), ("OR(TRUE, 1/0)", T), ("AND(TRUE, 1/0)", e("#DIV/0!")), ("OR(FALSE, 1/0)", e("#DIV/0!")),
        ("IFERROR(1, 1/0)", n(1.)), ("IFBLANK(1, 1/0)", n(1.)),
    ]);
}

#[test]
fn logic_functions() {
    all(&[
        ("IF(1>2,\"a\",\"b\")", s("b")), ("IF(1>2,\"a\")", Value::Empty), ("IF({完成}, \"done\", \"todo\")", s("done")), ("IF({备注}, 1, 2)", n(2.)),
        ("if(true, 1, 2)", n(1.)), ("IF(TRUE; 1; 2)", n(1.)), ("IF（TRUE，1，2）", n(1.)), ("IF(TRUE,,2)", Value::Empty), ("IF(\"a\", 1, 2)", n(1.)),
        ("IFS(1>2,\"a\",2>1,\"b\")", s("b")), ("IFS(FALSE,1)", Value::Empty),
        ("SWITCH({单选},\"待办\",1,\"完成\",2,0)", n(1.)), ("SWITCH(\"x\",\"a\",1,\"other\")", s("other")), ("SWITCH(\"x\",\"a\",1)", Value::Empty), ("SWITCH(\"A\",\"a\",1)", n(1.)),
        ("AND(1,TRUE,\"a\")", T), ("AND(1,0)", F), ("OR(0,\"\",FALSE)", F), ("OR(0,1)", T), ("XOR(TRUE,TRUE)", F), ("XOR(TRUE,FALSE)", T),
        ("NOT(TRUE)", F), ("NOT(0)", T), ("TRUE()", T), ("FALSE()", F),
        ("ISBLANK({备注})", T), ("ISBLANK({文本})", F), ("ISBLANK(\"\")", T), ("ISBLANK(0)", F), ("ISBLANK(FALSE)", F),
        ("IFBLANK({备注},\"无\")", s("无")), ("IFBLANK({文本},\"无\")", s("Hello")),
        ("ISNUMBER(1)", T), ("ISNUMBER(\"1\")", F), ("ISNUMBER({数量})", T), ("ISTEXT(\"a\")", T), ("ISTEXT(1)", F), ("ISTEXT({文本})", T),
    ]);
}

#[test]
fn number_functions() {
    all(&[
        ("SUM(1,2,3)", n(6.)), ("SUM({引用})", n(6.)), ("SUM({引用}, 4, {备注}, \"5\")", n(15.)), ("SUM(\"a\")", e("#VALUE!")), ("SUM({备注})", n(0.)),
        ("AVERAGE(1,2,3,4)", n(2.5)), ("AVERAGE({引用})", n(2.)), ("AVERAGE({备注})", e("#DIV/0!")),
        ("MAX(1,5,3)", n(5.)), ("MIN(1,5,3)", n(1.)), ("MAX({引用}, 2.5)", n(3.)), ("MAX({备注})", n(0.)),
        ("MAX(DATE(2026,1,1), DATE(2026,5,1))", d(2026, 5, 1)), ("MIN(DATE(2026,1,1), {日期})", d(2026, 1, 1)), ("SUM(DATE(2026,1,1))", e("#VALUE!")), ("MAX(DATE(2026,1,1), 1)", e("#VALUE!")),
        ("MEDIAN(3,1,2)", n(2.)), ("MEDIAN(1,2,3,4)", n(2.5)), ("MEDIAN({备注})", e("#NUM!")), ("PRODUCT(2,3,4)", n(24.)),
        ("COUNT(1,\"a\",2,{引用})", n(5.)), ("COUNTA(1,\"a\",\"\",{引用},{标签},{备注})", n(7.)),
        ("ABS(-3)", n(3.)), ("ROUND(2.567,2)", n(2.57)), ("ROUND(2.5)", n(3.)), ("ROUND(-2.5)", n(-3.)), ("ROUND(1.005,2)", n(1.01)), ("ROUND(1234,-2)", n(1200.)), ("ROUND(2.4)", n(2.)),
        ("ROUNDUP(2.111,2)", n(2.12)), ("ROUNDUP(2.12,2)", n(2.12)), ("ROUNDUP(-2.1)", n(-3.)), ("ROUNDDOWN(2.999,2)", n(2.99)), ("ROUNDDOWN(-2.9)", n(-2.)), ("TRUNC(2.9)", n(2.)), ("TRUNC(-2.99,1)", n(-2.9)),
        ("INT(2.9)", n(2.)), ("INT(-2.1)", n(-3.)), ("CEILING(2.1)", n(3.)), ("CEILING(2.1,0.5)", n(2.5)), ("CEILING(1.1,0.1)", n(1.1)), ("CEILING(-2.5)", n(-2.)), ("CEILING(5,0)", n(0.)),
        ("FLOOR(2.9)", n(2.)), ("FLOOR(2.9,0.5)", n(2.5)), ("FLOOR(0.3,0.1)", n(0.3)), ("FLOOR(17,5)", n(15.)),
        ("MOD(7,3)", n(1.)), ("MOD(-1,3)", n(2.)), ("MOD(5.5,2)", n(1.5)), ("MOD(1,0)", e("#DIV/0!")), ("QUOTIENT(7,2)", n(3.)), ("QUOTIENT(-7,2)", n(-3.)), ("QUOTIENT(1,0)", e("#DIV/0!")),
        ("POWER(2,3)", n(8.)), ("SQRT(16)", n(4.)), ("EXP(0)", n(1.)), ("ROUND(EXP(2),3)", n(7.389)), ("LN(1)", n(0.)), ("LOG(100)", n(2.)), ("LOG(8,2)", n(3.)), ("LOG10(1000)", n(3.)),
        ("SIGN(-5)", n(-1.)), ("SIGN(0)", n(0.)), ("SIGN(3)", n(1.)), ("VALUE(\"1,234.5\")", n(1234.5)), ("VALUE(\"50%\")", n(0.5)), ("VALUE(\"abc\")", e("#VALUE!")), ("VALUE(7)", n(7.)),
        ("ROUND(PI()*10,2)", n(31.42)), ("RANDBETWEEN(5,1)", e("#NUM!")), ("RANDBETWEEN(4,4)", n(4.)),
    ]);
    let (a, b) = (ev("RAND()"), ev("RAND()"));
    assert!(matches!(a, Value::Number(x) if (0. ..1.).contains(&x)), "{a:?}");
    assert_ne!(a, b);
    for _ in 0..50 { assert!(matches!(ev("RANDBETWEEN(1,6)"), Value::Number(x) if (1. ..=6.).contains(&x) && x == x.trunc())); }
}

#[test]
fn text_functions() {
    all(&[
        ("CONCATENATE(\"a\",1,TRUE)", s("a1TRUE")), ("CONCAT(\"a\",\"b\",{备注})", s("ab")), ("LEN(\"你好ab\")", n(4.)), ("LEN({备注})", n(0.)), ("LEN(12345)", n(5.)),
        ("LEFT(\"你好ab\",3)", s("你好a")), ("LEFT(\"abc\")", s("a")), ("LEFT(\"abc\",9)", s("abc")), ("LEFT(\"abc\",-1)", e("#VALUE!")),
        ("RIGHT(\"你好ab\",3)", s("好ab")), ("RIGHT(\"abc\")", s("c")), ("RIGHT(\"abc\",9)", s("abc")),
        ("MID(\"你好世界\",2,2)", s("好世")), ("MID(\"abc\",2,9)", s("bc")), ("MID(\"abc\",0,1)", e("#VALUE!")),
        ("UPPER(\"aB\")", s("AB")), ("LOWER(\"aB\")", s("ab")), ("TRIM(\"  a   b  \")", s("a b")),
        ("SUBSTITUTE(\"a-b-c\",\"-\",\"+\")", s("a+b+c")), ("SUBSTITUTE(\"a-b-c\",\"-\",\"+\",2)", s("a-b+c")), ("SUBSTITUTE(\"a-b-c\",\"-\",\"+\",5)", s("a-b-c")), ("SUBSTITUTE(\"abc\",\"\",\"x\")", s("abc")),
        ("REPLACE(\"你好世界\",2,2,\"X\")", s("你X界")), ("REPLACE(\"abc\",1,0,\"x\")", s("xabc")), ("REPLACE(\"abc\",2,9,\"x\")", s("ax")),
        ("FIND(\"世\",\"你好世界\")", n(3.)), ("FIND(\"z\",\"abc\")", n(0.)), ("FIND(\"a\",\"abca\",2)", n(4.)), ("FIND(\"B\",\"abc\")", n(0.)), ("SEARCH(\"B\",\"abc\")", n(2.)), ("SEARCH(\"界\",\"你好世界\",2)", n(4.)),
        ("CONTAINS(\"hello\",\"ell\")", T), ("CONTAINS(\"hello\",\"xyz\")", F), ("CONTAINS({标签},\"蓝\")", T), ("EXACT(\"a\",\"A\")", F), ("EXACT(\"a\",\"a\")", T),
        ("REPT(\"ab\",3)", s("ababab")), ("REPT(\"ab\",0)", s("")), ("REPT(\"ab\",1e9)", e("#VALUE!")), ("T(\"a\")", s("a")), ("T(1)", s("")),
        ("ENCODEURL(\"a b&c/中\")", s("a%20b%26c%2F%E4%B8%AD")),
        ("SPLIT(\"a,b,c\",\",\")", Value::List(vec![s("a"), s("b"), s("c")])), ("SPLIT({备注},\",\")", Value::Empty), ("ARRAYJOIN(SPLIT(\"a,b,c\",\",\"),\"-\")", s("a-b-c")),
        ("JOIN(\"-\", \"a\", \"\", \"b\", {引用})", s("a-b-1-2-3")), ("TEXTJOIN(\"-\", TRUE, \"a\", \"b\")", s("a-b")), ("TEXTJOIN(\"/\", {标签}, 1)", s("红/蓝/1")),
        (r#"REGEXMATCH("abc123","\d+")"#, T), (r#"REGEXMATCH("abc","^\d+$")"#, F), (r#"REGEXMATCH("a","(")"#, e("#VALUE!")),
        (r##"REGEXREPLACE("a1b22","\d+","#")"##, s("a#b#")), (r#"REGEXREPLACE("john smith","(\w+) (\w+)","$2 $1")"#, s("smith john")),
        (r#"REGEXEXTRACT("tel: 12345","\d+")"#, s("12345")), (r#"REGEXEXTRACT("k=v","=(\w+)")"#, s("v")), (r#"REGEXEXTRACT("abc","\d")"#, Value::Empty),
    ]);
}

#[test]
fn text_formats_numbers_and_dates() {
    all(&[
        ("TEXT(1234.5,\"0\")", s("1235")), ("TEXT(1234.5,\"0.00\")", s("1234.50")), ("TEXT(1234567.891,\"#,##0\")", s("1,234,568")), ("TEXT(1234567.891,\"#,##0.00\")", s("1,234,567.89")),
        ("TEXT(0.256,\"0%\")", s("26%")), ("TEXT(0.256,\"0.0%\")", s("25.6%")), ("TEXT(-5,\"0.00\")", s("-5.00")), ("TEXT(-0.001,\"0.00\")", s("0.00")), ("TEXT(7,\"000\")", s("007")),
        ("TEXT(12.5,\"¥0.00\")", s("¥12.50")), ("TEXT(3,\"0 元\")", s("3 元")), ("TEXT(1.5,\"0.##\")", s("1.5")), ("TEXT(2,\"0.##\")", s("2")), ("TEXT(999,\"#,##0\")", s("999")),
        ("TEXT(1.005,\"0.00\")", s("1.01")), ("TEXT(\"abc\",\"0\")", s("abc")), ("TEXT({备注},\"0\")", s("")), ("TEXT(5,\"\")", s("5")),
        ("TEXT({日期},\"YYYY-MM-DD\")", s("2026-03-15")), ("TEXT({日期},\"YYYY/MM/DD\")", s("2026/03/15")), ("TEXT({日期},\"YYYY年M月D日\")", s("2026年3月15日")),
        ("TEXT({日期},\"HH:mm\")", s("09:30")), ("TEXT({日期},\"YYYY-MM-DD HH:mm\")", s("2026-03-15 09:30")), ("TEXT({日期},\"MM-DD\")", s("03-15")),
        ("TEXT({日期},\"yyyy-mm-dd hh:mm:ss\")", s("2026-03-15 09:30:00")), ("TEXT({日期},\"YY/M/D H:m\")", s("26/3/15 9:30")), ("TEXT({日期},\"DDDD\")", s("星期日")), ("TEXT({日期},\"DDD\")", s("周日")),
        ("TEXT({日期},\"mm:ss\")", s("30:00")), ("TEXT(DATE(2026,11,7),\"M/D\")", s("11/7")),
    ]);
}

#[test]
fn date_functions() {
    all(&[
        ("TODAY()", d(2026, 11, 17)), ("NOW()", dt(2026, 11, 17, 10, 30)), ("DATE(2026,3,15)", d(2026, 3, 15)), ("DATE(2026,13,1)", d(2027, 1, 1)), ("DATE(2026,3,0)", d(2026, 2, 28)),
        ("DATE(2026,3,15,9,30)", dt(2026, 3, 15, 9, 30)), ("DATE(99999,1,1)", e("#NUM!")),
        ("YEAR({日期})", n(2026.)), ("MONTH({日期})", n(3.)), ("DAY({日期})", n(15.)), ("HOUR({日期})", n(9.)), ("MINUTE({日期})", n(30.)), ("SECOND({日期})", n(0.)), ("QUARTER({日期})", n(1.)),
        ("SECOND(DATEADD({日期}, 42, \"seconds\"))", n(42.)), ("QUARTER(DATE(2026,12,1))", n(4.)),
        ("YEAR(\"2025-06-01\")", n(2025.)), ("YEAR({备注})", Value::Empty), ("YEAR(\"abc\")", e("#VALUE!")), ("YEAR(5)", e("#VALUE!")),
        ("WEEKDAY({日期})", n(1.)), ("WEEKDAY({日期},2)", n(7.)), ("WEEKDAY({日期},3)", n(6.)), ("WEEKDAY(DATE(2026,11,17))", n(3.)), ("WEEKDAY(DATE(2026,11,17),2)", n(2.)), ("WEEKDAY({日期},9)", e("#NUM!")),
        ("WEEKNUM(DATE(2026,1,1))", n(1.)), ("WEEKNUM(DATE(2026,1,3))", n(1.)), ("WEEKNUM(DATE(2026,1,4))", n(2.)), ("WEEKNUM(DATE(2026,1,4),2)", n(1.)), ("WEEKNUM(DATE(2026,1,5),2)", n(2.)),
        ("WEEKNUM(DATE(2026,12,31))", n(53.)), ("WEEKNUM(DATE(2026,1,1),21)", n(1.)), ("WEEKNUM(DATE(2027,1,1),21)", n(53.)), ("WEEKNUM(DATE(2024,12,30),21)", n(1.)),
        ("DATEDIF(DATE(2024,1,31), DATE(2026,3,15), \"Y\")", n(2.)), ("DATEDIF(DATE(2024,1,31), DATE(2026,3,15), \"M\")", n(25.)), ("DATEDIF(DATE(2024,1,31), DATE(2026,3,15), \"D\")", n(774.)),
        ("DATEDIF(DATE(2024,1,31), DATE(2026,3,15), \"YM\")", n(1.)), ("DATEDIF(DATE(2024,1,31), DATE(2026,3,15), \"MD\")", n(12.)), ("DATEDIF(DATE(2024,1,31), DATE(2026,3,15), \"YD\")", n(43.)),
        ("DATEDIF(DATE(2026,1,10), DATE(2026,3,15), \"md\")", n(5.)), ("DATEDIF(DATE(2025,6,1), DATE(2026,3,15), \"YD\")", n(287.)), ("DATEDIF(\"2026-01-01\", \"2026-01-15\", \"W\")", n(2.)),
        ("DATEDIF(DATE(2026,3,15), DATE(2024,1,31), \"D\")", e("#NUM!")), ("DATEDIF(DATE(2024,1,31), DATE(2026,3,15), \"X\")", e("#VALUE!")), ("DATEDIF({备注}, TODAY(), \"D\")", Value::Empty),
        ("DAYS(DATE(2026,3,15), DATE(2026,3,1))", n(14.)), ("DAYS({日期}, \"2026-03-16\")", n(-1.)),
        ("DATEADD(DATE(2026,1,31),1,\"months\")", d(2026, 2, 28)), ("DATEADD(DATE(2026,1,31),10,\"days\")", d(2026, 2, 10)), ("DATEADD(DATE(2026,1,31),1,\"weeks\")", d(2026, 2, 7)),
        ("DATEADD(DATE(2026,1,31),1,\"years\")", d(2027, 1, 31)), ("DATEADD(DATE(2026,1,31),5,\"hours\")", dt(2026, 1, 31, 5, 0)), ("DATEADD(DATE(2026,1,31),90,\"minutes\")", dt(2026, 1, 31, 1, 30)),
        ("DATEADD(DATE(2024,2,29),1,\"Year\")", d(2025, 2, 28)), ("DATEADD(DATE(2026,1,31),-1,\"天\")", d(2026, 1, 30)), ("DATEADD(DATE(2026,1,31),1,\"eons\")", e("#VALUE!")),
        ("EDATE(DATE(2026,1,31),1)", d(2026, 2, 28)), ("EDATE(DATE(2026,1,31),-2)", d(2025, 11, 30)), ("EDATE({日期},12)", dt(2027, 3, 15, 9, 30)),
        ("EOMONTH(DATE(2026,1,15),1)", d(2026, 2, 28)), ("EOMONTH(DATE(2026,1,15),0)", d(2026, 1, 31)), ("EOMONTH(DATE(2024,3,31),-1)", d(2024, 2, 29)),
        ("WORKDAY(DATE(2026,11,17), 5)", d(2026, 11, 24)), ("WORKDAY(DATE(2026,11,17), 4)", d(2026, 11, 23)), ("WORKDAY(DATE(2026,11,17), -2)", d(2026, 11, 13)), ("WORKDAY(DATE(2026,11,17), 0)", d(2026, 11, 17)),
        ("WORKDAY(DATE(2026,11,14), 1)", d(2026, 11, 16)), ("WORKDAY(DATE(2026,11,14), 5)", d(2026, 11, 20)), ("WORKDAY(DATE(2026,11,15), -5)", d(2026, 11, 9)), ("WORKDAY(DATE(2026,11,17), 23)", d(2026, 12, 18)),
        ("WORKDAY(DATE(2026,11,17), 5, DATE(2026,11,20))", d(2026, 11, 25)),
        ("NETWORKDAYS(DATE(2026,11,16), DATE(2026,11,22))", n(5.)), ("NETWORKDAYS(DATE(2026,11,17), DATE(2026,11,30))", n(10.)), ("NETWORKDAYS(DATE(2026,11,30), DATE(2026,11,17))", n(-10.)),
        ("NETWORKDAYS(DATE(2026,11,17), DATE(2026,11,30), DATE(2026,11,20))", n(9.)), ("NETWORKDAYS(DATE(2026,11,14), DATE(2026,11,15))", n(0.)), ("NETWORKDAYS(DATE(2026,11,17), DATE(2026,11,17))", n(1.)),
        ("DATEVALUE(\"2026-3-15\")", d(2026, 3, 15)), ("DATEVALUE(\"2026年3月15日 9:30\")", dt(2026, 3, 15, 9, 30)), ("DATEVALUE(\"abc\")", e("#VALUE!")),
        ("TIMESTR({日期})", s("09:30:00")), ("DATESTR({日期})", s("2026-03-15")), ("ISTODAY(NOW())", T), ("ISTODAY({日期})", F),
    ]);
}

#[test]
fn other_functions() {
    all(&[
        ("RECORD_ID()", s("1")), ("CREATED_TIME()", dt(2026, 1, 1, 8, 0)), ("LAST_MODIFIED_TIME()", dt(2026, 2, 2, 9, 0)), ("record_id()", s("1")),
        ("ARRAYJOIN({标签}, \"/\")", s("红/蓝")), ("ARRAYJOIN({引用})", s("1, 2, 3")), ("ARRAYJOIN({客户}, \"\")", s("甲公司乙公司")), ("ARRAYJOIN({文本}, \"-\")", s("Hello")),
        ("ARRAYUNIQUE(1,2,1,\"a\",\"a\")", Value::List(vec![n(1.), n(2.), s("a")])), ("ARRAYUNIQUE({备注})", Value::List(vec![])),
        ("COUNTIF({引用}, \">1\")", n(2.)), ("COUNTIF({引用}, \">=1\")", n(3.)), ("COUNTIF({引用}, \"<>2\")", n(2.)), ("COUNTIF({引用}, 2)", n(1.)), ("COUNTIF({标签}, \"红\")", n(1.)), ("COUNTIF({标签}, \"=蓝\")", n(1.)),
        ("FIRST({引用})", n(1.)), ("LAST({引用})", n(3.)), ("FIRST({备注})", Value::Empty), ("LAST(1, 2, {备注})", n(2.)), ("FIRST({标签})", s("红")),
    ]);
}

#[test]
fn every_listed_function_compiles_and_runs() {
    let fs = functions();
    assert!(fs.len() >= 100);
    for (i, f) in fs.iter().enumerate() {
        assert!(["逻辑", "数字", "文本", "日期", "其它"].contains(&f.group), "{}", f.name);
        assert!(f.sig.starts_with(&format!("{}(", f.name)) && f.sig.ends_with(')'), "{}", f.sig);
        assert!(!f.desc.is_empty());
        assert!(!fs[..i].iter().any(|g| g.name == f.name), "{} twice", f.name);
        let (_, min, _) = funcs::lookup(f.name).unwrap();
        let src = format!("{}({})", f.name, vec!["1"; min].join(", "));
        assert_ne!(ev(&src), e("#ERROR!"), "{src}");
        assert_ne!(ev(&src.to_lowercase()), e("#ERROR!"), "{src}");
    }
}

#[test]
fn renaming_a_field_rewrites_only_references() {
    for (src, old, new, want) in [
        ("{数量}*2", "数量", "件数", "{件数}*2"),
        ("[数量] + 数量 + \"数量\" + SUM(数量, '数量')", "数量", "件数", "[件数] + 件数 + \"数量\" + SUM(件数, '数量')"),
        ("  {数量}  +\n\t数量 ", "数量", "件 数", "  {件 数}  +\n\t{件 数} "),
        ("数量+[数量]", "数量", "2 件", "{2 件}+[2 件]"),
        ("SUM(SUM) + {SUM}", "SUM", "总计", "SUM(总计) + {总计}"),
        ("{数量}+[数量]+数量", "数量", "a}b", "[a}b]+[a}b]+[a}b]"),
        ("[数量]+{数量}", "数量", "a]b", "{a]b}+{a]b}"),
        ("{数量} + \"abc", "数量", "件数", "{件数} + \"abc"),
        ("数量2 + {数量2} + 数量", "数量", "件数", "数量2 + {数量2} + 件数"),
        ("{ 数量 } + {单价}", "数量", "件数", "{件数} + {单价}"),
        ("数量 * 2", "数量", "true", "{true} * 2"),
        ("IF({a}>1，“a”，{a})", "a", "b", "IF({b}>1，“a”，{b})"),
        ("", "a", "b", ""),
    ] {
        assert_eq!(rename_field(src, old, new), want, "{src}");
    }
    // what comes out still compiles to the same thing
    let mut b = base();
    let before = compile("数量*[单价]+{数量}", &b.tables[0]).unwrap();
    let src = rename_field("数量*[单价]+{数量}", "数量", "件 数");
    b.tables[0].fields.iter_mut().find(|f| f.id == 100).unwrap().name = "件 数".into();
    assert_eq!(compile(&src, &b.tables[0]).unwrap(), before);
}
