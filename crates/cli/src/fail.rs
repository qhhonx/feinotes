//! Errors an agent can act on: a stable code, what happened, what to do next.

use serde_json::{Value, json};

#[derive(Debug)]
pub struct Fail {
    pub code: std::borrow::Cow<'static, str>,
    pub message: String,
    pub hint: Option<String>,
}

pub type R<T> = Result<T, Fail>;

pub fn fail(code: impl Into<std::borrow::Cow<'static, str>>, message: impl Into<String>) -> Fail { Fail { code: code.into(), message: message.into(), hint: None } }

impl Fail {
    pub fn hint(mut self, h: impl Into<String>) -> Self { self.hint = Some(h.into()); self }

    /// 2 usage, 3 nothing there, 4 the library moved under us, 1 the rest.
    pub fn exit(&self) -> i32 {
        match self.code.as_ref() {
            "usage" | "bad_args" => 2,
            "not_found" | "bad_ref" | "stale_ref" | "ambiguous" | "ambiguous_ref" => 3,
            "stale_rev" | "conflict" => 4,
            _ => 1,
        }
    }

    pub fn json(&self) -> Value { json!({ "error": { "code": self.code, "message": self.message, "hint": self.hint } }) }

    pub fn text(&self) -> String {
        let mut s = format!("error[{}]: {}", self.code, self.message);
        if let Some(h) = &self.hint { s.push_str(&format!("\nhint: {h}")); }
        s
    }
}

impl From<anyhow::Error> for Fail {
    fn from(e: anyhow::Error) -> Self { fail("io", format!("{e:#}")) }
}

impl From<std::io::Error> for Fail {
    fn from(e: std::io::Error) -> Self { fail("io", e.to_string()) }
}
