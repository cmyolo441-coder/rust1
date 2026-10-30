//! Client → agent operations. Ops are the *only* way to change agent state.
//!
//! Each op carries an [`OpId`] so retries are idempotent; unknown ops deserialize to
//! [`Op::Unknown`] so a newer client cannot crash an older agent.

use crate::config_types::PermissionMode;
use crate::ids::{RequestId, RunId, TurnId, WorkerId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Attachment {
    pub kind: String,
    /// Path or URL; content is loaded by the context engine, not by the transport.
    pub value: String,
    #[serde(default)]
    pub media_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Op {
    /// Start a turn. Idempotent by `OpId`.
    Submit {
        prompt: String,
        #[serde(default)]
        attachments: Vec<Attachment>,
        #[serde(default)]
        mode: Option<PermissionMode>,
        #[serde(default)]
        model: Option<String>,
    },
    /// Inject guidance at the next tool boundary of a running turn.
    Steer { turn_id: TurnId, text: String },
    /// Cancel a turn and every child of it.
    Interrupt { turn_id: TurnId, #[serde(default)] reason: Option<String> },
    /// Answer an approval request; the request id is single-use.
    Approve {
        request_id: RequestId,
        #[serde(default)]
        remember: bool,
    },
    /// Refuse an approval request.
    Deny {
        request_id: RequestId,
        #[serde(default)]
        reason: Option<String>,
    },
    /// Force a compaction pass, optionally focused on a topic.
    Compact {
        #[serde(default)]
        turn_id: Option<TurnId>,
        #[serde(default)]
        focus: Option<String>,
    },
    /// Change permission mode. Audited.
    SetMode { mode: PermissionMode },
    /// Spawn a parallel worker DAG.
    SpawnWorkers {
        #[serde(default)]
        goal: String,
        #[serde(default)]
        max_parallel: Option<u8>,
    },
    /// Kill switch for parallel mode.
    CancelWorkers {
        run_id: RunId,
        #[serde(default)]
        worker_id: Option<WorkerId>,
    },
    /// Flush state and exit.
    Shutdown {
        #[serde(default)]
        turn_id: Option<TurnId>,
    },
    /// Forward compatibility: an op this build does not understand.
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpEnvelope {
    pub op_id: crate::ids::OpId,
    #[serde(default)]
    pub thread_id: Option<crate::ids::ThreadId>,
    #[serde(flatten)]
    pub op: Op,
}

impl OpEnvelope {
    pub fn new(op: Op) -> Self {
        Self {
            op_id: crate::ids::OpId::new(),
            thread_id: None,
            op,
        }
    }
}

impl Op {
    /// Ops that are safe to replay on a transport retry.
    pub fn is_idempotent(&self) -> bool {
        matches!(
            self,
            Op::Submit { .. }
                | Op::Compact { .. }
                | Op::SetMode { .. }
                | Op::Shutdown { .. }
                | Op::Interrupt { .. }
        )
    }

    /// Every op that mutates agent state needs an audit entry.
    pub fn is_audited(&self) -> bool {
        matches!(
            self,
            Op::Approve { .. }
                | Op::Deny { .. }
                | Op::SetMode { .. }
                | Op::CancelWorkers { .. }
        )
    }

    pub fn name(&self) -> &'static str {
        match self {
            Op::Submit { .. } => "submit",
            Op::Steer { .. } => "steer",
            Op::Interrupt { .. } => "interrupt",
            Op::Approve { .. } => "approve",
            Op::Deny { .. } => "deny",
            Op::Compact { .. } => "compact",
            Op::SetMode { .. } => "set_mode",
            Op::SpawnWorkers { .. } => "spawn_workers",
            Op::CancelWorkers { .. } => "cancel_workers",
            Op::Shutdown { .. } => "shutdown",
            Op::Unknown => "unknown",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn submit_roundtrips_with_tag() {
        let op = Op::Submit {
            prompt: "fix the bug".into(),
            attachments: vec![],
            mode: None,
            model: None,
        };
        let json = serde_json::to_string(&op).unwrap();
        assert!(json.contains("\"op\":\"submit\""));
        let back: Op = serde_json::from_str(&json).unwrap();
        assert_eq!(op, back);
    }

    #[test]
    fn unknown_op_is_forward_compatible() {
        let op: Op = serde_json::from_str(r#"{"op":"quantum_teleport","x":1}"#).unwrap();
        assert_eq!(op, Op::Unknown);
    }

    #[test]
    fn envelope_flattens_op() {
        let env = OpEnvelope::new(Op::SetMode {
            mode: PermissionMode::Plan,
        });
        let json = serde_json::to_value(&env).unwrap();
        assert_eq!(json["op"], "set_mode");
        assert_eq!(json["mode"], "plan");
        assert!(json["op_id"].as_str().unwrap().starts_with("op_"));
    }

    #[test]
    fn audit_and_idempotency_are_declared_per_op() {
        let mode = Op::SetMode {
            mode: PermissionMode::Auto,
        };
        assert!(mode.is_audited());
        assert!(mode.is_idempotent());
        let approve = Op::Approve {
            request_id: RequestId::new(),
            remember: true,
        };
        assert!(approve.is_audited());
        assert!(!approve.is_idempotent());
    }
}
