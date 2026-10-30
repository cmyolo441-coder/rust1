//! Permission plane wire types: what the UI needs to show the *exact* thing being approved.

use crate::ids::RequestId;
use crate::tool::RiskClass;
use serde::{Deserialize, Serialize};

/// One approval request. `scope` is the reproducible pattern that a "always allow" grant binds to.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PermissionRequest {
    pub id: RequestId,
    pub tool: String,
    /// Short human preview, e.g. `rm -rf build/` — truncated by the UI, never by core.
    pub args_preview: String,
    /// Canonical, reproducible scope: `shell(rm -rf **)`.
    pub scope: String,
    pub risk: RiskClass,
    /// Which rule produced `Ask`, if any: `rule #4 shell(git push **) at .agent/config.toml:12`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule_hint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl PermissionRequest {
    pub fn new(
        tool: impl Into<String>,
        args_preview: impl Into<String>,
        scope: impl Into<String>,
        risk: RiskClass,
    ) -> Self {
        Self {
            id: RequestId::new(),
            tool: tool.into(),
            args_preview: args_preview.into(),
            scope: scope.into(),
            risk,
            rule_hint: None,
            reason: None,
        }
    }

    pub fn with_rule_hint(mut self, hint: impl Into<String>) -> Self {
        self.rule_hint = Some(hint.into());
        self
    }
}

/// The answer to a `PermissionRequest`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum Decision {
    Allow,
    Deny { reason: String },
    /// Needs a human; headless surfaces turn this into a denial with a hint.
    Ask,
    /// Allow now and remember the exact scope for the rest of the session.
    AllowAlways { pattern: String },
}

impl Decision {
    pub fn is_allow(&self) -> bool {
        matches!(self, Decision::Allow | Decision::AllowAlways { .. })
    }

    pub fn is_deny(&self) -> bool {
        matches!(self, Decision::Deny { .. })
    }

    pub fn label(&self) -> &'static str {
        match self {
            Decision::Allow => "allow",
            Decision::Deny { .. } => "deny",
            Decision::Ask => "ask",
            Decision::AllowAlways { .. } => "allow_always",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decisions_classify() {
        assert!(Decision::Allow.is_allow());
        assert!(Decision::AllowAlways { pattern: "x".into() }.is_allow());
        assert!(Decision::Deny { reason: "r".into() }.is_deny());
        assert!(!Decision::Ask.is_allow());
        assert_eq!(Decision::Ask.label(), "ask");
    }

    #[test]
    fn permission_request_scope_is_reproducible() {
        let r = PermissionRequest::new("shell", "git push", "shell(git push **)", RiskClass::Exec)
            .with_rule_hint("rule #4 at .agent/config.toml:12");
        assert_eq!(r.scope, "shell(git push **)");
        assert!(r.rule_hint.unwrap().contains("config.toml:12"));
    }
}
