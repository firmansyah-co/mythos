//! Master project representation, metadata, and task configuration.

use serde::{Deserialize, Serialize};

/// Master project configuration descriptor (`project.active.json` & `project.draft.json`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProjectConfig {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub created_at_utc: String,
    #[serde(default)]
    pub updated_at_utc: String,
}

impl ProjectConfig {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            version: "1.0.0".to_string(),
            description: String::new(),
            author: "MythOS Engineer".to_string(),
            created_at_utc: String::new(),
            updated_at_utc: String::new(),
        }
    }
}

/// Task configuration descriptor (`tasks.active.json` & `tasks.draft.json`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TaskConfig {
    pub name: String,
    pub interval_ms: u32,
    pub priority: u32,
    pub core_affinity: Option<usize>,
}
