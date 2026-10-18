use mythos_strucpp::compiler::{CompilerConfig, StrucppCompiler};
use mythos_strucpp::hot_reload::HotReloadManager;
use std::time::Duration;
use tempfile::tempdir;

const VERSION_A_ST: &str = r#"
PROGRAM LogicV1
  VAR
    out_val AT %QX0.0 : BOOL;
  END_VAR
  out_val := FALSE;
END_PROGRAM

CONFIGURATION Config0
  RESOURCE Res0 ON PLC
    TASK Task0(INTERVAL := T#10ms, PRIORITY := 1);
    PROGRAM Inst0 WITH Task0 : LogicV1;
  END_RESOURCE
END_CONFIGURATION
"#;

const VERSION_B_ST: &str = r#"
PROGRAM LogicV2
  VAR
    out_val AT %QX0.0 : BOOL;
  END_VAR
  out_val := TRUE;
END_PROGRAM

CONFIGURATION Config0
  RESOURCE Res0 ON PLC
    TASK Task0(INTERVAL := T#10ms, PRIORITY := 1);
    PROGRAM Inst0 WITH Task0 : LogicV2;
  END_RESOURCE
END_CONFIGURATION
"#;

#[test]
fn test_hot_reload_sub_50ms_sys_req_023() {
    let temp = tempdir().expect("tempdir");
    let config = CompilerConfig {
        output_dir: temp.path().to_path_buf(),
        ..Default::default()
    };

    let compiler = StrucppCompiler::new(config);
    let art_a = compiler.compile_source(VERSION_A_ST).expect("compile v1");
    let art_b = compiler.compile_source(VERSION_B_ST).expect("compile v2");

    let hot_reload = HotReloadManager::new();

    // Initial load
    hot_reload.stage_new_binary(&art_a.shared_library).expect("stage v1");
    let initial_swap = hot_reload.commit_swap_at_boundary().expect("swap v1");
    assert!(initial_swap.is_some());

    // Step cycle v1 -> out_val should be FALSE
    {
        let guard = hot_reload.active_module();
        let module = guard.as_ref().unwrap();
        module.step_cycle();
        let out_desc = module.located_descriptors.iter().find(|d| d.iec_address() == "%QX0.0").unwrap();
        unsafe {
            assert!(!out_desc.read_bool());
        }
    }

    // Now hot reload version B while keeping outputs alive
    hot_reload.stage_new_binary(&art_b.shared_library).expect("stage v2");
    assert!(hot_reload.is_swap_pending());

    let swap_duration = hot_reload.commit_swap_at_boundary().expect("commit swap v2").expect("duration");
    // Verify SLA: < 50ms (SYS-REQ-023)
    assert!(swap_duration < Duration::from_millis(50), "Swap took {:?}, exceeding 50ms!", swap_duration);

    // Step cycle v2 -> out_val should immediately be TRUE
    {
        let guard = hot_reload.active_module();
        let module = guard.as_ref().unwrap();
        module.step_cycle();
        let out_desc = module.located_descriptors.iter().find(|d| d.iec_address() == "%QX0.0").unwrap();
        unsafe {
            assert!(out_desc.read_bool());
        }
    }
}
