//! The shape of an IEC 61131-3 user-defined type, as a faceplate type carries it.
//!
//! A faceplate type holds its UDT structurally (see `crate::faceplate`), so the HMI needs only the
//! shape, not a data-type registry. It is the same JSON as the core's `datatype::UdtDefinition`;
//! when data types move into the logic module (ADR-011 §3 step 5) both read the one wire form.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Kind of IEC 61131-3 user-defined type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum UdtKind {
    Struct,
    Enum,
    Alias,
}

/// One named field of a STRUCT UDT.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UdtMember {
    pub name: String,
    pub data_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The wire form must stay the core's: a faceplate type written before the move reads back.
    #[test]
    fn reads_the_json_the_core_wrote() {
        let udt: UdtDefinition = serde_json::from_str(
            r#"{"name":"Motor","kind":"STRUCT","members":[{"name":"Speed","dataType":"REAL"}],"enumConstants":[]}"#,
        )
        .unwrap();
        assert_eq!(udt.kind, UdtKind::Struct);
        assert_eq!(udt.members[0].data_type, "REAL");
        assert_eq!(
            serde_json::to_value(&udt).unwrap()["members"][0]["dataType"],
            "REAL"
        );
    }
}
