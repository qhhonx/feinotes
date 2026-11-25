//! `quiet mcp`: the CLI as an MCP server (JSON-RPC 2.0, one message per
//! line on stdin/stdout) for agents with no shell. Two tools only, so the
//! schemas cost little context: `quiet` runs a command line, `quiet_run` a
//! script. Each call opens the library afresh, as a command would.

use std::io::{BufRead, Write};

use serde_json::{Value, json};

use crate::{Input, execute};

const INSTRUCTIONS: &str = "Quiet.md is a notes library of documents, boards and data tables. Read the guide first: call `quiet` with args [\"skill\"]. Use `quiet` for one step (outline, read a section, insert…) and `quiet_run` for anything with a loop or several steps.";

fn tools() -> Value {
    json!([
        {
            "name": "quiet",
            "description": "Run one `quiet` command on the Quiet.md library and get its output, e.g. [\"outline\", \"周报\"], [\"find\", \"budget\"], [\"insert\", \"周报\", \"--after\", \"#3~k7f2\", \"--md\", \"- [ ] item\"]. [\"help\"] lists commands; [\"skill\"] prints the full guide. An option value of \"-\" reads `stdin`.",
            "inputSchema": { "type": "object", "properties": {
                "args": { "type": "array", "items": { "type": "string" }, "description": "The command line after `quiet`, one argument per item" },
                "stdin": { "type": "string", "description": "Text for an option given as -, e.g. --md -" }
            }, "required": ["args"] }
        },
        {
            "name": "quiet_run",
            "description": "Run a JavaScript script against the library's `lib` API (see the guide's Script API) as one transaction: nothing is written if it throws. Return or print a small result.",
            "inputSchema": { "type": "object", "properties": {
                "code": { "type": "string" },
                "dry_run": { "type": "boolean", "description": "Show what would change, write nothing" },
                "timeout": { "type": "number", "description": "Seconds, default 60" }
            }, "required": ["code"] }
        }
    ])
}

/// A tool call as a command line; its output and whether it failed.
fn call(name: &str, args: &Value, library: &[String]) -> (String, bool) {
    let mut raw: Vec<String> = library.to_vec();
    let mut input = None;
    match name {
        "quiet" => {
            let Some(list) = args.get("args").and_then(Value::as_array) else { return ("args: a list of strings".into(), true) };
            raw.extend(list.iter().map(|v| v.as_str().map_or_else(|| v.to_string(), str::to_string)));
            input = args.get("stdin").and_then(Value::as_str).map(str::to_string);
            if raw.get(library.len()).is_some_and(|c| c == "mcp") { return ("already serving".into(), true); }
        }
        "quiet_run" => {
            let Some(code) = args.get("code").and_then(Value::as_str) else { return ("code: the script".into(), true) };
            raw.extend(["run".to_string(), "-e".to_string(), code.to_string()]);
            if args.get("dry_run").and_then(Value::as_bool) == Some(true) { raw.push("--dry-run".into()); }
            if let Some(t) = args.get("timeout").and_then(Value::as_u64) { raw.extend(["--timeout".to_string(), t.to_string()]); }
        }
        _ => return (format!("no tool {name:?}"), true),
    }
    let name = crate::usage::name(&raw);
    let mut out = String::new();
    let r = execute(raw, Input(Some(input.unwrap_or_default())), &mut out);
    crate::usage::count(&name, r.is_ok());
    match r {
        Ok(()) => (out, false),
        Err(e) => { if !out.is_empty() && !out.ends_with('\n') { out.push('\n'); } out += &e.text(); (out, true) }
    }
}

fn answer(id: &Value, result: Value) -> Value { json!({ "jsonrpc": "2.0", "id": id, "result": result }) }

fn error(id: &Value, code: i64, message: &str) -> Value { json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } }) }

/// One message in, the reply (None for a notification).
fn handle(msg: &Value, library: &[String]) -> Option<Value> {
    let id = msg.get("id")?.clone();
    let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
    let params = msg.get("params").cloned().unwrap_or(json!({}));
    Some(match method {
        "initialize" => {
            let version = params.get("protocolVersion").and_then(Value::as_str).unwrap_or("2025-06-18");
            answer(&id, json!({ "protocolVersion": version, "capabilities": { "tools": {} }, "serverInfo": { "name": "quiet", "version": env!("CARGO_PKG_VERSION") }, "instructions": INSTRUCTIONS }))
        }
        "ping" => answer(&id, json!({})),
        "tools/list" => answer(&id, json!({ "tools": tools() })),
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let (text, failed) = call(name, params.get("arguments").unwrap_or(&json!({})), library);
            answer(&id, json!({ "content": [{ "type": "text", "text": text }], "isError": failed }))
        }
        "resources/list" => answer(&id, json!({ "resources": [] })),
        "prompts/list" => answer(&id, json!({ "prompts": [] })),
        _ => error(&id, -32601, &format!("method not found: {method}")),
    })
}

pub fn serve(args: Vec<String>) -> i32 {
    // `--library DIR` applies to every call.
    let library: Vec<String> = ["--library"].iter().flat_map(|k| match args.iter().position(|a| a == k) { Some(i) if i + 1 < args.len() => vec![k.to_string(), args[i + 1].clone()], _ => vec![] }).collect();
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() { continue; }
        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(Value::Array(batch)) => { let r: Vec<Value> = batch.iter().filter_map(|m| handle(m, &library)).collect(); (!r.is_empty()).then(|| json!(r)) }
            Ok(msg) => handle(&msg, &library),
            Err(e) => Some(error(&Value::Null, -32700, &format!("parse error: {e}"))),
        };
        if let Some(r) = reply && writeln!(stdout, "{r}").and_then(|_| stdout.flush()).is_err() { break; }
    }
    0
}
