//! Tag metadata, retention classes, access permissions, and in-memory TagCatalog.

use crate::namespace::{TagDomain, TagNamespaceError, TagUri};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Memory retention classification for process variables.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RetentionMode {
    /// Cleared upon reboot or process restart.
    Volatile,
    /// Preserved across warm system restarts in shared memory and NVRAM.
    Retain,
    /// Preserved across cold power cycles and program re-flashes via NVRAM WAL.
    Persistent,
}

/// Access permission modes for tags.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AccessMode {
    /// Read-only (field sensor telemetry, unforced inputs)
    ReadOnly,
    /// Write-only (one-shot command pulses, remote commands)
    WriteOnly,
    /// Bidirectional read/write (setpoints, parameter adjustments)
    ReadWrite,
}

/// Rich metadata attached to every registered process variable.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TagMetadata {
    /// Canonical dot-separated URI (e.g. "external.BoilerRoom.PT101.pressure.pv")
    pub uri: String,

    /// Unique numeric index into Shared Memory TagBus array
    pub slot_id: u32,

    /// Human-readable description
    pub description: String,

    /// Engineering units (e.g. "bar", "°C", "RPM", "m³/h", "%")
    pub engineering_units: String,

    /// Lower physical limit of sensor scale
    pub scale_min: f64,

    /// Upper physical limit of sensor scale
    pub scale_max: f64,

    /// Raw register integer lower bound (e.g. 0)
    pub raw_min: i64,

    /// Raw register integer upper bound (e.g. 65535)
    pub raw_max: i64,

    /// Deadband filter percentage (0.0 to 100.0%)
    pub deadband_percent: f64,

    /// Historical retention policy
    pub retention_mode: RetentionMode,

    /// Access Mode permissions
    pub access_mode: AccessMode,
}

impl TagMetadata {
    pub fn new(uri: impl Into<String>, slot_id: u32) -> Result<Self, TagNamespaceError> {
        let uri_str = uri.into();
        // Validate URI syntax
        let _ = TagUri::parse(&uri_str)?;
        Ok(Self {
            uri: uri_str,
            slot_id,
            description: String::new(),
            engineering_units: String::new(),
            scale_min: 0.0,
            scale_max: 100.0,
            raw_min: 0,
            raw_max: 65535,
            deadband_percent: 0.0,
            retention_mode: RetentionMode::Volatile,
            access_mode: AccessMode::ReadWrite,
        })
    }

    /// Builder helper to set description.
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    /// Builder helper to set engineering units.
    pub fn with_units(mut self, units: impl Into<String>) -> Self {
        self.engineering_units = units.into();
        self
    }

    /// Builder helper to set retention mode.
    pub fn with_retention(mut self, mode: RetentionMode) -> Self {
        self.retention_mode = mode;
        self
    }

    /// Builder helper to set access mode.
    pub fn with_access(mut self, mode: AccessMode) -> Self {
        self.access_mode = mode;
        self
    }
}

/// In-memory tag catalog and namespace registry.
#[derive(Clone, Debug, Default)]
pub struct TagCatalog {
    by_uri: HashMap<String, TagMetadata>,
    by_slot: HashMap<u32, String>,
}

impl TagCatalog {
    pub fn new() -> Self {
        Self {
            by_uri: HashMap::new(),
            by_slot: HashMap::new(),
        }
    }

    /// Register a tag metadata entry. Returns error if URI is invalid or slot is already claimed.
    pub fn register(&mut self, metadata: TagMetadata) -> Result<(), &'static str> {
        let uri = metadata.uri.clone();
        let slot = metadata.slot_id;

        if self.by_uri.contains_key(&uri) {
            return Err("Tag URI already registered in catalog");
        }
        if self.by_slot.contains_key(&slot) {
            return Err("Shared memory slot ID already registered in catalog");
        }

        self.by_slot.insert(slot, uri.clone());
        self.by_uri.insert(uri, metadata);
        Ok(())
    }

    /// Lookup metadata by canonical URI.
    pub fn get_by_uri(&self, uri: &str) -> Option<&TagMetadata> {
        self.by_uri.get(uri)
    }

    /// Lookup metadata by slot ID.
    pub fn get_by_slot(&self, slot_id: u32) -> Option<&TagMetadata> {
        self.by_slot.get(&slot_id).and_then(|u| self.by_uri.get(u))
    }

    /// Total count of registered tags.
    pub fn len(&self) -> usize {
        self.by_uri.len()
    }

    /// Returns true if catalog is empty.
    pub fn is_empty(&self) -> bool {
        self.by_uri.is_empty()
    }

    /// Iterate over all registered tags.
    pub fn iter(&self) -> impl Iterator<Item = &TagMetadata> {
        self.by_uri.values()
    }

    /// Filter tags by domain (external, internal, kernel).
    pub fn find_by_domain(&self, domain: TagDomain) -> Vec<&TagMetadata> {
        self.by_uri
            .values()
            .filter(|meta| {
                if let Ok(parsed) = TagUri::parse(&meta.uri) {
                    parsed.domain == domain
                } else {
                    false
                }
            })
            .collect()
    }
}
