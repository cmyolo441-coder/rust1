//! Tool call / result / error taxonomy.
//!
//! The error shape is part of the product: `retryable` drives the scheduler, `hint` is what
//! the model reads to self-correct, and `kind` is what policy and telemetry key on.

use crate::ids::CallId;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Risk class — every tool declares exactly one, and policy decisions are a function of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum RiskClass {
    /// Reads workspace files.
    Read,
    /// Mutates workspace files.
    Mutate,
    /// Runs a child process.
    Exec,
    /// Performs egress.
    Net,
    /// Spawns another agent.
    Spawn,
    /// Mutates agent-internal state (plan/todo).
    State,
    /// Asks the human something.
    Meta,
}

impl RiskClass {
    pub fn as_str(&self) -> &'static str {
        match self {
            RiskClass::Read => "read",
            RiskClass::Mutate => "mutate",
            RiskClass::Exec => "exec",
            RiskClass::Net => "net",
            RiskClass::Spawn => "spawn",
            RiskClass::State => "state",
            RiskClass::Meta => "meta",
        }
    }

    /// Whether calls of this class may run concurrently. Mutating work stays serial.
    pub fn parallel_safe(&self) -> bool {
        matches!(self, RiskClass::Read | RiskClass::Meta)
    }
}

/// A model-issued tool invocation, parsed but not yet policy-checked.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: CallId,
    pub name: String,
    #[serde(default)]
    pub args: Value,
}

impl ToolCall {
    pub fn new(name: impl Into<String>, args: Value) -> Self {
        Self {
            id: CallId::new(),
            name: name.into(),
            args,
        }
    }

    /// Stable string form used for policy matching, dedupe and grant patterns.
    ///
    /// Shape is `tool(selector)`, where the selector is the *meaningful* argument:
    /// the command for `shell`, the path for file tools, the host for `web_fetch`.
    /// This is what the policy grammar in §10.4 matches against.
    pub fn canonical(&self) -> String {
        format!("{}({})", self.name, self.selector())
    }

    /// The single argument that identifies this call for policy purposes.
    pub fn selector(&self) -> String {
        if let Some(host) = self.host() {
            return host;
        }
        for key in ["command", "path", "file", "pattern", "goal", "question"] {
            if let Some(v) = self.arg_str(key) {
                return v.to_string();
            }
        }
        canonical_args(&self.args)
    }

    /// Host component of `url`, used by `net(...)` rules.
    pub fn host(&self) -> Option<String> {
        let url = self.arg_str("url")?;
        let after_scheme = url.split_once("://").map(|(_, rest)| rest).unwrap_or(url);
        let host = after_scheme
            .split(['/', '?', '#'])
            .next()
            .unwrap_or(after_scheme);
        let host = host.rsplit_once('@').map(|(_, h)| h).unwrap_or(host);
        let host = host.split(':').next().unwrap_or(host);
        Some(host.to_string())
    }

    /// Command words for exec tools (`shell`, `git`), else empty.
    pub fn argv(&self) -> Vec<String> {
        self.arg_str("command")
            .map(|c| c.split_whitespace().map(str::to_string).collect())
            .unwrap_or_default()
    }

    pub fn arg_str(&self, key: &str) -> Option<&str> {
        self.args.get(key).and_then(|v| v.as_str())
    }

    pub fn arg_u64(&self, key: &str) -> Option<u64> {
        self.args.get(key).and_then(|v| v.as_u64())
    }

    pub fn arg_bool(&self, key: &str) -> Option<bool> {
        self.args.get(key).and_then(|v| v.as_bool())
    }
}

/// Deterministic rendering of args for pattern matching: keys sorted, strings unquoted so a
/// shell command reads like `shell(git status)` and not like a JSON blob.
pub fn canonical_args(args: &Value) -> String {
    match args {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::Array(items) => items
            .iter()
            .map(canonical_args)
            .collect::<Vec<_>>()
            .join(" "),
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            keys.iter()
                .map(|k| format!("{k}={}", canonical_args(&map[*k])))
                .collect::<Vec<_>>()
                .join(",")
        }
    }
}

/// Extra, machine-readable facts about a result. Never truncated away.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ToolMeta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub bytes: u64,
    #[serde(default)]
    pub truncated: bool,
    #[serde(default)]
    pub duration_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matched: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub changed: Option<bool>,
}


/// The shaped result that goes back to the model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolResult {
    pub call_id: CallId,
    pub tool: String,
    pub ok: bool,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub structured: Option<Value>,
    #[serde(default)]
    pub meta: ToolMeta,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ToolError>,
}

impl ToolResult {
    pub fn ok(call: &ToolCall, content: impl Into<String>) -> Self {
        let content = content.into();
        Self {
            call_id: call.id.clone(),
            tool: call.name.clone(),
            ok: true,
            meta: ToolMeta {
                bytes: content.len() as u64,
                changed: Some(true),
                ..Default::default()
            },
            content,
            structured: None,
            error: None,
        }
    }

    pub fn err(call: &ToolCall, error: ToolError) -> Self {
        let content = error.render();
        Self {
            call_id: call.id.clone(),
            tool: call.name.clone(),
            ok: false,
            meta: ToolMeta {
                bytes: content.len() as u64,
                ..Default::default()
            },
            content,
            structured: None,
            error: Some(error),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolErrorKind {
    InvalidArgs,
    NotFound,
    PermissionDenied,
    Timeout,
    Io,
    Sandbox,
    Network,
    Cancelled,
    Internal,
}

impl ToolErrorKind {
    /// Codes are stable; docs and support scripts key on them.
    pub fn code(&self) -> &'static str {
        match self {
            ToolErrorKind::InvalidArgs => "tool.invalid_args",
            ToolErrorKind::NotFound => "tool.not_found",
            ToolErrorKind::PermissionDenied => "tool.permission_denied",
            ToolErrorKind::Timeout => "tool.timeout",
            ToolErrorKind::Io => "tool.io",
            ToolErrorKind::Sandbox => "tool.sandbox",
            ToolErrorKind::Network => "tool.network",
            ToolErrorKind::Cancelled => "tool.cancelled",
            ToolErrorKind::Internal => "tool.internal",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolError {
    pub kind: ToolErrorKind,
    pub message: String,
    #[serde(default)]
    pub retryable: bool,
    /// Actionable next step, rendered into the result the model sees.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

impl ToolError {
    pub fn new(kind: ToolErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            retryable: false,
            hint: None,
        }
    }

    pub fn retryable(mut self) -> Self {
        self.retryable = true;
        self
    }

    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    pub fn render(&self) -> String {
        match &self.hint {
            Some(hint) => format!(
                "error[{}]: {}\nhint: {}",
                self.kind.code(),
                self.message,
                hint
            ),
            None => format!("error[{}]: {}", self.kind.code(), self.message),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn canonical_form_is_tool_and_meaningful_selector() {
        let a = ToolCall::new("shell", json!({"command": "git status"}));
        assert_eq!(a.canonical(), "shell(git status)");
        assert_eq!(a.argv(), vec!["git", "status"]);

        let b = ToolCall::new("read_file", json!({"limit": 40, "path": "src/lib.rs"}));
        assert_eq!(b.canonical(), "read_file(src/lib.rs)");

        let c = ToolCall::new("web_fetch", json!({"url": "https://user@api.example.com:443/x"}));
        assert_eq!(c.selector(), "api.example.com");

        let d = ToolCall::new("plan", json!({"items": ["a", "b"]}));
        assert_eq!(d.canonical(), "plan(items=a b)");
    }

    #[test]
    fn tool_error_renders_hint() {
        let e = ToolError::new(ToolErrorKind::NotFound, "no such file: a.rs")
            .with_hint("call glob to list files first");
        assert!(e.render().contains("tool.not_found"));
        assert!(e.render().contains("hint: call glob"));
    }

    #[test]
    fn read_is_parallel_safe_but_mutate_is_not() {
        assert!(RiskClass::Read.parallel_safe());
        assert!(!RiskClass::Mutate.parallel_safe());
        assert!(!RiskClass::Exec.parallel_safe());
    }
}
