//! A runtime module that does not do as it is told, for the tests of what the core does not
//! leave to a module (ADR-013 §2.4): it reports RUN whatever mode it is put in, and from its
//! first exchange it asks for every output to be written 300.
//!
//! A real module stops when stopped. The core's promise that a stopped program writes nothing
//! must not depend on that.

use async_trait::async_trait;
use mythos_module::protocol::{
    ExchangeReply, ModuleKind, ModuleManifest, RuntimeMode, RuntimeProgram, RuntimeState,
    WireInput, WireOutput, WireSample, PROTOCOL_VERSION,
};
use mythos_module::RuntimeEngine;
use mythos_plugin_sdk::{
    MythosPlugin, PluginContext, PluginError, PluginHealthReport, PluginMetadata, PluginType,
    ProtocolDriver, SampleValue, TagDeclaration, TagSample,
};
use serde_json::{json, Value};

#[derive(Default)]
struct Stubborn {
    program: Option<RuntimeProgram>,
}

impl Stubborn {
    fn state(&self) -> RuntimeState {
        RuntimeState {
            tasks: Vec::new(),
            programs: Vec::new(),
            mode: RuntimeMode::Run,
            fault: None,
            program: "stubborn".to_string(),
            version: "1".to_string(),
            scans: 1,
            last_scan_us: 0,
            max_scan_us: 0,
            overruns: 0,
            scan_age_us: None,
            forced: Vec::new(),
        }
    }
}

fn program_of(config: &Value) -> Result<RuntimeProgram, PluginError> {
    serde_json::from_value(config["program"].clone())
        .map_err(|e| PluginError::InvalidConfiguration(e.to_string()))
}

fn declarations(program: &RuntimeProgram) -> Vec<TagDeclaration> {
    program
        .published
        .iter()
        .map(|p| TagDeclaration {
            uri: p.tag.clone(),
            description: "a constant".to_string(),
            writable: false,
        })
        .collect()
}

#[async_trait]
impl MythosPlugin for Stubborn {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            id: "org.mythos.test.stubborn".to_string(),
            name: "stubborn runtime".to_string(),
            version: "0".to_string(),
            author: "test".to_string(),
            description: String::new(),
            plugin_type: PluginType::ProcessEngine,
            capabilities: Vec::new(),
        }
    }
    async fn init(&mut self, _ctx: &PluginContext) -> Result<(), PluginError> {
        Ok(())
    }
    async fn start(&mut self, _ctx: &PluginContext, config: &Value) -> Result<(), PluginError> {
        self.program = Some(program_of(config)?);
        Ok(())
    }
    async fn stop(&mut self, _ctx: &PluginContext) -> Result<(), PluginError> {
        Ok(())
    }
    async fn health_check(&self) -> Result<PluginHealthReport, PluginError> {
        Ok(PluginHealthReport::healthy("stubborn"))
    }
}

#[async_trait]
impl ProtocolDriver for Stubborn {
    async fn describe_config(&self, config: &Value) -> Result<Vec<TagDeclaration>, PluginError> {
        Ok(declarations(&program_of(config)?))
    }
    fn declared_tags(&self) -> Vec<TagDeclaration> {
        self.program.as_ref().map(declarations).unwrap_or_default()
    }
    async fn poll_cycle(&mut self, _ctx: &PluginContext) -> Result<Vec<TagSample>, PluginError> {
        Err(PluginError::RuntimeError(
            "exchanged with, not polled".to_string(),
        ))
    }
    async fn write_tag(
        &mut self,
        _ctx: &PluginContext,
        _tag_uri: &str,
        _value: &Value,
    ) -> Result<(), PluginError> {
        Err(PluginError::InvalidTag("nothing is writable".to_string()))
    }
}

#[async_trait]
impl RuntimeEngine for Stubborn {
    async fn exchange(
        &mut self,
        _ctx: &PluginContext,
        _inputs: Vec<WireInput>,
    ) -> Result<ExchangeReply, PluginError> {
        let program = self
            .program
            .as_ref()
            .ok_or_else(|| PluginError::RuntimeError("not started".to_string()))?;
        Ok(ExchangeReply {
            state: self.state(),
            samples: program
                .published
                .iter()
                .map(|p| WireSample::from(&TagSample::value(p.tag.clone(), SampleValue::I32(1))))
                .collect(),
            outputs: program
                .outputs
                .iter()
                .map(|o| WireOutput {
                    tag: o.tag.clone(),
                    value: json!(300),
                })
                .collect(),
        })
    }

    /// Told to stop, it says it runs.
    async fn set_mode(
        &mut self,
        _ctx: &PluginContext,
        _run: bool,
        _cold: bool,
    ) -> Result<RuntimeState, PluginError> {
        Ok(self.state())
    }
}

fn main() -> std::process::ExitCode {
    mythos_module::run_runtime_module(
        ModuleManifest {
            protocol: PROTOCOL_VERSION,
            id: "org.mythos.test.stubborn".to_string(),
            name: "Stubborn runtime".to_string(),
            version: "0.0.0".to_string(),
            kind: ModuleKind::Runtime,
            provides: "stubborn".to_string(),
            description: "Ignores STOP; for tests.".to_string(),
            capabilities: Vec::new(),
            config_schema: Value::Null,
            routes: Vec::new(),
            always_on: false,
            profile: Value::Null,
        },
        Stubborn::default,
    )
}
