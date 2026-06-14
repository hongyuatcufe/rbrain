use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum SuggestedAction {
    RegisterInput {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        hint: Option<String>,
    },
    RegisterArtifact {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        hint: Option<String>,
    },
    LinkEvidence {
        from: String,
        to: String,
        link_type: String,
    },
    AddCitation {
        slug: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        chunk_ref: Option<String>,
    },
    RecordLimitation {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        finding_slug: Option<String>,
    },
    RerunAnalysis {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    RefineArtifact {
        slug: String,
    },
}

impl SuggestedAction {
    pub fn action_kind(&self) -> &'static str {
        match self {
            Self::RegisterInput { .. } => "register_input",
            Self::RegisterArtifact { .. } => "register_artifact",
            Self::LinkEvidence { .. } => "link_evidence",
            Self::AddCitation { .. } => "add_citation",
            Self::RecordLimitation { .. } => "record_limitation",
            Self::RerunAnalysis { .. } => "rerun_analysis",
            Self::RefineArtifact { .. } => "refine_artifact",
        }
    }

    pub fn target_slugs(&self) -> Vec<String> {
        match self {
            Self::RegisterInput { .. } | Self::RegisterArtifact { .. } => Vec::new(),
            Self::LinkEvidence { from, to, .. } => {
                if to.trim().is_empty() {
                    vec![from.clone()]
                } else {
                    vec![from.clone(), to.clone()]
                }
            }
            Self::AddCitation { slug, .. } => vec![slug.clone()],
            Self::RecordLimitation { finding_slug } => finding_slug.clone().into_iter().collect(),
            Self::RerunAnalysis { .. } => Vec::new(),
            Self::RefineArtifact { slug } => vec![slug.clone()],
        }
    }
}
