use std::fmt;

/// Stable error codes exposed by the initial API surface.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ErrorCode {
    InvalidRequest,
    RevisionInvalid,
    ToolUnavailable,
    SkillDenied,
    ModelUnavailable,
}

impl ErrorCode {
    /// Returns the stable snake_case wire value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidRequest => "invalid_request",
            Self::RevisionInvalid => "revision_invalid",
            Self::ToolUnavailable => "tool_unavailable",
            Self::SkillDenied => "skill_denied",
            Self::ModelUnavailable => "model_unavailable",
        }
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}
