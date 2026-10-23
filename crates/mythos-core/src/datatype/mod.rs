//! Project-scoped IEC 61131-3 user-defined data type (DUT/UDT) registry.
//!
//! Until this module existed the backend had **no** data-type store: the only DUT model lived in
//! the frontend logic editor as a hardcoded sample list (`DEFAULT_SAMPLE_DATA_TYPES` in
//! `frontend/plugins/ui-logic-editor/src/variables/DataTypeEditor.tsx`), so an engineer authoring a
//! DUT got no persistence and saw two types that belonged to no project (SYS-REQ-098).
//!
//! The UDT representation itself is shared with faceplate types ([`crate::faceplate`]): a faceplate
//! type's inline `udt` is exactly a [`UdtDefinition`], the same shape a registry entry carries. That
//! is deliberate — it means a faceplate type can later reference a registry entry instead of
//! inlining one without a data migration, and it means there is one representation rather than two.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::Path;
use thiserror::Error;
use ts_rs::TS;

/// Kind of IEC 61131-3 user-defined type.
///
/// Mirrors the DUT model in `frontend/plugins/ui-logic-editor/src/variables/types.ts`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum UdtKind {
    Struct,
    Enum,
    Alias,
}

/// One named field of a STRUCT UDT.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct UdtMember {
    pub name: String,
    pub data_type: String,
}

/// Serialisable UDT definition.
///
/// Structural rather than a name reference: every consumer (faceplate bindings, the logic editor,
/// future compiler integration) validates against the members it can actually see.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct UdtDefinition {
    pub name: String,
    pub kind: UdtKind,
    #[serde(default)]
    pub members: Vec<UdtMember>,
    #[serde(default)]
    pub enum_constants: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_type: Option<String>,
}

impl UdtDefinition {
    /// Names a binding may legally target for this UDT kind.
    pub fn bindable_names(&self) -> BTreeSet<&str> {
        match self.kind {
            // ENUM values are the only addressable name; ALIAS members are carried by the
            // aliased base type and are not describable here, so bindings must not target one.
            UdtKind::Struct => self.members.iter().map(|m| m.name.as_str()).collect(),
            UdtKind::Enum => self.enum_constants.iter().map(String::as_str).collect(),
            UdtKind::Alias => BTreeSet::new(),
        }
    }
}

/// A registry entry: a named UDT, versioned like every other project-scoped definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct DataTypeEntry {
    pub id: String,
    /// `0` while the entry has never been released; assigned server-side on draft writes.
    pub version: u32,
    pub definition: UdtDefinition,
}

/// Summary of one entry for the editor's type picker, always read from the draft stage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct DataTypeSummary {
    pub id: String,
    pub name: String,
    pub kind: UdtKind,
    pub field_count: u32,
    pub active_version: Option<u32>,
    pub has_draft: bool,
}

#[derive(Debug, Error)]
pub enum DataTypeError {
    #[error("data type '{0}' not found")]
    NotFound(String),

    #[error("invalid data type: {0}")]
    Invalid(String),

    #[error("I/O error during data type storage: {0}")]
    Io(#[from] std::io::Error),

    #[error("data type JSON error: {0}")]
    Serialization(#[from] serde_json::Error),
}

const TYPES_FILE: &str = "types.json";

fn read_list<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<Vec<T>, DataTypeError> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(DataTypeError::Io(e)),
    }
}

fn validate_token(kind: &str, value: &str) -> Result<(), DataTypeError> {
    if value.is_empty() {
        return Err(DataTypeError::Invalid(format!("{kind} is required")));
    }
    if value.len() > 128 {
        return Err(DataTypeError::Invalid(format!(
            "{kind} exceeds 128 characters"
        )));
    }
    if !value
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b'.')
    {
        return Err(DataTypeError::Invalid(format!(
            "{kind} may only contain ASCII alphanumerics, '_', '-' and '.'"
        )));
    }
    Ok(())
}

/// Validate a UDT definition. Kind-specific structure is enforced here so an entry cannot be
/// stored in a state no consumer can use.
pub fn validate_definition(def: &UdtDefinition) -> Result<(), DataTypeError> {
    if def.name.trim().is_empty() {
        return Err(DataTypeError::Invalid("data type name is required".into()));
    }
    validate_token("data type name", def.name.trim())?;

    match def.kind {
        UdtKind::Struct => {
            if def.members.is_empty() {
                return Err(DataTypeError::Invalid(format!(
                    "STRUCT '{}' has no members; a STRUCT with no fields cannot be bound to",
                    def.name
                )));
            }
            let mut seen = BTreeSet::new();
            for member in &def.members {
                if member.name.trim().is_empty() {
                    return Err(DataTypeError::Invalid(format!(
                        "STRUCT '{}' has a member with no name",
                        def.name
                    )));
                }
                if member.data_type.trim().is_empty() {
                    return Err(DataTypeError::Invalid(format!(
                        "STRUCT '{}' member '{}' has no data type",
                        def.name, member.name
                    )));
                }
                if !seen.insert(member.name.as_str()) {
                    return Err(DataTypeError::Invalid(format!(
                        "STRUCT '{}' declares member '{}' more than once",
                        def.name, member.name
                    )));
                }
            }
            if !def.enum_constants.is_empty() {
                return Err(DataTypeError::Invalid(format!(
                    "STRUCT '{}' must not declare enum constants",
                    def.name
                )));
            }
        }
        UdtKind::Enum => {
            if def.enum_constants.is_empty() {
                return Err(DataTypeError::Invalid(format!(
                    "ENUM '{}' has no constants",
                    def.name
                )));
            }
            let mut seen = BTreeSet::new();
            for constant in &def.enum_constants {
                if constant.trim().is_empty() {
                    return Err(DataTypeError::Invalid(format!(
                        "ENUM '{}' has an empty constant",
                        def.name
                    )));
                }
                if !seen.insert(constant.as_str()) {
                    return Err(DataTypeError::Invalid(format!(
                        "ENUM '{}' declares constant '{constant}' more than once",
                        def.name
                    )));
                }
            }
            if !def.members.is_empty() {
                return Err(DataTypeError::Invalid(format!(
                    "ENUM '{}' must not declare struct members",
                    def.name
                )));
            }
        }
        UdtKind::Alias => {
            let base = def.base_type.as_deref().unwrap_or("").trim();
            if base.is_empty() {
                return Err(DataTypeError::Invalid(format!(
                    "ALIAS '{}' must name its base type",
                    def.name
                )));
            }
            if !def.members.is_empty() || !def.enum_constants.is_empty() {
                return Err(DataTypeError::Invalid(format!(
                    "ALIAS '{}' must not declare members or constants",
                    def.name
                )));
            }
        }
    }
    Ok(())
}

/// Project-scoped data type registry rooted at a project directory.
///
/// Draft and active are separate files, the same staging model as screens and faceplate types: a
/// draft save is invisible to anything reading the released view.
#[derive(Debug, Clone)]
pub struct DataTypeStore {
    project_dir: std::path::PathBuf,
}

impl DataTypeStore {
    pub fn new(project_dir: impl Into<std::path::PathBuf>) -> Self {
        Self {
            project_dir: project_dir.into(),
        }
    }

    fn draft_path(&self) -> std::path::PathBuf {
        self.project_dir.join("datatypes/draft").join(TYPES_FILE)
    }

    fn active_path(&self) -> std::path::PathBuf {
        self.project_dir.join("datatypes/active").join(TYPES_FILE)
    }

    /// Draft-stage entries, as authored. Empty when the project has none — never seeded.
    pub fn draft_types(&self) -> Result<Vec<DataTypeEntry>, DataTypeError> {
        read_list(&self.draft_path())
    }

    /// Released versions, append-only, so an old version stays resolvable.
    pub fn active_versions(&self) -> Result<Vec<DataTypeEntry>, DataTypeError> {
        read_list(&self.active_path())
    }

    pub fn active_type(&self, id: &str) -> Result<Option<DataTypeEntry>, DataTypeError> {
        Ok(self
            .active_versions()?
            .into_iter()
            .filter(|t| t.id == id)
            .max_by_key(|t| t.version))
    }

    fn write_draft(&self, entries: &[DataTypeEntry]) -> Result<(), DataTypeError> {
        let bytes = serde_json::to_vec_pretty(entries)?;
        write_atomic(&self.draft_path(), &bytes)
    }

    /// Create a draft entry. The id is generated and the version assigned server-side.
    pub fn create_type(&self, definition: UdtDefinition) -> Result<DataTypeEntry, DataTypeError> {
        validate_definition(&definition)?;
        let id = format!("dut_{:032x}", rand::random::<u128>());
        let entry = DataTypeEntry {
            id: id.clone(),
            version: 0,
            definition,
        };

        // A project must not end up with two types sharing a name: the logic editor's pickers
        // address them by name, so a duplicate would be ambiguous.
        let mut drafts = self.draft_types()?;
        if let Some(existing) = drafts.iter().find(|t| {
            t.id != id
                && t.definition
                    .name
                    .eq_ignore_ascii_case(&entry.definition.name)
        }) {
            return Err(DataTypeError::Invalid(format!(
                "a data type named '{}' already exists in this project (id {})",
                existing.definition.name, existing.id
            )));
        }
        drafts.push(entry.clone());
        self.write_draft(&drafts)?;
        Ok(entry)
    }

    /// Replace the draft entry's definition.
    pub fn save_draft(
        &self,
        id: &str,
        definition: UdtDefinition,
    ) -> Result<DataTypeEntry, DataTypeError> {
        validate_definition(&definition)?;
        let mut drafts = self.draft_types()?;
        let index = drafts
            .iter()
            .position(|t| t.id == id)
            .ok_or_else(|| DataTypeError::NotFound(id.to_owned()))?;

        if let Some(other) = drafts
            .iter()
            .find(|t| t.id != id && t.definition.name.eq_ignore_ascii_case(&definition.name))
        {
            return Err(DataTypeError::Invalid(format!(
                "a data type named '{}' already exists in this project (id {})",
                other.definition.name, other.id
            )));
        }

        let next_version = match self.active_type(id)? {
            Some(active) => active.version + 1,
            None => 0,
        };
        drafts[index] = DataTypeEntry {
            id: id.to_owned(),
            version: next_version,
            definition,
        };
        let updated = drafts[index].clone();
        self.write_draft(&drafts)?;
        Ok(updated)
    }

    /// Promote the draft to a new released version.
    pub fn release(&self, id: &str) -> Result<DataTypeEntry, DataTypeError> {
        let drafts = self.draft_types()?;
        let draft = drafts
            .iter()
            .find(|t| t.id == id)
            .cloned()
            .ok_or_else(|| DataTypeError::NotFound(id.to_owned()))?;
        validate_definition(&draft.definition)?;

        let mut published = self.active_versions()?;
        let next_version = published
            .iter()
            .filter(|t| t.id == id)
            .max_by_key(|t| t.version)
            .map_or(1, |t| t.version + 1);
        let released = DataTypeEntry {
            id: id.to_owned(),
            version: next_version,
            definition: draft.definition,
        };
        published.retain(|t| !(t.id == id && t.version == next_version));
        published.push(released.clone());
        let bytes = serde_json::to_vec_pretty(&published)?;
        write_atomic(&self.active_path(), &bytes)?;
        Ok(released)
    }

    /// Remove a draft entry. Released versions are retained.
    pub fn delete_draft(&self, id: &str) -> Result<(), DataTypeError> {
        let mut drafts = self.draft_types()?;
        let before = drafts.len();
        drafts.retain(|t| t.id != id);
        if drafts.len() == before {
            return Err(DataTypeError::NotFound(id.to_owned()));
        }
        self.write_draft(&drafts)
    }

    /// Read one entry: `draft` returns the authored definition, anything else the latest released.
    pub fn read_type(&self, id: &str, view: &str) -> Result<DataTypeEntry, DataTypeError> {
        if view.eq_ignore_ascii_case("draft") {
            self.draft_types()?
                .into_iter()
                .find(|t| t.id == id)
                .ok_or_else(|| DataTypeError::NotFound(id.to_owned()))
        } else {
            self.active_type(id)?
                .ok_or_else(|| DataTypeError::NotFound(id.to_owned()))
        }
    }

    /// Summaries for the logic editor's picker, read from the draft stage.
    pub fn summaries(&self) -> Result<Vec<DataTypeSummary>, DataTypeError> {
        let drafts = self.draft_types()?;
        let active = self.active_versions()?;
        Ok(drafts
            .into_iter()
            .map(|d| {
                let active_version = active
                    .iter()
                    .filter(|t| t.id == d.id)
                    .max_by_key(|t| t.version)
                    .map(|t| t.version);
                DataTypeSummary {
                    field_count: match d.definition.kind {
                        UdtKind::Struct => d.definition.members.len() as u32,
                        UdtKind::Enum => d.definition.enum_constants.len() as u32,
                        UdtKind::Alias => 1,
                    },
                    id: d.id.clone(),
                    name: d.definition.name.clone(),
                    kind: d.definition.kind,
                    has_draft: active_version != Some(d.version),
                    active_version,
                }
            })
            .collect())
    }
}

/// Write through a temporary file and rename so a crash never leaves a half-written registry.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), DataTypeError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}
