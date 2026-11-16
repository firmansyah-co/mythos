//! Canonical Plant Hierarchy Tag Namespace parser, validator, and domain types.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Prohibited transport protocol identifiers that must never appear in tag namespaces.
pub const PROHIBITED_PROTOCOLS: &[&str] = &[
    "modbus",
    "modbus_tcp",
    "modbus_rtu",
    "modbustcp",
    "modbusrtu",
    "profinet",
    "profibus",
    "ethernet_ip",
    "ethernetip",
    "eip",
    "opcua",
    "opc_ua",
    "opc_da",
    "opc",
    "mqtt",
    "s7",
    "s7comm",
    "bacnet",
    "canopen",
    "ethercat",
    "cclink",
    "cc_link",
    "hart",
];

#[derive(Error, Debug, PartialEq, Eq)]
pub enum TagNamespaceError {
    #[error("Tag URI cannot be empty")]
    EmptyUri,

    #[error(
        "Unknown or invalid root domain '{domain}'. Must be 'external', 'internal', or 'kernel'"
    )]
    InvalidRootDomain { domain: String },

    #[error("Namespace contains prohibited protocol name '{protocol}'. Transport protocols must never appear in plant hierarchy tags.")]
    ProhibitedProtocolInNamespace { protocol: String },

    #[error("Insufficient URI segments for '{uri}'. External tags require 'external.<namespace>.<device>.<variable>[.<subfield>]'")]
    InsufficientSegments { uri: String },

    #[error("Token '{token}' exceeds maximum allowed length of {max_len} characters")]
    TokenTooLong { token: String, max_len: usize },

    #[error("Token '{token}' contains invalid characters. Tokens must match required alphanumeric format.")]
    InvalidTokenFormat { token: String },

    #[error("Variable name '{variable}' must be lowercase alphanumeric with underscores")]
    InvalidVariableCase { variable: String },
}

/// Root classification of a process variable.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TagDomain {
    /// Field devices and instruments (e.g., Supmea P200, Modicon M251, RTDs)
    External,
    /// Soft-logic variables, intermediate timers, accumulator registers
    Internal,
    /// Host OS metrics, microkernel diagnostics, bus cycle counters
    Kernel,
}

/// Parsed canonical tag uniform resource identifier (URI).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TagUri {
    pub raw: String,
    pub domain: TagDomain,
    pub plant_area: String,
    pub device_or_block: String,
    pub variable: String,
    pub subfield: Option<String>,
}

impl std::fmt::Display for TagUri {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.raw)
    }
}

impl std::str::FromStr for TagUri {
    type Err = TagNamespaceError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl TagUri {
    /// Parse and strictly validate a canonical tag URI.
    pub fn parse(uri: &str) -> Result<Self, TagNamespaceError> {
        let trimmed = uri.trim();
        if trimmed.is_empty() {
            return Err(TagNamespaceError::EmptyUri);
        }

        let parts: Vec<&str> = trimmed.split('.').collect();
        if parts.len() < 3 {
            return Err(TagNamespaceError::InsufficientSegments {
                uri: trimmed.to_string(),
            });
        }

        let domain_str = parts[0];
        let domain = match domain_str {
            "external" => TagDomain::External,
            "internal" => TagDomain::Internal,
            "kernel" => TagDomain::Kernel,
            other => {
                return Err(TagNamespaceError::InvalidRootDomain {
                    domain: other.to_string(),
                })
            }
        };

        match domain {
            TagDomain::External => {
                // Format: external.<namespace>.<device>.<variable>[.<subfield>...]
                // Minimum 4 segments: external, namespace, device, variable
                if parts.len() < 4 {
                    return Err(TagNamespaceError::InsufficientSegments {
                        uri: trimmed.to_string(),
                    });
                }

                // If namespace has hierarchical dots (e.g. external.PKS.Station2.PT101.pressure.pv),
                // the device is parts[len - 2] (or len - 3 if subfield), variable is parts[len - 1].
                // According to spec EBNF:
                // ExternalUri ::= "external." PlantArea "." DeviceId "." VariableId ( "." MemberId )* ;
                // In standard 4-token URI: parts[1] = PlantArea, parts[2] = Device, parts[3] = Variable
                let (plant_area, device, variable, subfield) =
                    Self::dissect_hierarchy(&parts[1..])?;

                // Check for prohibited protocol names in PlantArea
                let area_lower = plant_area.to_lowercase();
                for proto in PROHIBITED_PROTOCOLS {
                    for token in area_lower.split('.') {
                        if token == *proto {
                            return Err(TagNamespaceError::ProhibitedProtocolInNamespace {
                                protocol: proto.to_string(),
                            });
                        }
                    }
                }

                Ok(Self {
                    raw: trimmed.to_string(),
                    domain,
                    plant_area,
                    device_or_block: device,
                    variable,
                    subfield,
                })
            }
            TagDomain::Internal => {
                // Format: internal.<namespace>.<block>.<variable>[.<subfield>]
                if parts.len() < 4 {
                    return Err(TagNamespaceError::InsufficientSegments {
                        uri: trimmed.to_string(),
                    });
                }
                let (plant_area, block, variable, subfield) = Self::dissect_hierarchy(&parts[1..])?;

                Ok(Self {
                    raw: trimmed.to_string(),
                    domain,
                    plant_area,
                    device_or_block: block,
                    variable,
                    subfield,
                })
            }
            TagDomain::Kernel => {
                // Format: kernel.<subsystem>.<metric>
                let subsystem = parts[1];
                let metric = parts[2];
                Self::validate_token(subsystem, 32, false)?;
                Self::validate_token(metric, 48, true)?;

                let subfield = if parts.len() > 3 {
                    Some(parts[3..].join("."))
                } else {
                    None
                };

                Ok(Self {
                    raw: trimmed.to_string(),
                    domain,
                    plant_area: "kernel".to_string(),
                    device_or_block: subsystem.to_string(),
                    variable: metric.to_string(),
                    subfield,
                })
            }
        }
    }

    /// Dissects remaining tokens into (PlantArea, Device/Block, Variable, Subfield).
    /// Last segment may be subfield if variable is before it, or variable is last.
    fn dissect_hierarchy(
        tokens: &[&str],
    ) -> Result<(String, String, String, Option<String>), TagNamespaceError> {
        // tokens has at least 3 elements: e.g. [BoilerRoom, PT101, pressure] or [BoilerRoom, PT101, pressure, pv]
        // or [PKS, Station2, PT101, pressure, pv]
        // The last 1 or 2 tokens:
        // Variable token must be lowercase alphanumeric [a-z0-9_].
        // Device token is [A-Za-z0-9_].
        // If the last token is a known subfield or lowercase, check if preceding is variable.
        let n = tokens.len();
        if n == 3 {
            // [Area, Device, Variable]
            let area = tokens[0];
            let device = tokens[1];
            let variable = tokens[2];

            Self::validate_plant_area(area)?;
            Self::validate_token(device, 32, false)?;
            Self::validate_token(variable, 48, true)?;

            Ok((
                area.to_string(),
                device.to_string(),
                variable.to_string(),
                None,
            ))
        } else if n == 4 {
            // Either [Area, Device, Variable, Subfield] or [Area1, Area2, Device, Variable]
            // We differentiate by checking if tokens[3] is subfield and tokens[2] is variable
            let maybe_var = tokens[2];
            let is_var_valid = Self::validate_token(maybe_var, 48, true).is_ok();

            if is_var_valid {
                // [Area, Device, Variable, Subfield]
                let area = tokens[0];
                let device = tokens[1];
                let variable = tokens[2];
                let subfield = tokens[3];

                Self::validate_plant_area(area)?;
                Self::validate_token(device, 32, false)?;
                Self::validate_token(subfield, 32, true)?;

                Ok((
                    area.to_string(),
                    device.to_string(),
                    variable.to_string(),
                    Some(subfield.to_string()),
                ))
            } else {
                // [Area1, Area2, Device, Variable]
                let area = format!("{}.{}", tokens[0], tokens[1]);
                let device = tokens[2];
                let variable = tokens[3];

                Self::validate_plant_area(&area)?;
                Self::validate_token(device, 32, false)?;
                Self::validate_token(variable, 48, true)?;

                Ok((area, device.to_string(), variable.to_string(), None))
            }
        } else {
            // n >= 5: e.g. [PKS, Station2, PT101, pressure, pv]
            let area = tokens[..n - 3].join(".");
            let device = tokens[n - 3];
            let variable = tokens[n - 2];
            let subfield = tokens[n - 1];

            Self::validate_plant_area(&area)?;
            Self::validate_token(device, 32, false)?;
            Self::validate_token(variable, 48, true)?;
            Self::validate_token(subfield, 32, true)?;

            Ok((
                area,
                device.to_string(),
                variable.to_string(),
                Some(subfield.to_string()),
            ))
        }
    }

    fn validate_plant_area(area: &str) -> Result<(), TagNamespaceError> {
        for token in area.split('.') {
            Self::validate_token(token, 32, false)?;
        }
        Ok(())
    }

    fn validate_token(
        token: &str,
        max_len: usize,
        require_lowercase: bool,
    ) -> Result<(), TagNamespaceError> {
        if token.is_empty() {
            return Err(TagNamespaceError::InvalidTokenFormat {
                token: token.to_string(),
            });
        }
        if token.len() > max_len {
            return Err(TagNamespaceError::TokenTooLong {
                token: token.to_string(),
                max_len,
            });
        }
        if !token.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(TagNamespaceError::InvalidTokenFormat {
                token: token.to_string(),
            });
        }
        if require_lowercase && token.chars().any(|c| c.is_ascii_uppercase()) {
            return Err(TagNamespaceError::InvalidVariableCase {
                variable: token.to_string(),
            });
        }
        Ok(())
    }
}
