use serde::{Deserialize, Serialize};

use super::actions::SuggestedAction;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ValidatorStatus {
    Pass,
    Warn,
    Fail,
}

impl ValidatorStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Warn => "warn",
            Self::Fail => "fail",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatorResult {
    pub validator: String,
    pub status: ValidatorStatus,
    pub message: String,
    pub affected_slugs: Vec<String>,
    pub suggested_actions: Vec<SuggestedAction>,
}

impl ValidatorResult {
    pub fn pass(validator: &str) -> Self {
        Self {
            validator: validator.to_string(),
            status: ValidatorStatus::Pass,
            message: "passed".to_string(),
            affected_slugs: Vec::new(),
            suggested_actions: Vec::new(),
        }
    }

    pub fn warn(validator: &str, message: impl Into<String>) -> Self {
        Self {
            validator: validator.to_string(),
            status: ValidatorStatus::Warn,
            message: message.into(),
            affected_slugs: Vec::new(),
            suggested_actions: Vec::new(),
        }
    }

    pub fn fail(validator: &str, message: impl Into<String>) -> Self {
        Self {
            validator: validator.to_string(),
            status: ValidatorStatus::Fail,
            message: message.into(),
            affected_slugs: Vec::new(),
            suggested_actions: Vec::new(),
        }
    }

    pub fn with_affected(mut self, slugs: impl IntoIterator<Item = String>) -> Self {
        self.affected_slugs.extend(slugs);
        self
    }

    pub fn with_actions(mut self, actions: impl IntoIterator<Item = SuggestedAction>) -> Self {
        self.suggested_actions.extend(actions);
        self
    }
}
