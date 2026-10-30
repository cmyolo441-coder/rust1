//! Provider-neutral message IR. Adapters translate; nothing provider-specific leaks in here.

use crate::tool::ToolCall;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

impl Role {
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::Tool => "tool",
        }
    }
}

/// One block of content. Images carry a blob handle, never raw bytes in the event stream.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentPart {
    Text {
        text: String,
    },
    Thinking {
        text: String,
    },
    Image {
        media_type: String,
        /// Reference to a content-addressed blob; bytes stay out of the transcript.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        blob: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        base64: Option<String>,
    },
    ToolUse {
        call: ToolCall,
    },
    ToolResult {
        call_id: crate::ids::CallId,
        name: String,
        content: String,
        #[serde(default)]
        is_error: bool,
    },
}

impl ContentPart {
    pub fn text(text: impl Into<String>) -> Self {
        ContentPart::Text { text: text.into() }
    }

    pub fn is_text(&self) -> bool {
        matches!(self, ContentPart::Text { .. })
    }

    /// Text projection used for token estimation, FTS indexing and replay assertions.
    pub fn plain_text(&self) -> String {
        match self {
            ContentPart::Text { text } | ContentPart::Thinking { text } => text.clone(),
            ContentPart::Image { media_type, .. } => format!("[image {media_type}]"),
            ContentPart::ToolUse { call } => format!("[tool_use {} {}]", call.name, call.args),
            ContentPart::ToolResult {
                name,
                content,
                is_error,
                ..
            } => {
                if *is_error {
                    format!("[tool_result {name} ERROR] {content}")
                } else {
                    format!("[tool_result {name}] {content}")
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    #[serde(default)]
    pub parts: Vec<ContentPart>,
}

impl Message {
    pub fn new(role: Role, parts: Vec<ContentPart>) -> Self {
        Self { role, parts }
    }

    pub fn system(text: impl Into<String>) -> Self {
        Self::new(Role::System, vec![ContentPart::text(text)])
    }

    pub fn user(text: impl Into<String>) -> Self {
        Self::new(Role::User, vec![ContentPart::text(text)])
    }

    pub fn assistant(text: impl Into<String>) -> Self {
        Self::new(Role::Assistant, vec![ContentPart::text(text)])
    }

    pub fn assistant_tool_calls(text: Option<String>, calls: Vec<ToolCall>) -> Self {
        let mut parts = Vec::new();
        if let Some(t) = text {
            if !t.is_empty() {
                parts.push(ContentPart::text(t));
            }
        }
        parts.extend(calls.into_iter().map(|call| ContentPart::ToolUse { call }));
        Self::new(Role::Assistant, parts)
    }

    pub fn tool_result(
        call_id: crate::ids::CallId,
        name: impl Into<String>,
        content: impl Into<String>,
        is_error: bool,
    ) -> Self {
        Self::new(
            Role::Tool,
            vec![ContentPart::ToolResult {
                call_id,
                name: name.into(),
                content: content.into(),
                is_error,
            }],
        )
    }

    pub fn text(&self) -> String {
        self.parts
            .iter()
            .filter_map(|p| match p {
                ContentPart::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("")
    }

    pub fn tool_calls(&self) -> Vec<ToolCall> {
        self.parts
            .iter()
            .filter_map(|p| match p {
                ContentPart::ToolUse { call } => Some(call.clone()),
                _ => None,
            })
            .collect()
    }

    pub fn plain_text(&self) -> String {
        self.parts
            .iter()
            .map(ContentPart::plain_text)
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn has_tool_use(&self) -> bool {
        self.parts
            .iter()
            .any(|p| matches!(p, ContentPart::ToolUse { .. }))
    }
}

/// The request the gateway sends: ordered messages plus the tool schemas that are live.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelRequest {
    pub model: String,
    pub messages: Vec<Message>,
    #[serde(default)]
    pub tools: Vec<ToolSchema>,
    #[serde(default)]
    pub max_output_tokens: u32,
    #[serde(default)]
    pub temperature: f32,
    /// Index into `messages` where the cacheable prefix ends (stable → volatile boundary).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_boundary: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolSchema {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

impl ToolSchema {
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        input_schema: serde_json::Value,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            input_schema,
        }
    }
}

/// One decoded streaming chunk from any provider.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StreamChunk {
    Text {
        text: String,
    },
    Thinking {
        text: String,
    },
    /// A partial tool call: `args_delta` may be invalid JSON until the stream ends.
    ToolDelta {
        index: usize,
        id: Option<String>,
        name: Option<String>,
        args_delta: String,
    },
    Usage {
        usage: crate::usage::Usage,
    },
    Done {
        finish_reason: FinishReason,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    Stop,
    ToolUse,
    Length,
    ContentFilter,
    Error,
    Unknown,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn tool_calls_are_extractable_and_text_join_is_lossless() {
        let calls = vec![ToolCall::new("read_file", json!({"path": "a.rs"}))];
        let m = Message::assistant_tool_calls(Some("looking".into()), calls);
        assert_eq!(m.text(), "looking");
        assert_eq!(m.tool_calls().len(), 1);
        assert!(m.has_tool_use());
        assert!(m.plain_text().contains("[tool_use read_file"));
    }

    #[test]
    fn empty_assistant_text_is_not_pushed_as_part() {
        let m = Message::assistant_tool_calls(Some(String::new()), vec![]);
        assert!(m.parts.is_empty());
    }

    #[test]
    fn tool_result_part_projects_error_marker() {
        let m = Message::tool_result(crate::ids::CallId::new(), "shell", "boom", true);
        assert!(m.plain_text().contains("ERROR"));
    }
}

