//! The Modbus driver as a MythOS module: its manifest, and the entry point of its executable.

use crate::ModbusDriver;
use mythos_module::{ModuleKind, ModuleManifest, PROTOCOL_VERSION};
use serde_json::{json, Value};
use std::process::ExitCode;

/// Module identifier.
pub const MODULE_ID: &str = "org.mythos.driver.modbus";

/// The driver name a connection uses: `"driver": "modbus"`.
pub const DRIVER_NAME: &str = "modbus";

/// JSON Schema of a connection's `config`.
///
/// It describes the form. [`crate::config::ModbusDriverConfig::validate`] stays the authority
/// on what is valid; the tests keep the two from drifting apart.
pub fn config_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "Modbus TCP connection",
        "type": "object",
        "required": ["transport", "host", "mappings"],
        "properties": {
            "transport": {
                "type": "string",
                "title": "Transport",
                "description": "Only Modbus TCP is implemented.",
                "enum": ["tcp"],
                "default": "tcp"
            },
            "host": {
                "type": "string",
                "title": "Host",
                "description": "IP address or host name of the device or gateway.",
                "minLength": 1
            },
            "port": {
                "type": "integer",
                "title": "Port",
                "minimum": 1,
                "maximum": 65535,
                "default": 502
            },
            "slave_id": {
                "type": "integer",
                "title": "Unit ID",
                "minimum": 1,
                "maximum": 247,
                "default": 1
            },
            "timeout_ms": {
                "type": "integer",
                "title": "Request timeout (ms)",
                "minimum": 10,
                "maximum": 10000,
                "default": 1000
            },
            "mappings": {
                "type": "array",
                "title": "Tags",
                "items": {
                    "type": "object",
                    "required": ["uri", "address", "function_code"],
                    "properties": {
                        "uri": {
                            "type": "string",
                            "title": "Tag",
                            "description": "external.<area>.<device>.<signal>",
                            "pattern": "^external\\.[^.]+\\.[^.]+\\..+$",
                            "x-mythos-role": "tag-uri"
                        },
                        "address": {
                            "type": "integer",
                            "title": "Address",
                            "minimum": 0,
                            "maximum": 65535
                        },
                        "function_code": {
                            "type": "integer",
                            "title": "Register",
                            "enum": [3, 4],
                            "x-enum-labels": ["Holding (3)", "Input (4)"],
                            "default": 3
                        },
                        "data_type": {
                            "type": "string",
                            "title": "Type",
                            "enum": ["u16", "i16", "u32", "i32", "f32"],
                            "default": "u16"
                        },
                        "word_order": {
                            "type": "string",
                            "title": "Word order",
                            "description": "Which register of a two-register value holds the high word.",
                            "enum": ["high_first", "low_first"],
                            "default": "high_first"
                        },
                        "scale": {
                            "type": "number",
                            "title": "Scale",
                            "description": "Multiplier. Any value other than 1 makes the tag a 64-bit float.",
                            "default": 1
                        }
                    }
                }
            }
        }
    })
}

pub fn manifest() -> ModuleManifest {
    ModuleManifest {
        protocol: PROTOCOL_VERSION,
        id: MODULE_ID.to_string(),
        name: "Modbus TCP".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        kind: ModuleKind::Driver,
        provides: DRIVER_NAME.to_string(),
        description: "Reads holding and input registers from a Modbus TCP device.".to_string(),
        capabilities: vec![crate::CAPABILITY_NETWORK_CLIENT.to_string()],
        config_schema: config_schema(),
    }
}

/// Entry point of the `mythos-module-modbus` executable.
pub fn main() -> ExitCode {
    mythos_module::run_driver_module(manifest(), ModbusDriver::new)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ModbusDriverConfig;

    fn enum_of(schema: &Value, pointer: &str) -> Vec<Value> {
        schema
            .pointer(pointer)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_else(|| panic!("no enum at {pointer}"))
    }

    fn config_with(mapping: Value) -> Value {
        json!({ "transport": "tcp", "host": "192.0.2.1", "mappings": [mapping] })
    }

    #[test]
    fn every_value_the_schema_offers_is_accepted_by_the_driver() {
        let schema = config_schema();
        let item = "/properties/mappings/items/properties";
        for data_type in enum_of(&schema, &format!("{item}/data_type/enum")) {
            for word_order in enum_of(&schema, &format!("{item}/word_order/enum")) {
                for function_code in enum_of(&schema, &format!("{item}/function_code/enum")) {
                    let config = config_with(json!({
                        "uri": "external.Area.Dev.signal",
                        "address": 0,
                        "function_code": function_code,
                        "data_type": data_type,
                        "word_order": word_order,
                    }));
                    let parsed: ModbusDriverConfig = serde_json::from_value(config.clone())
                        .unwrap_or_else(|e| panic!("{config} does not parse: {e}"));
                    parsed
                        .validate()
                        .unwrap_or_else(|e| panic!("{config} is refused: {e}"));
                }
            }
        }
        for transport in enum_of(&schema, "/properties/transport/enum") {
            let config = json!({ "transport": transport, "host": "h", "mappings": [] });
            let parsed: ModbusDriverConfig = serde_json::from_value(config).unwrap();
            parsed.validate().unwrap();
        }
    }

    #[test]
    fn the_schema_defaults_are_the_driver_defaults() {
        let schema = config_schema();
        let parsed: ModbusDriverConfig = serde_json::from_value(config_with(json!({
            "uri": "external.Area.Dev.signal", "address": 0, "function_code": 3
        })))
        .unwrap();
        let default = |p: &str| schema.pointer(p).cloned().unwrap();
        assert_eq!(default("/properties/port/default"), json!(parsed.port));
        assert_eq!(
            default("/properties/slave_id/default"),
            json!(parsed.slave_id)
        );
        assert_eq!(
            default("/properties/timeout_ms/default"),
            json!(parsed.timeout_ms)
        );
        let m = &parsed.mappings[0];
        let item = "/properties/mappings/items/properties";
        assert_eq!(
            default(&format!("{item}/data_type/default")),
            json!(m.data_type)
        );
        assert_eq!(
            default(&format!("{item}/word_order/default")),
            json!(m.word_order)
        );
        assert_eq!(
            default(&format!("{item}/scale/default")).as_f64(),
            Some(m.scale)
        );
    }

    #[test]
    fn the_schema_limits_are_the_driver_limits() {
        let schema = config_schema();
        let limit = |p: &str| schema.pointer(p).and_then(Value::as_u64).unwrap();
        let check = |slave_id: u64, timeout_ms: u64| {
            let parsed: ModbusDriverConfig = serde_json::from_value(json!({
                "transport": "tcp", "host": "h", "slave_id": slave_id, "timeout_ms": timeout_ms
            }))
            .unwrap();
            parsed.validate().is_ok()
        };
        let (smin, smax) = (
            limit("/properties/slave_id/minimum"),
            limit("/properties/slave_id/maximum"),
        );
        let (tmin, tmax) = (
            limit("/properties/timeout_ms/minimum"),
            limit("/properties/timeout_ms/maximum"),
        );
        assert!(check(smin, 1000) && check(smax, 1000));
        assert!(!check(smin - 1, 1000) && !check(smax + 1, 1000));
        assert!(check(1, tmin) && check(1, tmax));
        assert!(!check(1, tmin - 1) && !check(1, tmax + 1));
    }

    #[test]
    fn the_manifest_names_the_driver_a_connection_uses() {
        let m = manifest();
        assert_eq!(m.provides, "modbus");
        assert_eq!(m.kind, ModuleKind::Driver);
        assert_eq!(m.protocol, PROTOCOL_VERSION);
    }
}
