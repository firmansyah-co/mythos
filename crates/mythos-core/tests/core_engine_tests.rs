use mythos_core::*;
use mythos_shm::TagValueRaw;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_canonical_external_tag_parsing() {
    let uri = "external.BoilerRoom.PT101.pressure.pv";
    let parsed = TagUri::parse(uri).unwrap();
    assert_eq!(parsed.domain, TagDomain::External);
    assert_eq!(parsed.plant_area, "BoilerRoom");
    assert_eq!(parsed.device_or_block, "PT101");
    assert_eq!(parsed.variable, "pressure");
    assert_eq!(parsed.subfield.as_deref(), Some("pv"));
}

#[test]
fn test_canonical_internal_tag_parsing() {
    let uri = "internal.BoilerRoom.SeqController.step_timer";
    let parsed = TagUri::parse(uri).unwrap();
    assert_eq!(parsed.domain, TagDomain::Internal);
    assert_eq!(parsed.plant_area, "BoilerRoom");
    assert_eq!(parsed.device_or_block, "SeqController");
    assert_eq!(parsed.variable, "step_timer");
    assert_eq!(parsed.subfield, None);
}

#[test]
fn test_canonical_kernel_tag_parsing() {
    let uri = "kernel.system.cycle_time_us";
    let parsed = TagUri::parse(uri).unwrap();
    assert_eq!(parsed.domain, TagDomain::Kernel);
    assert_eq!(parsed.device_or_block, "system");
    assert_eq!(parsed.variable, "cycle_time_us");
}

#[test]
fn test_hierarchical_nested_plant_area() {
    let uri = "external.PKS.Station2.Supmea_P200.pressure.pv";
    let parsed = TagUri::parse(uri).unwrap();
    assert_eq!(parsed.domain, TagDomain::External);
    assert_eq!(parsed.plant_area, "PKS.Station2");
    assert_eq!(parsed.device_or_block, "Supmea_P200");
    assert_eq!(parsed.variable, "pressure");
    assert_eq!(parsed.subfield.as_deref(), Some("pv"));
}

#[test]
fn test_prohibited_protocol_rejection() {
    let invalid_uris = [
        "external.modbus.PT101.pressure",
        "external.BoilerRoom.modbus_tcp.pressure", // device is fine, but in area:
        "external.profinet.Pump1.speed",
        "external.eip.Drive1.freq",
        "external.BoilerRoom.opcua.temp",
    ];

    assert!(matches!(
        TagUri::parse(invalid_uris[0]),
        Err(TagNamespaceError::ProhibitedProtocolInNamespace { .. })
    ));
    assert!(matches!(
        TagUri::parse(invalid_uris[2]),
        Err(TagNamespaceError::ProhibitedProtocolInNamespace { .. })
    ));
}

#[test]
fn test_tag_catalog_registration_and_lookup() {
    let mut catalog = TagCatalog::new();
    let meta1 = TagMetadata::new("external.BoilerRoom.PT101.pressure.pv", 1)
        .unwrap()
        .with_description("Main Steam Pressure")
        .with_units("bar")
        .with_retention(RetentionMode::Persistent)
        .with_access(AccessMode::ReadOnly);

    let meta2 = TagMetadata::new("internal.BoilerRoom.SeqController.step_timer", 2)
        .unwrap()
        .with_retention(RetentionMode::Volatile);

    catalog.register(meta1.clone()).unwrap();
    catalog.register(meta2).unwrap();

    assert_eq!(catalog.len(), 2);
    assert_eq!(
        catalog.get_by_uri("external.BoilerRoom.PT101.pressure.pv"),
        Some(&meta1)
    );
    assert_eq!(catalog.get_by_slot(1), Some(&meta1));

    let external_tags = catalog.find_by_domain(TagDomain::External);
    assert_eq!(external_tags.len(), 1);
    assert_eq!(external_tags[0].slot_id, 1);
}

#[test]
fn test_nvram_wal_append_and_recovery() {
    let temp_dir = tempdir().unwrap();
    let nvram_path = temp_dir.path().join("nvram.dat");

    {
        let mut engine = NvramWalEngine::new(&nvram_path);
        let lsn1 = engine
            .append_record(10, TagValueRaw::from_f64(12.34), 1000)
            .unwrap();
        let lsn2 = engine
            .append_record(20, TagValueRaw::from_i32(500), 2000)
            .unwrap();
        // Update tag 10 again
        let lsn3 = engine
            .append_record(10, TagValueRaw::from_f64(56.78), 3000)
            .unwrap();

        assert_eq!(lsn1, 1);
        assert_eq!(lsn2, 2);
        assert_eq!(lsn3, 3);
    }

    // Recover in fresh engine
    let mut recovery_engine = NvramWalEngine::new(&nvram_path);
    let recovered = recovery_engine.recover().unwrap();

    assert_eq!(recovered.len(), 2);
    assert_eq!(recovered[0].0, 10);
    assert_eq!(recovered[0].1.as_f64(), 56.78);
    assert_eq!(recovered[1].0, 20);
    assert_eq!(recovered[1].1.as_i32(), 500);
}

#[test]
fn test_nvram_wal_crc_corruption_detection() {
    let temp_dir = tempdir().unwrap();
    let nvram_path = temp_dir.path().join("corrupt_nvram.dat");

    {
        let mut engine = NvramWalEngine::new(&nvram_path);
        engine
            .append_record(1, TagValueRaw::from_u32(100), 100)
            .unwrap();
    }

    // Corrupt one byte of payload
    let mut data = fs::read(&nvram_path).unwrap();
    data[15] ^= 0xFF; // Flip bit
    fs::write(&nvram_path, data).unwrap();

    let mut recovery_engine = NvramWalEngine::new(&nvram_path);
    let res = recovery_engine.recover();
    assert!(matches!(res, Err(NvramError::ChecksumMismatch { .. })));
}

#[test]
fn test_nvram_wal_compaction() {
    let temp_dir = tempdir().unwrap();
    let nvram_path = temp_dir.path().join("nvram_compact.dat");

    let mut engine = NvramWalEngine::new(&nvram_path);
    for i in 0..100 {
        engine
            .append_record(1, TagValueRaw::from_i32(i), i as u64)
            .unwrap();
    }
    // File size has 100 records = 3400 bytes
    let size_before = fs::metadata(&nvram_path).unwrap().len();
    assert_eq!(size_before, 100 * 34);

    let compacted_count = engine.compact().unwrap();
    assert_eq!(compacted_count, 1);

    let size_after = fs::metadata(&nvram_path).unwrap().len();
    assert_eq!(size_after, 34);

    // Verify recovery after compaction
    let mut recovery_engine = NvramWalEngine::new(&nvram_path);
    let state = recovery_engine.recover().unwrap();
    assert_eq!(state.len(), 1);
    assert_eq!(state[0].0, 1);
    assert_eq!(state[0].1.as_i32(), 99);
}

#[test]
fn test_controller_storage_layout_and_symlink_switch() {
    let temp_dir = tempdir().unwrap();
    let storage = ControllerStorage::new(temp_dir.path()).unwrap();

    let proj1 = ProjectConfig::new("proj_line1", "Bottling Line 1");
    let proj2 = ProjectConfig::new("proj_line2", "Packaging Line 2");

    let path1 = storage.create_project(&proj1).unwrap();
    let _path2 = storage.create_project(&proj2).unwrap();

    assert!(path1.join("logic/active").is_dir());
    assert!(path1.join("screens/active").is_dir());
    assert!(path1.join("project.active.json").is_file());

    let mut projects = storage.list_projects().unwrap();
    projects.sort();
    assert_eq!(projects, vec!["proj_line1", "proj_line2"]);

    // Initially no active project
    assert_eq!(storage.get_active_project().unwrap(), None);

    // Switch to proj1
    storage.switch_active_project("proj_line1").unwrap();
    assert_eq!(
        storage.get_active_project().unwrap(),
        Some("proj_line1".to_string())
    );

    // Switch to proj2 atomically
    storage.switch_active_project("proj_line2").unwrap();
    assert_eq!(
        storage.get_active_project().unwrap(),
        Some("proj_line2".to_string())
    );
}

#[test]
fn test_project_wal_append_replay_and_recovery() {
    let temp_dir = tempdir().unwrap();
    let storage = ControllerStorage::new(temp_dir.path()).unwrap();
    let proj = ProjectConfig::new("proj_wal_test", "WAL Test Project");
    storage.create_project(&proj).unwrap();

    let wal = storage.get_wal("proj_wal_test");
    assert!(!wal.has_pending_entries());

    // Append two delta edits
    let logic_code = b"PROGRAM Test\nVAR x: INT; END_VAR\nEND_PROGRAM";
    wal.append("logic/draft/main.st", logic_code, 1000).unwrap();
    wal.append("tasks.draft.json", b"{\"tasks\": []}", 2000)
        .unwrap();

    assert!(wal.has_pending_entries());

    let entries = wal.replay().unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].doc_path, "logic/draft/main.st");
    assert_eq!(entries[0].payload, logic_code);
    assert_eq!(entries[1].doc_path, "tasks.draft.json");

    // Recover project WAL into actual files
    let recovered_count = storage.recover_project_wal("proj_wal_test").unwrap();
    assert_eq!(recovered_count, 2);

    let target_file = storage
        .project_dir("proj_wal_test")
        .join("logic/draft/main.st");
    assert_eq!(fs::read(&target_file).unwrap(), logic_code);

    // After recovery, WAL is truncated
    assert!(!wal.has_pending_entries());
}

#[test]
fn test_project_draft_commit_with_rollback() {
    let temp_dir = tempdir().unwrap();
    let storage = ControllerStorage::new(temp_dir.path()).unwrap();
    let proj = ProjectConfig::new("proj_commit_test", "Commit Test Project");
    storage.create_project(&proj).unwrap();

    let pdir = storage.project_dir("proj_commit_test");
    let active_path = pdir.join("project.active.json");
    let draft_path = pdir.join("project.draft.json");

    // Edit draft
    let mut draft_config = proj.clone();
    draft_config.description = "Updated staging draft description".to_string();
    fs::write(
        &draft_path,
        serde_json::to_string_pretty(&draft_config).unwrap(),
    )
    .unwrap();

    // Commit draft to active
    storage.commit_draft_to_active("proj_commit_test").unwrap();

    let active_content = fs::read_to_string(&active_path).unwrap();
    let updated_config: ProjectConfig = serde_json::from_str(&active_content).unwrap();
    assert_eq!(
        updated_config.description,
        "Updated staging draft description"
    );
}

#[tokio::test]
async fn test_plugin_host_registration_and_lifecycle() {
    use mythos_plugin_sdk::*;
    use serde_json::json;
    use std::collections::HashSet;

    struct DummyPlugin {
        started: bool,
    }

    #[async_trait::async_trait]
    impl MythosPlugin for DummyPlugin {
        fn metadata(&self) -> PluginMetadata {
            PluginMetadata {
                id: "test.dummy".to_string(),
                name: "Dummy".to_string(),
                version: "1.0.0".to_string(),
                author: "Test".to_string(),
                description: "Test".to_string(),
                plugin_type: PluginType::Extension,
                capabilities: vec![],
            }
        }
        async fn init(&mut self, _ctx: &PluginContext) -> Result<(), PluginError> {
            Ok(())
        }
        async fn start(
            &mut self,
            _ctx: &PluginContext,
            _config: &serde_json::Value,
        ) -> Result<(), PluginError> {
            self.started = true;
            Ok(())
        }
        async fn stop(&mut self, _ctx: &PluginContext) -> Result<(), PluginError> {
            self.started = false;
            Ok(())
        }
        async fn health_check(&self) -> Result<PluginHealthReport, PluginError> {
            if self.started {
                Ok(PluginHealthReport::healthy("Running"))
            } else {
                Ok(PluginHealthReport::stopped("Stopped"))
            }
        }
    }

    let host = mythos_core::PluginHost::new();
    let ctx = PluginContext::new("test.dummy", HashSet::new(), "/tmp");
    let plugin = Box::new(DummyPlugin { started: false });

    let meta = host.register(plugin, ctx).await.unwrap();
    assert_eq!(meta.id, "test.dummy");

    // Init & Start
    host.initialize_plugin("test.dummy").await.unwrap();
    host.start_plugin("test.dummy", &json!({})).await.unwrap();

    let reports = host.health_check_all().await;
    assert_eq!(
        reports.get("test.dummy").unwrap().status,
        HealthStatus::Healthy
    );

    // Stop
    host.stop_plugin("test.dummy").await.unwrap();
    let report = host.health_check("test.dummy").await.unwrap();
    assert_eq!(report.status, HealthStatus::Stopped);
}

#[tokio::test]
async fn test_axum_api_gateway_rest_and_telemetry_stream() {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use mythos_core::api::{create_router, ApiState};
    use mythos_core::storage::ControllerStorage;
    use mythos_core::tag::{TagCatalog, TagMetadata};
    use mythos_shm::{QualityCode, ShmWriterHandle, TagFlags, TagValueRaw};
    use serde_json::json;
    use std::sync::Arc;
    use tokio::sync::RwLock;
    use tower::ServiceExt; // for oneshot()

    let tmp_dir = tempfile::tempdir().unwrap();
    let storage = Arc::new(ControllerStorage::new(tmp_dir.path()).unwrap());

    let mut catalog = TagCatalog::new();
    let meta = TagMetadata::new("external.BoilerRoom.PT101.pressure.pv", 0).unwrap();
    catalog.register(meta).unwrap();
    let tag_catalog = Arc::new(RwLock::new(catalog));

    // Create SHM segments for testing
    let shm_name = format!("mythos_test_api_{}", std::process::id());
    let mut writer = ShmWriterHandle::<mythos_pal::MemoryShm>::new(&shm_name, 64).unwrap();
    let reader = mythos_shm::ShmReaderHandle::<mythos_pal::MemoryShm>::new(&shm_name, 64).unwrap();

    // Write a tag value (e.g. 12.5 bar)
    writer
        .write_tag(
            0,
            0,
            QualityCode::GOOD,
            TagValueRaw::from_f64(12.5),
            1000,
            TagFlags::ACCESS_RO,
        )
        .unwrap();
    writer.publish();

    let state = ApiState::new(storage.clone(), tag_catalog.clone(), Some(reader));
    let router = create_router(state);

    // 1. Test GET /api/v1/health
    let req = Request::builder()
        .uri("/api/v1/health")
        .body(Body::empty())
        .unwrap();
    let resp = router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // 2. Test GET /api/v1/tags
    let req = Request::builder()
        .uri("/api/v1/tags")
        .body(Body::empty())
        .unwrap();
    let resp = router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // 3. Test POST /api/v1/projects
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/projects")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "id": "proj_alpha",
                "name": "Project Alpha",
                "description": "Industrial control setup",
                "author": "Neo"
            })
            .to_string(),
        ))
        .unwrap();
    let resp = router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    // 4. Test GET /api/v1/projects/proj_alpha
    let req = Request::builder()
        .uri("/api/v1/projects/proj_alpha")
        .body(Body::empty())
        .unwrap();
    let resp = router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // 5. Test POST /api/v1/projects/proj_alpha/activate
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/projects/proj_alpha/activate")
        .body(Body::empty())
        .unwrap();
    let resp = router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        storage.get_active_project().unwrap(),
        Some("proj_alpha".to_string())
    );

    // 6. Test GET /api/v1/telemetry/stream (verify SSE headers)
    let req = Request::builder()
        .uri("/api/v1/telemetry/stream")
        .body(Body::empty())
        .unwrap();
    let resp = router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "text/event-stream"
    );
}
