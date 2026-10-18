use mythos_strucpp::compiler::{CompilerConfig, StrucppCompiler};
use mythos_strucpp::hot_reload::LoadedLogicModule;
use tempfile::tempdir;

const TEST_ST_SRC: &str = r#"
PROGRAM SimpleProg
  VAR
    in_val AT %IX0.0 : BOOL;
    out_val AT %QX0.0 : BOOL;
    counter : INT := 0;
  END_VAR
  counter := counter + 1;
  out_val := in_val;
END_PROGRAM

CONFIGURATION Config0
  RESOURCE Res0 ON PLC
    TASK Task0(INTERVAL := T#10ms, PRIORITY := 1);
    PROGRAM Inst0 WITH Task0 : SimpleProg;
  END_RESOURCE
END_CONFIGURATION
"#;

#[test]
fn test_compiler_pipeline_and_so_generation() {
    let temp = tempdir().expect("create temp dir");
    let config = CompilerConfig {
        output_dir: temp.path().to_path_buf(),
        ..Default::default()
    };

    let compiler = StrucppCompiler::new(config);
    let artifacts = compiler
        .compile_source(TEST_ST_SRC)
        .expect("compilation successful");

    assert!(artifacts.shared_library.exists());
    assert_eq!(
        artifacts.shared_library.extension().and_then(|e| e.to_str()),
        Some(std::env::consts::DLL_EXTENSION)
    );
    assert!(artifacts.cpp_source.exists());
    assert!(artifacts.cpp_header.exists());

    // Test zero-recompilation caching
    let artifacts_cached = compiler
        .compile_source(TEST_ST_SRC)
        .expect("cached compilation");
    assert_eq!(artifacts.source_hash, artifacts_cached.source_hash);

    // Test dynamic loading of shared library via LoadedLogicModule
    let loaded = LoadedLogicModule::load(&artifacts.shared_library).expect("load dynamic module");
    assert_eq!(loaded.located_descriptors.len(), 2);

    let in_desc = loaded
        .located_descriptors
        .iter()
        .find(|d| d.iec_address() == "%IX0.0");
    let out_desc = loaded
        .located_descriptors
        .iter()
        .find(|d| d.iec_address() == "%QX0.0");
    assert!(in_desc.is_some());
    assert!(out_desc.is_some());

    // Execute one cycle step
    let in_desc = in_desc.unwrap();
    let out_desc = out_desc.unwrap();

    unsafe {
        in_desc.write_bool(true);
    }

    loaded.step_cycle();

    unsafe {
        assert!(out_desc.read_bool());
    }
}

#[test]
fn test_standalone_fallback_compilation() {
    let temp = tempdir().expect("create temp dir");
    let config = CompilerConfig {
        strucpp_cli: "/nonexistent/strucpp/cli.js".into(),
        strucpp_include: "/nonexistent/strucpp/include".into(),
        output_dir: temp.path().to_path_buf(),
        ..Default::default()
    };

    let compiler = StrucppCompiler::new(config);
    let artifacts = compiler
        .compile_source(TEST_ST_SRC)
        .expect("standalone fallback compilation successful");

    assert!(artifacts.shared_library.exists());
    assert_eq!(
        artifacts.shared_library.extension().and_then(|e| e.to_str()),
        Some(std::env::consts::DLL_EXTENSION)
    );
    assert!(artifacts.cpp_source.exists());
    assert!(artifacts.cpp_header.exists());

    // Test dynamic loading of standalone shared library via LoadedLogicModule
    let loaded = LoadedLogicModule::load(&artifacts.shared_library).expect("load dynamic module");
    assert_eq!(loaded.located_descriptors.len(), 2);

    let in_desc = loaded
        .located_descriptors
        .iter()
        .find(|d| d.iec_address() == "%IX0.0")
        .unwrap();
    let out_desc = loaded
        .located_descriptors
        .iter()
        .find(|d| d.iec_address() == "%QX0.0")
        .unwrap();

    unsafe {
        in_desc.write_bool(true);
    }

    loaded.step_cycle();

    unsafe {
        assert!(out_desc.read_bool());
    }
}
