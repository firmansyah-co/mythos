//! ANSI/ISA-88 Recipe Model & PackML Phase State Machine.
//!
//! Provides decoupled Formula parameters and Procedure execution models
//! complying with ANSI/ISA-88.01-2010 and ISA-TR88.00.02 (PackML).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RecipeLifecycleState {
    Draft,
    InReview,
    Approved,
    Released,
    Archived,
}

impl std::fmt::Display for RecipeLifecycleState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Draft => write!(f, "DRAFT"),
            Self::InReview => write!(f, "IN_REVIEW"),
            Self::Approved => write!(f, "APPROVED"),
            Self::Released => write!(f, "RELEASED"),
            Self::Archived => write!(f, "ARCHIVED"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum ParameterValue {
    Float(f64),
    Integer(i64),
    Boolean(bool),
    DurationSeconds(u64),
    Text(String),
}

impl ParameterValue {
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Float(v) => Some(*v),
            Self::Integer(v) => Some(*v as f64),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParameterConstraint {
    pub name: String,
    pub description: String,
    pub unit: String,
    pub min_value: Option<f64>,
    pub max_value: Option<f64>,
    pub default_value: ParameterValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StepTransitionCondition {
    TimerComplete,
    ProcessTagThreshold {
        tag_name: String,
        target_value: u64,
        operator: String,
    },
    OperatorConfirmationPrompt {
        prompt_text: String,
    },
    PlcPhaseCompleteSignal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecipeStep {
    pub step_index: u16,
    pub step_name: String,
    pub phase_identifier: String,
    pub step_timeout_seconds: u32,
    pub step_parameters: BTreeMap<String, ParameterValue>,
    pub transition_condition: StepTransitionCondition,
}

/// Master Recipe conforming to ANSI/ISA-88.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MasterRecipe {
    pub recipe_id: String,
    pub revision: String,
    pub title: String,
    pub author_user_id: String,
    pub state: RecipeLifecycleState,
    pub created_at_utc: String,
    pub formula: BTreeMap<String, ParameterValue>,
    pub steps: Vec<RecipeStep>,
    pub digital_signature_sha256: Option<String>,
}

impl MasterRecipe {
    /// Validates all formula entries against strict boundary constraints.
    pub fn validate_constraints(&self, constraints: &[ParameterConstraint]) -> Result<(), String> {
        for c in constraints {
            if let Some(val) = self.formula.get(&c.name) {
                if let Some(num) = val.as_f64() {
                    if let Some(min) = c.min_value {
                        if num < min {
                            return Err(format!(
                                "Parameter {} value {} < min {}",
                                c.name, num, min
                            ));
                        }
                    }
                    if let Some(max) = c.max_value {
                        if num > max {
                            return Err(format!(
                                "Parameter {} value {} > max {}",
                                c.name, num, max
                            ));
                        }
                    }
                }
            } else {
                return Err(format!("Missing mandatory parameter: {}", c.name));
            }
        }
        Ok(())
    }

    /// Computes deterministic SHA-256 fingerprint for recipe parameter verification.
    pub fn compute_content_digest(&self) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(self.recipe_id.as_bytes());
        hasher.update(self.revision.as_bytes());
        for (k, v) in &self.formula {
            hasher.update(k.as_bytes());
            hasher.update(format!("{:?}", v).as_bytes());
        }
        for s in &self.steps {
            hasher.update(s.step_name.as_bytes());
            hasher.update(s.phase_identifier.as_bytes());
        }
        format!("{:x}", hasher.finalize())
    }
}

/// PackML Machine and Unit States per ISA-TR88.00.02.
#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PackMlState {
    Undefined = 0,
    Clearing = 1,
    Stopped = 2,
    Starting = 3,
    Idle = 4,
    Suspended = 5,
    Execute = 6,
    Stopping = 7,
    Aborting = 8,
    Aborted = 9,
    Holding = 10,
    Held = 11,
    Unholding = 12,
    Suspending = 13,
    Unsuspending = 14,
    Resetting = 15,
    Completing = 16,
    Complete = 17,
}

impl std::fmt::Display for PackMlState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Undefined => write!(f, "UNDEFINED"),
            Self::Clearing => write!(f, "CLEARING"),
            Self::Stopped => write!(f, "STOPPED"),
            Self::Starting => write!(f, "STARTING"),
            Self::Idle => write!(f, "IDLE"),
            Self::Suspended => write!(f, "SUSPENDED"),
            Self::Execute => write!(f, "EXECUTE"),
            Self::Stopping => write!(f, "STOPPING"),
            Self::Aborting => write!(f, "ABORTING"),
            Self::Aborted => write!(f, "ABORTED"),
            Self::Holding => write!(f, "HOLDING"),
            Self::Held => write!(f, "HELD"),
            Self::Unholding => write!(f, "UNHOLDING"),
            Self::Suspending => write!(f, "SUSPENDING"),
            Self::Unsuspending => write!(f, "UNSUSPENDING"),
            Self::Resetting => write!(f, "RESETTING"),
            Self::Completing => write!(f, "COMPLETING"),
            Self::Complete => write!(f, "COMPLETE"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BatchRunStatus {
    PendingSync,
    Synchronizing,
    Running,
    Held,
    Completing,
    Completed,
    Aborted,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchExecutionInstance {
    pub batch_id: String,
    pub recipe_id: String,
    pub recipe_revision: String,
    pub equipment_unit_id: String,
    pub started_by_user_id: String,
    pub start_timestamp_ns: u64,
    pub active_step_index: u16,
    pub packml_state: PackMlState,
    pub status: BatchRunStatus,
}
