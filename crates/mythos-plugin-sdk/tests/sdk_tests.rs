use mythos_plugin_sdk::*;
use serde_json::json;
use std::collections::HashSet;

struct MockDriver {
    healthy: bool,
    poll_count: u32,
}

#[async_trait::async_trait]
impl MythosPlugin for MockDriver {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            id: "org.mythos.driver.mock".to_string(),
            name: "Mock Driver".to_string(),
            version: "0.1.0".to_string(),
            author: "MythOS Team".to_string(),
            description: "Mock Driver for testing".to_string(),
            plugin_type: PluginType::ProtocolDriver,
            capabilities: vec!["tagbus:write".to_string()],
        }
    }

    async fn init(&mut self, ctx: &PluginContext) -> Result<(), PluginError> {
        ctx.check_capability("tagbus:write")?;
        Ok(())
    }

    async fn start(
        &mut self,
        _ctx: &PluginContext,
        config: &serde_json::Value,
    ) -> Result<(), PluginError> {
        self.validate_config(config)?;
        Ok(())
    }

    async fn stop(&mut self, _ctx: &PluginContext) -> Result<(), PluginError> {
        self.healthy = false;
        Ok(())
    }

    async fn health_check(&self) -> Result<PluginHealthReport, PluginError> {
        if self.healthy {
            Ok(PluginHealthReport::healthy("Operating nominally"))
        } else {
            Ok(PluginHealthReport::stopped("Stopped"))
        }
    }
}

#[async_trait::async_trait]
impl ProtocolDriver for MockDriver {
    fn validate_config(&self, config: &serde_json::Value) -> Result<(), PluginError> {
        if config.get("port").is_none() {
            return Err(PluginError::InvalidConfiguration("Missing 'port'".into()));
        }
        Ok(())
    }

    async fn poll_cycle(&mut self, ctx: &PluginContext) -> Result<u32, PluginError> {
        ctx.check_capability("tagbus:write")?;
        self.poll_count += 1;
        Ok(self.poll_count)
    }

    async fn write_tag(
        &mut self,
        ctx: &PluginContext,
        tag_uri: &str,
        _value: &serde_json::Value,
    ) -> Result<(), PluginError> {
        ctx.check_capability("tagbus:write")?;
        if !tag_uri.starts_with("external.") {
            return Err(PluginError::InvalidTag("Tag must be external.*".into()));
        }
        Ok(())
    }
}

#[tokio::test]
async fn test_plugin_context_capabilities() {
    let mut caps = HashSet::new();
    caps.insert("tagbus:write".to_string());
    let ctx = PluginContext::new("mock", caps, "/tmp");

    assert!(ctx.has_capability("tagbus:write"));
    assert!(!ctx.has_capability("network:raw"));
    assert!(ctx.check_capability("tagbus:write").is_ok());
    assert!(ctx.check_capability("network:raw").is_err());
}

#[tokio::test]
async fn test_protocol_driver_lifecycle() {
    let mut caps = HashSet::new();
    caps.insert("tagbus:write".to_string());
    let ctx = PluginContext::new("mock", caps, "/tmp");

    let mut driver = MockDriver {
        healthy: true,
        poll_count: 0,
    };

    assert!(driver.init(&ctx).await.is_ok());

    // Bad config
    let bad_config = json!({});
    assert!(driver.start(&ctx, &bad_config).await.is_err());

    // Good config
    let good_config = json!({"port": 502});
    assert!(driver.start(&ctx, &good_config).await.is_ok());

    let health = driver.health_check().await.unwrap();
    assert_eq!(health.status, HealthStatus::Healthy);

    let polled = driver.poll_cycle(&ctx).await.unwrap();
    assert_eq!(polled, 1);

    // Write valid tag
    assert!(driver
        .write_tag(&ctx, "external.Plant.PT101.pv", &json!(42.0))
        .await
        .is_ok());
    // Write invalid tag
    assert!(driver
        .write_tag(&ctx, "internal.Timer", &json!(1))
        .await
        .is_err());

    assert!(driver.stop(&ctx).await.is_ok());
    let health_after = driver.health_check().await.unwrap();
    assert_eq!(health_after.status, HealthStatus::Stopped);
}
