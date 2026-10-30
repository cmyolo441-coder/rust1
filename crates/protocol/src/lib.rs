//! `protocol` — wire contracts shared by every other crate. Types only, zero logic.
//!
//! Stability rule: adding a field or an enum variant is additive (clients ignore what they
//! do not know); removing or renaming one is a breaking change and bumps
//! [`PROTOCOL_VERSION`].

pub mod config_types;
pub mod event;
pub mod ids;
pub mod message;
pub mod model;
pub mod op;
pub mod permission;
pub mod tool;
pub mod usage;

pub use config_types::{
    AgentsConfig, BudgetConfig, Config, ContextConfig, ModelConfig, NetworkPolicy,
    PermissionMode, PermissionsConfig, SandboxConfig, TelemetryConfig, UiConfig,
};
pub use event::{CompactStage, Event, EventEnvelope, TurnOutcome, TurnState, EVENT_SCHEMA_VERSION};
pub use ids::{
    now_millis, ulid_now, CallId, EventId, OpId, RequestId, RunId, SessionId, ThreadId, TurnId,
    WorkerId,
};
pub use message::{ContentPart, FinishReason, Message, ModelRequest, Role, StreamChunk, ToolSchema};
pub use model::{ModelCaps, ModelDescriptor, Price};
pub use op::{Attachment, Op, OpEnvelope};
pub use permission::{Decision, PermissionRequest};
pub use tool::{
    canonical_args, RiskClass, ToolCall, ToolError, ToolErrorKind, ToolMeta, ToolResult,
};
pub use usage::Usage;

/// Wire version of the op/event contract. Bumped only on breaking changes.
pub const PROTOCOL_VERSION: u32 = 1;

/// Human-readable name used in banners, doctor output and user agents.
pub const PRODUCT_NAME: &str = "agent";

#[cfg(test)]
mod tests {
    #[test]
    fn protocol_version_is_positive() {
        assert!(super::PROTOCOL_VERSION >= 1);
        assert_eq!(super::EVENT_SCHEMA_VERSION, super::PROTOCOL_VERSION);
    }
}
