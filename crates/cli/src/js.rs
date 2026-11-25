//! `quiet run`: a script in QuickJS against the `lib` API (prelude.js).
//! The engine has no files, network or processes: the library is reached
//! only through the session, so a script can do no more than the commands
//! can. Time and memory are capped; a failure leaves the session uncommitted.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use rquickjs::context::EvalOptions;
use rquickjs::{CatchResultExt, CaughtError, Context, Function, Promise, Runtime, Value as Js};
use serde_json::{Value, json};

use crate::fail::{Fail, R, fail};
use crate::session::Session;

const PRELUDE: &str = include_str!("prelude.js");

fn engine(e: rquickjs::Error) -> Fail { fail("script_engine", e.to_string()) }

/// Run `code`; what it prints goes to `out`.
pub fn run(mut session: Session, code: &str, timeout_secs: u64, out: &mut String) -> (Session, R<()>) {
    session.sandbox = true;
    let shared = Rc::new(RefCell::new(session));
    let printed = Rc::new(RefCell::new(String::new()));
    let r = eval(&shared, &printed, code, timeout_secs);
    let mut session = Rc::try_unwrap(shared).ok().expect("the script engine is gone").into_inner();
    session.sandbox = false;
    out.push_str(&printed.borrow());
    (session, r)
}

fn eval(shared: &Rc<RefCell<Session>>, printed: &Rc<RefCell<String>>, code: &str, timeout_secs: u64) -> R<()> {
    let rt = Runtime::new().map_err(engine)?;
    rt.set_memory_limit(512 << 20);
    rt.set_max_stack_size(4 << 20);
    let deadline = Instant::now() + Duration::from_secs(timeout_secs.max(1));
    let late = Rc::new(std::cell::Cell::new(false));
    let flag = late.clone();
    rt.set_interrupt_handler(Some(Box::new(move || { let over = Instant::now() > deadline; if over { flag.set(true); } over })));
    let ctx = Context::full(&rt).map_err(engine)?;
    let s = shared.clone();
    let r = ctx.with(|ctx| -> R<()> {
        let g = ctx.globals();
        let q = Function::new(ctx.clone(), move |op: String, args: String| -> String {
            let a: Value = serde_json::from_str(&args).unwrap_or_else(|_| json!({}));
            match s.borrow_mut().call(&op, &a) { Ok(v) => json!({ "ok": v }).to_string(), Err(e) => e.json().to_string() }
        }).map_err(engine)?;
        g.set("__q", q).map_err(engine)?;
        let p = printed.clone();
        g.set("__print", Function::new(ctx.clone(), move |s: String| {
            let mut o = p.borrow_mut();
            const CAP: usize = 2 << 20;
            if o.len() < CAP { o.push_str(&s); o.push('\n'); if o.len() >= CAP { o.push_str("…[output cut at 2 MB: print less]\n"); } }
        }).map_err(engine)?).map_err(engine)?;
        g.set("__guide", Function::new(ctx.clone(), || crate::GUIDE.to_string()).map_err(engine)?).map_err(engine)?;
        let named = |f: &str| { let mut o = EvalOptions::default(); o.strict = false; o.filename = Some(f.to_string()); o };
        ctx.eval_with_options::<(), _>(PRELUDE, named("prelude.js")).catch(&ctx).map_err(|e| fail("script_engine", format!("prelude: {e}")))?;
        let wrapped = format!("(async () => {{\n{code}\n}})()");
        let p: Promise = ctx.eval_with_options(wrapped, named("script.js")).catch(&ctx).map_err(caught)?;
        let v: Js = p.finish().catch(&ctx).map_err(caught)?;
        if !v.is_undefined() {
            let show: Function = ctx.eval("(v) => __print(typeof v === 'string' ? v : JSON.stringify(v))").map_err(engine)?;
            show.call::<_, ()>((v,)).catch(&ctx).map_err(caught)?;
        }
        Ok(())
    });
    if late.get() { return Err(fail("timeout", format!("the script ran past {timeout_secs}s; nothing was written")).hint("do less per run, or pass --timeout SECS")); }
    r
}

/// The script's own line in a stack frame (`script.js:7:17`), less the
/// line the async wrapper adds; frames in the prelude are skipped.
fn script_line(frame: &str) -> Option<usize> {
    let rest = frame.split("script.js:").nth(1)?;
    rest.split(':').next()?.parse::<usize>().ok().map(|n| n.saturating_sub(1)).filter(|n| *n > 0)
}

/// A thrown error as the CLI reports it: the API's own code and hint when
/// it came from `lib`, else `script_error` with the message and where.
fn caught(e: CaughtError) -> Fail {
    match e {
        CaughtError::Exception(x) => {
            let o = x.as_object();
            let code = o.get::<_, Option<String>>("code").ok().flatten();
            let hint = o.get::<_, Option<String>>("hint").ok().flatten();
            let msg = x.message().unwrap_or_default();
            let at = x.stack().unwrap_or_default().lines().find_map(script_line).map(|n| format!(" (line {n})")).unwrap_or_default();
            let f = fail(code.unwrap_or_else(|| "script_error".into()), format!("{msg}{at}; nothing was written"));
            match hint { Some(h) => f.hint(h), None => f }
        }
        CaughtError::Value(v) => fail("script_error", format!("thrown: {v:?}; nothing was written")),
        CaughtError::Error(rquickjs::Error::WouldBlock) => fail("script_error", "the script awaited something that never finishes; nothing was written"),
        CaughtError::Error(e) => fail("script_engine", e.to_string()),
    }
}
