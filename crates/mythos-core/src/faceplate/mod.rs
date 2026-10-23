//! Master faceplate type model, persistence, and release-time propagation (TSK-309).
//!
//! A **faceplate type** is the master definition: an identity, the IEC 61131-3 UDT it
//! formally binds to, and one or more views. A **faceplate instance** is a placed copy on
//! a plant screen that references its master by `type_id` and carries only its own tag
//! bindings.
//!
//! Propagation is by reference at read time, not by rewriting instance records. Nothing
//! from the master is copied into an instance at creation, so releasing a new version
//! changes what every following instance renders with no per-instance edit. An instance
//! that a commissioning engineer explicitly froze (`type_version` set) keeps rendering the
//! version it was frozen to; a release never fails because of one.
//!
//! There is no project-scoped data-type registry in the backend yet, so the UDT is carried
//! **structurally** on the type and every view binding is validated against its members.
//! A shared backend DUT/UDT registry is a separate follow-up; see the PR body for TSK-309.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::Path;
use thiserror::Error;
use ts_rs::TS;

/// Kind of IEC 61131-3 user-defined type a faceplate binds to.
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

/// Serialisable UDT definition carried by a faceplate type.
///
/// This is deliberately structural rather than a reference to a registry: the backend has
/// no shared data-type store yet, and a dangling name string would claim a validation that
/// does not exist.
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
    /// Names a view binding may legally target for this UDT kind.
    fn bindable_names(&self) -> BTreeSet<&str> {
        match self.kind {
            // ENUM values are the only addressable name; ALIAS members are carried by the
            // aliased base type and are not describable here, so bindings must not target one.
            UdtKind::Struct => self.members.iter().map(|m| m.name.as_str()).collect(),
            UdtKind::Enum => self.enum_constants.iter().map(String::as_str).collect(),
            UdtKind::Alias => BTreeSet::new(),
        }
    }
}

/// Direction capability of a faceplate view binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BindingAccess {
    ReadOnly,
    ReadWrite,
}

/// One property of a master view bound to a UDT member.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct FaceplateBinding {
    /// Property key inside the faceplate view, e.g. `running` (see `autoBind.ts`).
    pub faceplate_key: String,
    /// Member of the bound UDT this key presents, e.g. `bRunning`.
    pub udt_property: String,
    pub access: BindingAccess,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
}

/// Presentation archetype of a master view (ISA-101 3-tier faceplate model plus the alarm
/// archetype).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FaceplateViewKind {
    PidSymbol,
    SummaryCard,
    DiagnosticPopup,
    Alarm,
}

/// A master view: its archetype and the UDT-bound properties it presents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct FaceplateView {
    pub kind: FaceplateViewKind,
    #[serde(default)]
    pub bindings: Vec<FaceplateBinding>,
}

/// A published master faceplate definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct FaceplateType {
    pub id: String,
    pub name: String,
    /// `0` while the type has never been released; assigned server-side on draft writes.
    pub version: u32,
    pub udt: UdtDefinition,
    #[serde(default)]
    pub views: Vec<FaceplateView>,
}

/// Tree summary of a type, always read from the draft stage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct FaceplateTypeSummary {
    pub id: String,
    pub name: String,
    /// Highest released version, or `Option::None` when the type is draft-only.
    pub active_version: Option<u32>,
    pub has_draft: bool,
    pub udt_name: String,
    pub instance_count: u32,
}

/// Per-instance tag binding. The view shape itself always comes from the master.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct InstanceBinding {
    pub faceplate_key: String,
    /// Absolute tag URI (`external.` / `internal.` / `kernel.`), or empty when unassigned.
    pub tag_uri: String,
}

/// A placed faceplate on a plant screen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct FaceplateInstance {
    pub id: String,
    pub name: String,
    pub type_id: String,
    /// `None` follows the active master; `Some(n)` freezes the instance to version `n`.
    ///
    /// Always serialized (as `null` for "follow active") so the generated TypeScript type's
    /// `number | null` is accurate rather than an absent field reading as `undefined`.
    pub type_version: Option<u32>,
    #[serde(default)]
    pub bindings: Vec<InstanceBinding>,
}

/// A frozen instance left behind by a release.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct StrandedInstance {
    pub id: String,
    pub name: String,
    pub type_version: u32,
}

/// Result of a successful release.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct FaceplateReleaseResult {
    pub id: String,
    pub version: u32,
    /// Instances that moved to the released version (frozen instances are excluded).
    pub instances_affected: u32,
    /// Frozen instances deliberately left on their own version.
    pub stranded_instances: Vec<StrandedInstance>,
}

/// An instance resolved against its master at read time.
///
/// `udt` and `views` are `None` only when the master definition cannot be resolved; the
/// reason is always stated in `warnings`, never substituted with a placeholder.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedInstance {
    pub id: String,
    pub name: String,
    pub type_id: String,
    pub type_version: Option<u32>,
    /// Version actually used, or `None` when the master could not be resolved.
    pub resolved_version: Option<u32>,
    pub frozen: bool,
    pub udt: Option<UdtDefinition>,
    pub views: Option<Vec<FaceplateView>>,
    pub bindings: Vec<InstanceBinding>,
    pub warnings: Vec<String>,
}

/// Persistence or validation failure inside the faceplate store.
#[derive(Debug, Error)]
pub enum FaceplateError {
    #[error("faceplate type '{0}' not found")]
    TypeNotFound(String),

    #[error("faceplate instance '{0}' not found")]
    InstanceNotFound(String),

    #[error("invalid faceplate definition: {0}")]
    Invalid(String),

    #[error("project '{0}' does not exist")]
    ProjectNotFound(String),

    #[error("I/O error during faceplate storage: {0}")]
    Io(#[from] std::io::Error),

    #[error("faceplate JSON error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("storage error: {0}")]
    Storage(#[from] crate::storage::StorageError),
}

const TYPES_FILE: &str = "types.json";
const INSTANCES_FILE: &str = "instances.json";

fn read_list<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<Vec<T>, FaceplateError> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(FaceplateError::Io(e)),
    }
}

/// Reject ids and names that could escape the project directory or corrupt the store.
fn validate_token(kind: &str, value: &str) -> Result<(), FaceplateError> {
    if value.is_empty() {
        return Err(FaceplateError::Invalid(format!("{kind} is required")));
    }
    if value.len() > 128 {
        return Err(FaceplateError::Invalid(format!(
            "{kind} exceeds 128 characters"
        )));
    }
    if !value
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b'.')
    {
        return Err(FaceplateError::Invalid(format!(
            "{kind} may only contain ASCII alphanumerics, '_', '-' and '.'"
        )));
    }
    Ok(())
}

/// Validate a master definition before it is persisted.
///
/// Every view binding must name a member of the bound UDT, so "formal UDT binding" is
/// checked against real data rather than accepted as a free string.
pub fn validate_type(def: &FaceplateType) -> Result<(), FaceplateError> {
    validate_token("faceplate type id", &def.id)?;
    if def.name.trim().is_empty() {
        return Err(FaceplateError::Invalid(
            "faceplate type name is required".into(),
        ));
    }
    if def.udt.name.trim().is_empty() {
        return Err(FaceplateError::Invalid(
            "faceplate type must declare the UDT it binds to".into(),
        ));
    }
    if def.views.is_empty() {
        return Err(FaceplateError::Invalid(
            "faceplate type must define at least one view".into(),
        ));
    }

    let bindable = def.udt.bindable_names();
    for view in &def.views {
        let mut seen_keys = BTreeSet::new();
        for binding in &view.bindings {
            if binding.faceplate_key.trim().is_empty() {
                return Err(FaceplateError::Invalid(
                    "view binding is missing faceplate_key".into(),
                ));
            }
            if !seen_keys.insert(binding.faceplate_key.as_str()) {
                return Err(FaceplateError::Invalid(format!(
                    "view {:?} declares faceplate_key '{}' more than once",
                    view.kind, binding.faceplate_key
                )));
            }
            if binding.udt_property.trim().is_empty() {
                return Err(FaceplateError::Invalid(format!(
                    "view binding '{}' is missing udt_property",
                    binding.faceplate_key
                )));
            }
            if !bindable.contains(binding.udt_property.as_str()) {
                let available: Vec<&str> = bindable.iter().copied().collect();
                return Err(FaceplateError::Invalid(format!(
                    "view binding '{}' references udt_property '{}' which is not a member of UDT '{}' ({}); declared names: {:?}",
                    binding.faceplate_key,
                    binding.udt_property,
                    def.udt.name,
                    match def.udt.kind {
                        UdtKind::Struct => "STRUCT",
                        UdtKind::Enum => "ENUM",
                        UdtKind::Alias => "ALIAS has no addressable members",
                    },
                    available
                )));
            }
        }
    }
    Ok(())
}

/// Every `faceplate_key` declared across a master's views.
fn declared_keys(def: &FaceplateType) -> BTreeSet<&str> {
    def.views
        .iter()
        .flat_map(|v| v.bindings.iter().map(|b| b.faceplate_key.as_str()))
        .collect()
}

/// Project-scoped faceplate store rooted at a project directory.
#[derive(Debug, Clone)]
pub struct FaceplateStore {
    project_dir: std::path::PathBuf,
}

impl FaceplateStore {
    pub fn new(project_dir: impl Into<std::path::PathBuf>) -> Self {
        Self {
            project_dir: project_dir.into(),
        }
    }

    fn draft_path(&self) -> std::path::PathBuf {
        self.project_dir.join("faceplates/draft").join(TYPES_FILE)
    }

    fn active_path(&self) -> std::path::PathBuf {
        self.project_dir.join("faceplates/active").join(TYPES_FILE)
    }

    fn instances_path(&self) -> std::path::PathBuf {
        self.project_dir.join("faceplates").join(INSTANCES_FILE)
    }

    /// Draft-stage types, as authored. Empty when the project has none.
    pub fn draft_types(&self) -> Result<Vec<FaceplateType>, FaceplateError> {
        read_list(&self.draft_path())
    }

    /// Published versions, append-only: every version ever released is retained so frozen
    /// instances stay resolvable.
    pub fn active_versions(&self) -> Result<Vec<FaceplateType>, FaceplateError> {
        read_list(&self.active_path())
    }

    /// Instances placed on plant screens. Instances are plant configuration, not staged.
    pub fn instances(&self) -> Result<Vec<FaceplateInstance>, FaceplateError> {
        read_list(&self.instances_path())
    }

    /// Highest released version of a type, if it has ever been released.
    pub fn active_type(&self, id: &str) -> Result<Option<FaceplateType>, FaceplateError> {
        Ok(self
            .active_versions()?
            .into_iter()
            .filter(|t| t.id == id)
            .max_by_key(|t| t.version))
    }

    /// A specific published version, for frozen instances.
    pub fn active_version(
        &self,
        id: &str,
        version: u32,
    ) -> Result<Option<FaceplateType>, FaceplateError> {
        Ok(self
            .active_versions()?
            .into_iter()
            .find(|t| t.id == id && t.version == version))
    }

    fn write_draft(&self, types: &[FaceplateType]) -> Result<(), FaceplateError> {
        let bytes = serde_json::to_vec_pretty(types)?;
        write_atomic(&self.draft_path(), &bytes)
    }

    fn write_active(&self, types: &[FaceplateType]) -> Result<(), FaceplateError> {
        let bytes = serde_json::to_vec_pretty(types)?;
        write_atomic(&self.active_path(), &bytes)
    }

    fn write_instances(&self, instances: &[FaceplateInstance]) -> Result<(), FaceplateError> {
        let bytes = serde_json::to_vec_pretty(instances)?;
        write_atomic(&self.instances_path(), &bytes)
    }

    /// Create or update a draft definition.
    ///
    /// The version is assigned server-side: `active_version + 1`, or `0` for a type that has
    /// never been released. A caller-supplied version is not trusted.
    pub fn save_draft(&self, mut def: FaceplateType) -> Result<FaceplateType, FaceplateError> {
        let mut drafts = self.draft_types()?;
        let next_version = match self.active_type(&def.id)? {
            Some(active) => active.version + 1,
            None => 0,
        };
        def.version = next_version;
        validate_type(&def)?;

        match drafts.iter_mut().find(|t| t.id == def.id) {
            Some(existing) => *existing = def.clone(),
            None => drafts.push(def.clone()),
        }
        self.write_draft(&drafts)?;
        Ok(def)
    }

    /// Create a draft-only type. The id is generated; the type is not visible to active
    /// readers until it is released.
    pub fn create_type(
        &self,
        name: String,
        udt: UdtDefinition,
    ) -> Result<FaceplateType, FaceplateError> {
        if name.trim().is_empty() {
            return Err(FaceplateError::Invalid(
                "faceplate type name is required".into(),
            ));
        }
        let id = format!("fp_{:032x}", rand::random::<u128>());
        let def = FaceplateType {
            id,
            name: name.trim().to_owned(),
            version: 0,
            udt,
            // A newly created type starts with the three ISA-101 view tiers so the
            // designer has a real skeleton; bindings are added by the author.
            views: vec![
                FaceplateView {
                    kind: FaceplateViewKind::PidSymbol,
                    bindings: Vec::new(),
                },
                FaceplateView {
                    kind: FaceplateViewKind::SummaryCard,
                    bindings: Vec::new(),
                },
                FaceplateView {
                    kind: FaceplateViewKind::DiagnosticPopup,
                    bindings: Vec::new(),
                },
            ],
        };
        self.save_draft(def)
    }

    /// Promote the draft of `id` to a new published version.
    ///
    /// Frozen instances are never touched and never block the release; they are reported as
    /// stranded. A type with no instances releases successfully as a no-op.
    pub fn release(&self, id: &str) -> Result<FaceplateReleaseResult, FaceplateError> {
        let drafts = self.draft_types()?;
        let mut draft = drafts
            .into_iter()
            .find(|t| t.id == id)
            .ok_or_else(|| FaceplateError::TypeNotFound(id.to_owned()))?;
        validate_type(&draft)?;

        let mut published = self.active_versions()?;
        let next_version = published
            .iter()
            .filter(|t| t.id == id)
            .max_by_key(|t| t.version)
            .map_or(1, |t| t.version + 1);
        draft.version = next_version;

        // Append-only: prior versions remain readable for frozen instances.
        published.retain(|t| !(t.id == id && t.version == next_version));
        published.push(draft.clone());
        self.write_active(&published)?;

        let instances = self.instances()?;
        let mut instances_affected = 0u32;
        let mut stranded = Vec::new();
        for inst in instances.iter().filter(|i| i.type_id == id) {
            match inst.type_version {
                None => instances_affected += 1,
                Some(frozen_at) if frozen_at < next_version => stranded.push(StrandedInstance {
                    id: inst.id.clone(),
                    name: inst.name.clone(),
                    type_version: frozen_at,
                }),
                Some(_) => {}
            }
        }

        Ok(FaceplateReleaseResult {
            id: id.to_owned(),
            version: next_version,
            instances_affected,
            stranded_instances: stranded,
        })
    }

    /// Tree summaries read from the draft stage.
    pub fn type_summaries(&self) -> Result<Vec<FaceplateTypeSummary>, FaceplateError> {
        let drafts = self.draft_types()?;
        let active = self.active_versions()?;
        let instances = self.instances()?;
        Ok(drafts
            .into_iter()
            .map(|d| {
                let active_version = active
                    .iter()
                    .filter(|t| t.id == d.id)
                    .max_by_key(|t| t.version)
                    .map(|t| t.version);
                FaceplateTypeSummary {
                    instance_count: instances.iter().filter(|i| i.type_id == d.id).count() as u32,
                    udt_name: d.udt.name.clone(),
                    id: d.id.clone(),
                    name: d.name.clone(),
                    active_version,
                    has_draft: active_version != Some(d.version),
                }
            })
            .collect())
    }

    /// Read a type for the designer: `view == "draft"` returns the authored definition,
    /// anything else returns the highest released version.
    pub fn read_type(&self, id: &str, view: &str) -> Result<FaceplateType, FaceplateError> {
        if view.eq_ignore_ascii_case("draft") {
            self.draft_types()?
                .into_iter()
                .find(|t| t.id == id)
                .ok_or_else(|| FaceplateError::TypeNotFound(id.to_owned()))
        } else {
            self.active_type(id)?
                .ok_or_else(|| FaceplateError::TypeNotFound(id.to_owned()))
        }
    }

    /// Place a new instance. The master is validated for existence; nothing is copied from
    /// it, so a later release changes what this instance renders.
    pub fn create_instance(
        &self,
        name: String,
        type_id: String,
        type_version: Option<u32>,
        bindings: Vec<InstanceBinding>,
    ) -> Result<FaceplateInstance, FaceplateError> {
        if name.trim().is_empty() {
            return Err(FaceplateError::Invalid("instance name is required".into()));
        }
        let master = self.read_type(&type_id, "draft")?;
        if let Some(v) = type_version {
            self.active_version(&type_id, v)?.ok_or_else(|| {
                FaceplateError::Invalid(format!(
                    "instance cannot freeze to version {v} of type '{type_id}': that version is not published"
                ))
            })?;
        }
        validate_instance_bindings(&master, &bindings)?;

        let mut instances = self.instances()?;
        let instance = FaceplateInstance {
            id: format!("inst_{:032x}", rand::random::<u128>()),
            name: name.trim().to_owned(),
            type_id,
            type_version,
            bindings,
        };
        instances.push(instance.clone());
        self.write_instances(&instances)?;
        Ok(instance)
    }

    /// Update an instance's tag bindings and/or freeze state.
    pub fn update_instance(
        &self,
        id: &str,
        type_version: Option<Option<u32>>,
        bindings: Option<Vec<InstanceBinding>>,
    ) -> Result<FaceplateInstance, FaceplateError> {
        let mut instances = self.instances()?;
        let index = instances
            .iter()
            .position(|i| i.id == id)
            .ok_or_else(|| FaceplateError::InstanceNotFound(id.to_owned()))?;
        let type_id = instances[index].type_id.clone();
        let master = self.read_type(&type_id, "draft")?;

        if let Some(freeze) = type_version {
            if let Some(v) = freeze {
                self.active_version(&type_id, v)?.ok_or_else(|| {
                    FaceplateError::Invalid(format!(
                        "cannot freeze instance to version {v} of type '{type_id}': that version is not published"
                    ))
                })?;
            }
            instances[index].type_version = freeze;
        }
        if let Some(bindings) = bindings {
            validate_instance_bindings(&master, &bindings)?;
            instances[index].bindings = bindings;
        }
        let updated = instances[index].clone();
        self.write_instances(&instances)?;
        Ok(updated)
    }

    /// Resolve every instance against its master at read time.
    ///
    /// This is the propagation path: instances carry no copy of the master, so a released
    /// version takes effect here immediately.
    pub fn resolve_instances(&self) -> Result<Vec<ResolvedInstance>, FaceplateError> {
        let instances = self.instances()?;
        let active = self.active_versions()?;
        let drafts = self.draft_types()?;

        Ok(instances
            .into_iter()
            .map(|inst| {
                let mut warnings = Vec::new();
                let active_for_type = active
                    .iter()
                    .filter(|t| t.id == inst.type_id)
                    .max_by_key(|t| t.version);
                let draft_for_type = drafts.iter().find(|t| t.id == inst.type_id);

                let (master, resolved_version): (Option<&FaceplateType>, Option<u32>) =
                    match inst.type_version {
                        Some(frozen_at) => {
                            let pinned = active
                                .iter()
                                .find(|t| t.id == inst.type_id && t.version == frozen_at);
                            if let Some(active_latest) = active_for_type {
                                if active_latest.version > frozen_at {
                                    warnings.push(format!(
                                        "frozen at v{frozen_at}; active is v{}",
                                        active_latest.version
                                    ));
                                }
                            }
                            match pinned {
                                Some(t) => (Some(t), Some(t.version)),
                                None => {
                                    warnings.push(format!(
                                        "master definition for type '{}' v{frozen_at} is missing; master data unavailable",
                                        inst.type_id
                                    ));
                                    (None, None)
                                }
                            }
                        }
                        None => match active_for_type {
                            Some(t) => (Some(t), Some(t.version)),
                            None => match draft_for_type {
                                Some(t) => {
                                    warnings.push(format!(
                                        "type '{}' is not released; rendering draft. Release it to make this instance production-safe",
                                        inst.type_id
                                    ));
                                    (Some(t), Some(t.version))
                                }
                                None => {
                                    warnings.push(format!(
                                        "faceplate type '{}' does not exist; master data unavailable",
                                        inst.type_id
                                    ));
                                    (None, None)
                                }
                            },
                        },
                    };

                if let Some(master) = master {
                    let declared = declared_keys(master);
                    for binding in &inst.bindings {
                        if !declared.contains(binding.faceplate_key.as_str()) {
                            warnings.push(format!(
                                "binding '{}' is not declared by master v{}",
                                binding.faceplate_key,
                                master.version
                            ));
                        }
                    }
                }

                ResolvedInstance {
                    id: inst.id,
                    name: inst.name,
                    type_id: inst.type_id,
                    type_version: inst.type_version,
                    resolved_version,
                    frozen: inst.type_version.is_some(),
                    udt: master.map(|m| m.udt.clone()),
                    views: master.map(|m| m.views.clone()),
                    bindings: inst.bindings,
                    warnings,
                }
            })
            .collect())
    }
}

/// An instance binding must target a key its master declares, and a valid absolute tag URI.
fn validate_instance_bindings(
    master: &FaceplateType,
    bindings: &[InstanceBinding],
) -> Result<(), FaceplateError> {
    let declared = declared_keys(master);
    let mut seen = BTreeSet::new();
    for binding in bindings {
        if !seen.insert(binding.faceplate_key.as_str()) {
            return Err(FaceplateError::Invalid(format!(
                "instance binds faceplate_key '{}' more than once",
                binding.faceplate_key
            )));
        }
        if !declared.contains(binding.faceplate_key.as_str()) {
            return Err(FaceplateError::Invalid(format!(
                "instance binding '{}' is not declared by master type '{}'",
                binding.faceplate_key, master.id
            )));
        }
        let uri = binding.tag_uri.trim();
        if !uri.is_empty() {
            crate::namespace::TagUri::parse(uri).map_err(|e| {
                FaceplateError::Invalid(format!(
                    "instance binding '{}' has invalid tagUri '{uri}': {e}",
                    binding.faceplate_key
                ))
            })?;
        }
    }
    Ok(())
}

/// Write through a temporary file and rename, so a crash never leaves a half-written store.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), FaceplateError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// SSE payload emitted when a type is released.
pub fn release_event_payload(result: &FaceplateReleaseResult, at_ms: u64) -> Value {
    json!({
        "typeId": result.id,
        "version": result.version,
        "instancesAffected": result.instances_affected,
        "atMs": at_ms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn motor_udt() -> UdtDefinition {
        UdtDefinition {
            name: "FB_Motor_VFD".into(),
            kind: UdtKind::Struct,
            members: vec![
                UdtMember {
                    name: "bRunning".into(),
                    data_type: "BOOL".into(),
                },
                UdtMember {
                    name: "rSpeedFeedback".into(),
                    data_type: "REAL".into(),
                },
            ],
            enum_constants: Vec::new(),
            base_type: None,
        }
    }

    fn motor_type(id: &str) -> FaceplateType {
        FaceplateType {
            id: id.into(),
            name: "Motor".into(),
            version: 0,
            udt: motor_udt(),
            views: vec![FaceplateView {
                kind: FaceplateViewKind::PidSymbol,
                bindings: vec![FaceplateBinding {
                    faceplate_key: "running".into(),
                    udt_property: "bRunning".into(),
                    access: BindingAccess::ReadOnly,
                    unit: None,
                }],
            }],
        }
    }

    #[test]
    fn binding_to_an_undeclared_udt_member_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let store = FaceplateStore::new(dir.path());
        let mut def = motor_type("fp_motor");
        def.views[0].bindings[0].udt_property = "bNotAMember".into();
        let err = store.save_draft(def).unwrap_err().to_string();
        assert!(
            err.contains("bNotAMember"),
            "error must name the offender: {err}"
        );
    }

    #[test]
    fn enum_udt_binds_against_its_constants() {
        let dir = tempfile::tempdir().unwrap();
        let store = FaceplateStore::new(dir.path());
        let mut def = motor_type("fp_mode");
        def.udt = UdtDefinition {
            name: "DUT_MachineState".into(),
            kind: UdtKind::Enum,
            members: Vec::new(),
            enum_constants: vec!["IDLE".into(), "RUNNING".into()],
            base_type: None,
        };
        def.views[0].bindings[0].udt_property = "RUNNING".into();
        assert!(store.save_draft(def).is_ok());
    }

    #[test]
    fn alias_udt_has_no_bindable_members() {
        let dir = tempfile::tempdir().unwrap();
        let store = FaceplateStore::new(dir.path());
        let mut def = motor_type("fp_alias");
        def.udt = UdtDefinition {
            name: "DUT_Speed".into(),
            kind: UdtKind::Alias,
            members: Vec::new(),
            enum_constants: Vec::new(),
            base_type: Some("REAL".into()),
        };
        assert!(store.save_draft(def).is_err());
    }

    #[test]
    fn duplicate_faceplate_key_within_a_view_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let store = FaceplateStore::new(dir.path());
        let mut def = motor_type("fp_motor");
        let duplicate = def.views[0].bindings[0].clone();
        def.views[0].bindings.push(duplicate);
        assert!(store.save_draft(def).is_err());
    }
}
