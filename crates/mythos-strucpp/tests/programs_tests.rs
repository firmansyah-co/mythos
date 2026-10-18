use mythos_strucpp::compiler::{CompilerConfig, StrucppCompiler};
use mythos_strucpp::hot_reload::LoadedLogicModule;
use tempfile::tempdir;

const TON_SRC: &str = r#"
PROGRAM TonTest
  VAR
    in_trig AT %IX0.0 : BOOL;
    out_q AT %QX0.0 : BOOL;
    timer : TON;
  END_VAR
  timer(IN := in_trig, PT := T#50ms);
  out_q := timer.Q;
END_PROGRAM

CONFIGURATION ConfigTon
  RESOURCE Res0 ON PLC
    TASK Task0(INTERVAL := T#10ms, PRIORITY := 1);
    PROGRAM Inst0 WITH Task0 : TonTest;
  END_RESOURCE
END_CONFIGURATION
"#;

const CTU_SRC: &str = r#"
PROGRAM CtuTest
  VAR
    count_up AT %IX0.0 : BOOL;
    reset_in AT %IX0.1 : BOOL;
    out_q AT %QX0.0 : BOOL;
    counter : CTU;
  END_VAR
  counter(CU := count_up, R := reset_in, PV := 3);
  out_q := counter.Q;
END_PROGRAM

CONFIGURATION ConfigCtu
  RESOURCE Res0 ON PLC
    TASK Task0(INTERVAL := T#10ms, PRIORITY := 1);
    PROGRAM Inst0 WITH Task0 : CtuTest;
  END_RESOURCE
END_CONFIGURATION
"#;

const STATE_MACHINE_SRC: &str = r#"
PROGRAM StateMachine
  VAR
    start_btn AT %IX0.0 : BOOL;
    stop_btn AT %IX0.1 : BOOL;
    motor_run AT %QX0.0 : BOOL;
    fault_led AT %QX0.1 : BOOL;
    state : INT := 0; (* 0: Idle, 1: Running, 2: Fault *)
  END_VAR

  CASE state OF
    0: (* Idle *)
      motor_run := FALSE;
      fault_led := FALSE;
      IF start_btn THEN
        state := 1;
      END_IF;
    1: (* Running *)
      motor_run := TRUE;
      IF stop_btn THEN
        state := 0;
      END_IF;
    ELSE
      motor_run := FALSE;
      fault_led := TRUE;
  END_CASE;
END_PROGRAM

CONFIGURATION ConfigSM
  RESOURCE Res0 ON PLC
    TASK Task0(INTERVAL := T#10ms, PRIORITY := 1);
    PROGRAM Inst0 WITH Task0 : StateMachine;
  END_RESOURCE
END_CONFIGURATION
"#;

#[test]
fn test_ton_timer_logic() {
    let temp = tempdir().expect("tempdir");
    let config = CompilerConfig {
        output_dir: temp.path().to_path_buf(),
        ..Default::default()
    };

    let compiler = StrucppCompiler::new(config);
    let art = compiler.compile_source(TON_SRC).expect("compile TON");
    let loaded = LoadedLogicModule::load(&art.shared_library).expect("load TON");

    let in_trig = loaded.located_descriptors.iter().find(|d| d.iec_address() == "%IX0.0").unwrap();
    let out_q = loaded.located_descriptors.iter().find(|d| d.iec_address() == "%QX0.0").unwrap();

    // Initially false
    loaded.step_cycle();
    unsafe {
        assert!(!out_q.read_bool());
        in_trig.write_bool(true);
    }

    // Step cycle with trig = true
    loaded.step_cycle();
    unsafe {
        // PT is 50ms, so immediate output is still false
        assert!(!out_q.read_bool());
    }
}

#[test]
fn test_ctu_counter_logic() {
    let temp = tempdir().expect("tempdir");
    let config = CompilerConfig {
        output_dir: temp.path().to_path_buf(),
        ..Default::default()
    };

    let compiler = StrucppCompiler::new(config);
    let art = compiler.compile_source(CTU_SRC).expect("compile CTU");
    let loaded = LoadedLogicModule::load(&art.shared_library).expect("load CTU");

    let cu = loaded.located_descriptors.iter().find(|d| d.iec_address() == "%IX0.0").unwrap();
    let rst = loaded.located_descriptors.iter().find(|d| d.iec_address() == "%IX0.1").unwrap();
    let q = loaded.located_descriptors.iter().find(|d| d.iec_address() == "%QX0.0").unwrap();

    unsafe {
        rst.write_bool(false);
        cu.write_bool(false);
    }
    loaded.step_cycle();
    unsafe {
        assert!(!q.read_bool());
    }

    // Pulse 1
    unsafe { cu.write_bool(true); }
    loaded.step_cycle();
    unsafe { cu.write_bool(false); }
    loaded.step_cycle();

    // Pulse 2
    unsafe { cu.write_bool(true); }
    loaded.step_cycle();
    unsafe { cu.write_bool(false); }
    loaded.step_cycle();

    // Pulse 3 (hits PV=3)
    unsafe { cu.write_bool(true); }
    loaded.step_cycle();
    unsafe {
        assert!(q.read_bool(), "Counter should trigger Q at PV=3");
    }
}

#[test]
fn test_state_machine_logic() {
    let temp = tempdir().expect("tempdir");
    let config = CompilerConfig {
        output_dir: temp.path().to_path_buf(),
        ..Default::default()
    };

    let compiler = StrucppCompiler::new(config);
    let art = compiler.compile_source(STATE_MACHINE_SRC).expect("compile SM");
    let loaded = LoadedLogicModule::load(&art.shared_library).expect("load SM");

    let start = loaded.located_descriptors.iter().find(|d| d.iec_address() == "%IX0.0").unwrap();
    let stop = loaded.located_descriptors.iter().find(|d| d.iec_address() == "%IX0.1").unwrap();
    let motor = loaded.located_descriptors.iter().find(|d| d.iec_address() == "%QX0.0").unwrap();
    let fault = loaded.located_descriptors.iter().find(|d| d.iec_address() == "%QX0.1").unwrap();

    // 1. Initial State: Idle
    loaded.step_cycle();
    unsafe {
        assert!(!motor.read_bool());
        assert!(!fault.read_bool());
    }

    // 2. Press start button -> Transitions to Running
    unsafe { start.write_bool(true); }
    loaded.step_cycle();
    unsafe { start.write_bool(false); }
    loaded.step_cycle();

    unsafe {
        assert!(motor.read_bool(), "Motor should be running");
        assert!(!fault.read_bool());
    }

    // 3. Press stop button -> Transitions to Idle
    unsafe { stop.write_bool(true); }
    loaded.step_cycle();
    unsafe { stop.write_bool(false); }
    loaded.step_cycle();

    unsafe {
        assert!(!motor.read_bool(), "Motor should stop");
        assert!(!fault.read_bool());
    }
}
