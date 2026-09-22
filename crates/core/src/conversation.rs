use std::error::Error;
use std::fmt::{self, Display, Formatter};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tool {
    Claude,
    Codex,
}

impl Display for Tool {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Claude => formatter.write_str("Claude"),
            Self::Codex => formatter.write_str("Codex"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "ConversationRefData")]
pub struct ConversationRef {
    tool: Tool,
    native_id: String,
}

impl ConversationRef {
    pub fn new(tool: Tool, native_id: impl Into<String>) -> Result<Self, ConversationRefError> {
        let native_id = native_id.into();

        if native_id.trim().is_empty() {
            return Err(ConversationRefError::EmptyNativeId);
        }

        Ok(Self { tool, native_id })
    }

    pub fn tool(&self) -> Tool {
        self.tool
    }

    pub fn native_id(&self) -> &str {
        &self.native_id
    }
}

#[derive(Deserialize)]
struct ConversationRefData {
    tool: Tool,
    native_id: String,
}

impl TryFrom<ConversationRefData> for ConversationRef {
    type Error = ConversationRefError;

    fn try_from(value: ConversationRefData) -> Result<Self, Self::Error> {
        Self::new(value.tool, value.native_id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConversationRefError {
    EmptyNativeId,
}

impl Display for ConversationRefError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyNativeId => formatter.write_str("native conversation ID cannot be empty"),
        }
    }
}

impl Error for ConversationRefError {}

#[cfg(test)]
mod tests {
    use super::{ConversationRef, ConversationRefError, Tool};

    #[test]
    fn creates_a_conversation_reference() {
        let conversation = ConversationRef::new(Tool::Claude, "session-123")
            .expect("a non-empty native ID should be valid");

        assert_eq!(conversation.tool(), Tool::Claude);
        assert_eq!(conversation.native_id(), "session-123");
    }

    #[test]
    fn rejects_a_blank_native_id() {
        let result = ConversationRef::new(Tool::Codex, "   ");

        assert_eq!(result, Err(ConversationRefError::EmptyNativeId));
    }

    #[test]
    fn rejects_a_blank_native_id_during_deserialisation() {
        let result =
            serde_json::from_str::<ConversationRef>(r#"{"tool":"codex","native_id":"  "}"#);

        assert!(result.is_err());
    }
}
