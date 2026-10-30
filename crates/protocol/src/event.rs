//! Agent → client events: the single source of truth. TUI, app-server, JSONL rollout,
//! SQLite index and telemetry are all *projections* of this stream.
//!
//! Events are additive-only. Clients must ignore variants they do not know
//! (hence [`Event::Unknown`]).

use crate::ids::{CallId, EventId, SessionId, ThreadId, TurnId, WorkerId};
use crate::permission::{Decision, PermissionRequest};
use crate::tool::{ToolCall, ToolError, ToolResult};
use crate::usage::Usage;
use serde::{Deserialize, Serialize};

/// Bumped only for *breaking* envelope changes; payload evolution is additive.
pub const EVENT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventEnvelope {
    pub id: EventId,
    pub thread_id: ThreadId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<TurnId>,
    /// Unix milliseconds.
    pub ts: i64,
    pub schema_version: u32,
    #[serde(flatten)]
    pub event: Event,
}

impl EventEnvelope {
    pub fn new(thread_id: ThreadId, turn_id: Option<TurnId>, event: Event) -> Self {
        Self {
            id: EventId::new(),
            thread_id,
            turn_id,
            ts: crate::ids::now_millis(),
            schema_version: EVENT_SCHEMA_VERSION,
            event,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnOutcome {
    Completed,
    BudgetStop,
    MaxTurns,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnState {
    Idle,
    Assembling,
    Streaming,
    Approving,
    Executing,
    Verifying,
    Retrying,
    Compacting,
    Completed,
    Cancelled,
    Failed,
}

impl TurnState {
    pub fn as_str(&self) -> &'static str {
        match self {
            TurnState::Idle => "idle",
            TurnState::Assembling => "assembling",
            TurnState::Streaming => "streaming",
            TurnState::Approving => "approving",
            TurnState::Executing => "executing",
            TurnState::Verifying => "verifying",
            TurnState::Retrying => "retrying",
            TurnState::Compacting => "compacting",
            TurnState::Completed => "completed",
            TurnState::Cancelled => "cancelled",
            TurnState::Failed => "failed",
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            TurnState::Completed | TurnState::Cancelled | TurnState::Failed
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompactStage {
    BudgetReduce,
    Snip,
    Dedupe,
    Collapse,
    Summarize,
    SubagentIsolate,
}

impl CompactStage {
    pub fn as_str(&self) -> &'static str {
        match self {
            CompactStage::BudgetReduce => "budget_reduce",
            CompactStage::Snip => "snip",
            CompactStage::Dedupe => "dedupe",
            CompactStage::Collapse => "collapse",
            CompactStage::Summarize => "summarize",
            CompactStage::SubagentIsolate => "subagent_isolate",
        }
    }
}

/// One event. Families: turn, content, tools, context, workers, system.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    // ---- turn -------------------------------------------------------------
    TurnStarted {
        #[serde(default)]
        prompt_preview: String,
        #[serde(default)]
        model: String,
    },
    TurnCompleted {
        outcome: TurnOutcome,
        #[serde(default)]
        turns: u32,
        #[serde(default)]
        usage: Usage,
    },
    TurnFailed {
        code: String,
        message: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        remediation: Option<String>,
    },
    TurnCancelled {
        #[serde(default)]
        reason: String,
    },
    StateChanged {
        from: TurnState,
        to: TurnState,
    },

    // ---- content ----------------------------------------------------------
    TextDelta {
        text: String,
    },
    ThinkingDelta {
        text: String,
    },
    TextCompleted {
        text: String,
    },
    ContentReplaced {
        reason: String,
        #[serde(default)]
        summary: String,
    },

    // ---- tools ------------------------------------------------------------
    ToolRequested {
        call: ToolCall,
    },
    ToolApprovalNeeded {
        request: PermissionRequest,
    },
    ToolApprovalDecided {
        call_id: CallId,
        decision: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rule: Option<String>,
    },
    ToolStarted {
        call_id: CallId,
        name: String,
    },
    ToolOutputDelta {
        call_id: CallId,
        chunk: String,
    },
    ToolCompleted {
        result: ToolResult,
    },
    ToolFailed {
        call_id: CallId,
        error: ToolError,
    },
    ToolDenied {
        call_id: CallId,
        reason: String,
    },
    ToolApproved {
        call_id: CallId,
        #[serde(default)]
        remembered: bool,
    },

    // ---- context ----------------------------------------------------------
    ContextAssembled {
        system_tokens: u32,
        tools_tokens: u32,
        history_tokens: u32,
        total_tokens: u32,
        window: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cache_boundary_index: Option<usize>,
    },
    CompactStarted {
        from_tokens: u32,
        target_tokens: u32,
    },
    CompactStageRun {
        stage: CompactStage,
        tokens_before: u32,
        tokens_after: u32,
    },
    CompactCompleted {
        from_tokens: u32,
        to_tokens: u32,
        #[serde(default)]
        stages: Vec<String>,
    },
    MemoryLoaded {
        path: String,
        bytes: u64,
    },

    // ---- workers ----------------------------------------------------------
    WorkerSpawned {
        worker_id: WorkerId,
        goal: String,
        #[serde(default)]
        profile: String,
    },
    WorkerProgress {
        worker_id: WorkerId,
        message: String,
        #[serde(default)]
        percent: u8,
    },
    WorkerReported {
        worker_id: WorkerId,
        summary: String,
        #[serde(default)]
        files_changed: Vec<String>,
    },
    WorkerCancelled {
        worker_id: WorkerId,
        reason: String,
    },
    MergeQueued {
        worker_id: WorkerId,
        branch: String,
    },
    MergeCompleted {
        worker_id: WorkerId,
        #[serde(default)]
        conflicts: Vec<String>,
    },
    VerifyResult {
        passed: bool,
        #[serde(default)]
        output: String,
    },

    // ---- system -----------------------------------------------------------
    UsageUpdated {
        usage: Usage,
    },
    CostUpdated {
        cost_micros: u64,
        session_cost_micros: u64,
    },
    Warning {
        code: String,
        message: String,
    },
    Error {
        code: String,
        message: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        remediation: Option<String>,
    },
    ConfigChanged {
        key: String,
        layer: String,
    },
    SessionResumed {
        session_id: SessionId,
        turns: u32,
    },
    /// Forward compatibility: an event this build does not understand.
    #[serde(other)]
    Unknown,
}

impl Event {
    /// Family name, used for filtering in the TUI and for metrics labels.
    pub fn family(&self) -> &'static str {
        match self {
            Event::TurnStarted { .. }
            | Event::TurnCompleted { .. }
            | Event::TurnFailed { .. }
            | Event::TurnCancelled { .. }
            | Event::StateChanged { .. } => "turn",
            Event::TextDelta { .. }
            | Event::ThinkingDelta { .. }
            | Event::TextCompleted { .. }
            | Event::ContentReplaced { .. } => "content",
            Event::ToolRequested { .. }
            | Event::ToolApprovalNeeded { .. }
            | Event::ToolApprovalDecided { .. }
            | Event::ToolStarted { .. }
            | Event::ToolOutputDelta { .. }
            | Event::ToolCompleted { .. }
            | Event::ToolFailed { .. }
            | Event::ToolDenied { .. }
            | Event::ToolApproved { .. } => "tools",
            Event::ContextAssembled { .. }
            | Event::CompactStarted { .. }
            | Event::CompactStageRun { .. }
            | Event::CompactCompleted { .. }
            | Event::MemoryLoaded { .. } => "context",
            Event::WorkerSpawned { .. }
            | Event::WorkerProgress { .. }
            | Event::WorkerReported { .. }
            | Event::WorkerCancelled { .. }
            | Event::MergeQueued { .. }
            | Event::MergeCompleted { .. }
            | Event::VerifyResult { .. } => "workers",
            Event::UsageUpdated { .. }
            | Event::CostUpdated { .. }
            | Event::Warning { .. }
            | Event::Error { .. }
            | Event::ConfigChanged { .. }
            | Event::SessionResumed { .. }
            | Event::Unknown => "system",
        }
    }
}

/// Convenience: the decision label an approval event carries.
pub fn decision_label(d: &Decision) -> &'static str {
    d.label()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_flattens_event_and_stamps_version() {
        let env = EventEnvelope::new(
            ThreadId::new(),
            None,
            Event::TextDelta {
                text: "hi".into(),
            },
        );
        let v = serde_json::to_value(&env).unwrap();
        assert_eq!(v["type"], "text_delta");
        assert_eq!(v["schema_version"], EVENT_SCHEMA_VERSION);
        assert!(v["id"].as_str().unwrap().starts_with("evt_"));
        assert!(v["ts"].as_i64().unwrap() > 0);
    }

    #[test]
    fn unknown_event_is_forward_compatible() {
        let e: Event = serde_json::from_str(r#"{"type":"hologram","x":1}"#).unwrap();
        assert_eq!(e, Event::Unknown);
    }

    #[test]
    fn families_are_assigned() {
        assert_eq!(
            Event::ToolDenied {
                call_id: CallId::new(),
                reason: "denied".into()
            }
            .family(),
            "tools"
        );
        assert_eq!(Event::Unknown.family(), "system");
        assert_eq!(
            Event::CompactStageRun {
                stage: CompactStage::Snip,
                tokens_before: 10,
                tokens_after: 5
            }
            .family(),
            "context"
        );
    }

    #[test]
    fn event_roundtrips_through_json_lines() {
        let env = EventEnvelope::new(
            ThreadId::new(),
            Some(TurnId::new()),
            Event::ToolCompleted {
                result: ToolResult::ok(
                    &ToolCall::new("read_file", serde_json::json!({"path":"a"})),
                    "body",
                ),
            },
        );
        let line = serde_json::to_string(&env).unwrap();
        let back: EventEnvelope = serde_json::from_str(&line).unwrap();
        assert_eq!(env, back);
    }
}

