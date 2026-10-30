//! Full serde config schema. Every key has a default and a doc comment; unknown keys are a
//! hard error so typos cannot silently disable safety settings.

use serde::{Deserialize, Serialize};

/// Derived per-field defaults keep the schema readable: `Config::default()` is the source of
/// truth for "what happens if the user never writes a config file".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub model: ModelConfig,
    pub permissions: PermissionsConfig,
    pub sandbox: SandboxConfig,
    pub context: ContextConfig,
    pub budget: BudgetConfig,
    pub telemetry: TelemetryConfig,
    pub ui: UiConfig,
    pub agents: AgentsConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            model: ModelConfig::default(),
            permissions: PermissionsConfig::default(),
            sandbox: SandboxConfig::default(),
            context: ContextConfig::default(),
            budget: BudgetConfig::default(),
            telemetry: TelemetryConfig::default(),
            ui: UiConfig::default(),
            agents: AgentsConfig::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ModelConfig {
    /// Main coding model, `provider/model`.
    pub main: String,
    /// Cheaper model for compaction/summaries; falls back to `main`.
    pub summarizer: Option<String>,
    pub temperature: f32,
    pub max_output_tokens: u32,
    /// BYO endpoint for air-gapped deployments.
    pub base_url: Option<String>,
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            main: "anthropic/claude-sonnet-4-5".into(),
            summarizer: None,
            temperature: 0.0,
            max_output_tokens: 8_000,
            base_url: None,
        }
    }
}

/// Permission modes, in ascending order of autonomy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum PermissionMode {
    /// Read-only: no mutation, no exec. Used for review/plan.
    Plan,
    /// Ask before every mutating/exec/net action. The default.
    #[default]
    Ask,
    /// Auto-approve file edits inside the workspace; ask for exec/net.
    AcceptEdits,
    /// Auto-approve everything except hard deny rules.
    Auto,
    /// Disable approvals entirely — requires an explicit flag and is always audited.
    Bypass,
}

impl PermissionMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            PermissionMode::Plan => "plan",
            PermissionMode::Ask => "ask",
            PermissionMode::AcceptEdits => "accept-edits",
            PermissionMode::Auto => "auto",
            PermissionMode::Bypass => "bypass",
        }
    }

    /// Bypass needs a non-interactive, non-production escape hatch; callers must pass the flag.
    pub fn requires_flag(&self) -> bool {
        matches!(self, PermissionMode::Bypass)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PermissionsConfig {
    pub mode: PermissionMode,
    /// Patterns that are always allowed, e.g. `read(src/**)`, `shell(cargo test **)`.
    pub allow: Vec<String>,
    /// Patterns that always require a human, even in `auto`.
    pub ask: Vec<String>,
    /// Patterns that are never allowed. Deny always wins.
    pub deny: Vec<String>,
    /// Session grant TTL in minutes; 0 disables grants.
    pub grant_ttl_minutes: u32,
}

impl Default for PermissionsConfig {
    fn default() -> Self {
        Self {
            mode: PermissionMode::Ask,
            allow: vec!["read(**)".into(), "list_dir(**)".into()],
            ask: vec![],
            deny: vec![
                "shell(rm -rf **)".into(),
                "shell(sudo **)".into(),
                "read(~/.ssh/**)".into(),
                "read(**/.env)".into(),
            ],
            grant_ttl_minutes: 480,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum NetworkPolicy {
    #[default]
    Deny,
    LocalhostOnly,
    Allowlist,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SandboxConfig {
    pub enabled: bool,
    pub network: NetworkPolicy,
    /// Roots the agent may write. Relative paths resolve against the workspace.
    pub writable_roots: Vec<String>,
    pub readable_roots: Vec<String>,
    pub allowed_hosts: Vec<String>,
    /// Child processes get only these env vars.
    pub env_allowlist: Vec<String>,
    pub max_memory_mb: u32,
    pub max_pids: u32,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            network: NetworkPolicy::Deny,
            writable_roots: vec![".".into()],
            readable_roots: vec![".".into()],
            allowed_hosts: vec![],
            env_allowlist: vec![
                "PATH".into(),
                "HOME".into(),
                "LANG".into(),
                "TERM".into(),
                "TMPDIR".into(),
            ],
            max_memory_mb: 2048,
            max_pids: 256,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ContextConfig {
    /// Model context window in tokens.
    pub window: u32,
    /// Tokens held back for the model's answer.
    pub reserve_output: u32,
    /// Tokenizer-error margin; estimates always under-report.
    pub reserve_safety: u32,
    /// Fixed schema budget for model-facing tools.
    pub max_tool_schema_tokens: u32,
    /// Compact when estimated usage crosses this fraction of the usable window.
    pub compaction_threshold: f32,
    /// Turns kept verbatim through a compaction.
    pub compact_keep_recent_turns: usize,
    pub repo_map_tokens: u32,
    pub memory_max_tokens: u32,
}

impl Default for ContextConfig {
    fn default() -> Self {
        Self {
            window: 200_000,
            reserve_output: 16_000,
            reserve_safety: 4_000,
            max_tool_schema_tokens: 8_000,
            compaction_threshold: 0.80,
            compact_keep_recent_turns: 4,
            repo_map_tokens: 2_048,
            memory_max_tokens: 6_000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BudgetConfig {
    /// Hard stop on model turns per user prompt.
    pub max_turns: u32,
    pub max_tool_calls: u32,
    /// Hard stop on spend for the session; 0 = unlimited.
    pub max_cost_micros: u64,
    /// Wall-clock limit for a single turn.
    pub turn_timeout_secs: u64,
}

impl Default for BudgetConfig {
    fn default() -> Self {
        Self {
            max_turns: 40,
            max_tool_calls: 200,
            max_cost_micros: 0,
            turn_timeout_secs: 900,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TelemetryConfig {
    /// Opt-in. Off by default: local-first product.
    pub enabled: bool,
    pub otlp_endpoint: Option<String>,
    pub log_level: String,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            otlp_endpoint: None,
            log_level: "info".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct UiConfig {
    pub show_thinking: bool,
    pub diff_style: String,
    /// Linear, spinner-free output for screen readers and pipes.
    pub accessible: bool,
    pub word_wrap: bool,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            show_thinking: false,
            diff_style: "unified".into(),
            accessible: false,
            word_wrap: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AgentsConfig {
    /// Upper bound on concurrent workers.
    pub max_parallel: u8,
    pub default_profile: String,
    pub worker_max_turns: u32,
    /// Isolation level for workers: `L1`..`L3`.
    pub isolation: String,
}

impl Default for AgentsConfig {
    fn default() -> Self {
        Self {
            max_parallel: 4,
            default_profile: "implement".into(),
            worker_max_turns: 20,
            isolation: "L2".into(),
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_safe() {
        let c = Config::default();
        assert_eq!(c.permissions.mode, PermissionMode::Ask);
        assert!(c.sandbox.enabled);
        assert_eq!(c.sandbox.network, NetworkPolicy::Deny);
        assert!(!c.telemetry.enabled, "telemetry must be opt-in");
        assert!(c.permissions.deny.iter().any(|r| r.contains("rm -rf")));
        assert!(c.permissions.deny.iter().any(|r| r.contains("sudo")));
    }

    #[test]
    fn unknown_keys_are_rejected() {
        let err = serde_json::from_str::<Config>(r#"{"model":{"mainn":"x"}}"#).unwrap_err();
        assert!(err.to_string().contains("unknown field"), "got: {err}");
    }

    #[test]
    fn partial_config_keeps_defaults() {
        let c: Config =
            serde_json::from_str(r#"{"permissions":{"mode":"plan"}}"#).unwrap();
        assert_eq!(c.permissions.mode, PermissionMode::Plan);
        assert_eq!(c.context.window, 200_000);
        assert!(!c.permissions.deny.is_empty(), "deny rules survive a partial config");
    }

    #[test]
    fn mode_serde_is_kebab_case() {
        let json = serde_json::to_string(&PermissionMode::AcceptEdits).unwrap();
        assert_eq!(json, "\"accept-edits\"");
        assert!(PermissionMode::Bypass.requires_flag());
        assert!(!PermissionMode::Ask.requires_flag());
    }

    #[test]
    fn config_roundtrips_through_json() {
        let c = Config::default();
        let json = serde_json::to_string(&c).unwrap();
        let back: Config = serde_json::from_str(&json).unwrap();
        assert_eq!(c, back);
    }
}

