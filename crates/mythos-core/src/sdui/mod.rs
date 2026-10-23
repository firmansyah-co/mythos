//! Server-Driven UI (SDUI) schema models and automated TypeScript bindings (TSK-302 / ADR-006).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// HMI Screen Lifecycle modes conforming to ISA-101 / TIA Portal staging pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum HmiLifecycleMode {
    /// Sandboxed draft buffer under active editing (uncommitted).
    Edit,
    /// Non-actuating simulated preview with mock telemetry.
    Preview,
    /// Production active runtime mode executing on target controller.
    Deploy,
    /// Live production operational mode.
    Runtime,
}

/// Standardized industrial component types for Server-Driven UI presentation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum ComponentType {
    Button,
    Slider,
    FaceplateCard,
    TrendChart,
    DiagramNode,
    MonacoEditor,
    AlarmTable,
    MetricDisplay,
    ToggleSwitch,
    InterlockMatrix,
    Custom,
}

/// Dynamic property declaration for an SDUI component.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SduiProperty {
    pub key: String,
    pub value: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub readonly: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color_token: Option<String>,
}

/// Real-time TagBus binding configuration for faceplates and components.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct FaceplateBindingConfig {
    pub tag_uri: String,
    pub property_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub direction: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deadband: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scale: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format_pattern: Option<String>,
}

/// Declarative Server-Driven UI component definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SduiComponent {
    pub id: String,
    pub component_type: ComponentType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default)]
    pub properties: Vec<SduiProperty>,
    #[serde(default)]
    pub bindings: Vec<FaceplateBindingConfig>,
    #[serde(default)]
    pub children: Vec<SduiComponent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub y: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visible: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

/// Multi-zone layout section conforming to Dockview 5-Zone industrial shell.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SduiZone {
    pub id: String,
    pub name: String,
    pub zone_index: u32,
    #[serde(default)]
    pub components: Vec<SduiComponent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collapsible: Option<bool>,
}

/// Complete Server-Driven UI Screen Layout schema.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SduiScreenLayout {
    pub screen_id: String,
    pub title: String,
    pub version: String,
    pub lifecycle_mode: HmiLifecycleMode,
    #[serde(default)]
    pub zones: Vec<SduiZone>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background_color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_interval_ms: Option<u64>,
}

/// In-canvas logic deployment request payload (`POST /api/v1/control/deploy`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DeploymentRequest {
    /// IEC 61131-3 Structured Text or Ladder logic source code.
    pub code: String,
    /// Programming language (e.g. "ST", "LD", "CPP").
    pub language: String,
    /// Target execution block or task identifier (e.g. "MainTask", "OB1").
    pub target_block: String,
    /// Whether to perform atomic hot-reloading into running controller.
    pub hot_reload: bool,
}

/// Result of an in-controller deployment operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DeploymentResponse {
    /// Whether the compilation and staging/deployment succeeded.
    pub success: bool,
    /// In-controller compilation duration in milliseconds.
    pub compilation_time_ms: u64,
    /// Informational message or compiler diagnostic output.
    pub message: String,
    /// Deterministic SHA-256 hash of the compiled binary artifact.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binary_hash: Option<String>,
}
