use serde::{Deserialize, Serialize};

/// Long-lived Session lifecycle.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionStatus {
    Active,
    Idle,
    Closed,
    Archived,
}

impl SessionStatus {
    /// Validates one Session status transition.
    #[must_use]
    pub const fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Active, Self::Idle | Self::Closed)
                | (Self::Idle, Self::Active | Self::Closed)
                | (Self::Closed, Self::Archived)
        )
    }
}

/// Long-lived user and Agent conversation container.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Session {
    pub id: String,
    pub tenant: String,
    pub agent: String,
    pub user: String,
    pub status: SessionStatus,
    pub created_at: String,
    pub last_active_at: String,
}

#[cfg(test)]
mod tests {
    use super::{Session, SessionStatus};

    #[test]
    fn parses_core_session_fixture() {
        let session: Session =
            serde_json::from_str(include_str!("../fixtures/core/session.json")).unwrap();
        assert_eq!(session.id, "ses-example-18");
        assert_eq!(session.status, SessionStatus::Active);
    }
}
