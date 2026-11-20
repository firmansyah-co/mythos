//! Each Sequential Function Chart here has a twin written by hand in Structured Text, and the
//! two, compiled, run on the VM through the same random inputs and clock: every variable the
//! program declares, every step's flag, every step's elapsed time while it is active, every
//! action's flag and every transition's kept condition and enable flag are equal after every
//! scan (ADR-013 §2.17).
//!
//! **The twins** are written from IEC 61131-3 ed. 3 §6.7 alone: each step a BOOL `x_<step>`;
//! its elapsed time the ET of a TON on its flag, called at the start of the scan and again after
//! the evolution (so its ET is the time since the scan that activated it); each action the
//! blocks of Figure 23 b) (ACTION_CONTROL without final scan), the library's RS, TON and
//! R_TRIG, with T the duration of the active time-related association or `T#0s` (rule c); each
//! transition `e_<localId>`, the AND of the steps before it, and `c_<localId>`, its condition;
//! the clearing written out by hand: a selection's transitions each AND NOT those of higher
//! priority, every step before a cleared transition deactivated and then every step after it
//! activated (§6.7.5). The charts cover single sequences, a selection divergence by position
//! (Table 61 feature 2a) and by number (2b), a selection convergence (3), a simultaneous
//! divergence and convergence (4a, 4c), a selection after a simultaneous convergence (4d), a
//! skip (5) and a loop (6) through a jump step; the qualifiers N, S, R, L, D, P, P1, DS;
//! actions that are BOOL variables, named actions and actions written in the action block, one
//! action in two steps; named transitions and a negated one; steps' times read in conditions;
//! and a chart in a function block, two instances of it called from an ST program.

mod support;
#[path = "../../mythos-logic-graphic/tests/support/tc6.rs"]
mod tc6;

use mythos_logic_compiler::{compile, compile_sources, At, Holds, Sources, TargetProfile};
use mythos_logic_vm::Value;
use support::Run;
use tc6::{project, Act, Pou};

const SEQUENCES: u64 = 200;
const SCANS: usize = 100;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        // xorshift64*
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    /// TRUE one time in `n`.
    fn one_in(&mut self, n: u64) -> bool {
        self.next() % n == 0
    }
}

/// What is compared besides the program's own variables: `(path in the chart, path in the
/// twin, the step flag in the twin that must be TRUE for it to be compared)`.
type Pairs = Vec<(String, String, Option<String>)>;

/// The pairs every chart has, from its probes: each step's flag and time, each action's flag,
/// each transition's enable flag and kept condition (compared where it was evaluated).
fn pairs_of(
    f: &Run,
    pou: &str,
    prefix: &str,
    steps: &[(u64, &str)],
    actions: &[(u64, &str)],
) -> Pairs {
    let mut out = Vec::new();
    for p in f.c.probes.iter().filter(|p| p.pou == pou) {
        let path = match &p.holds {
            Holds::Variable { path, .. } => format!("{prefix}{path}"),
            Holds::Guarded { path, ran } => {
                out.push((
                    format!("{prefix}{ran}"),
                    format!("{prefix}e_{}", p.local_id),
                    None,
                ));
                out.push((
                    format!("{prefix}{path}"),
                    format!("{prefix}c_{}", p.local_id),
                    Some(format!("{prefix}e_{}", p.local_id)),
                ));
                continue;
            }
            Holds::On => unreachable!(),
        };
        let step = |id: u64| {
            steps
                .iter()
                .find(|s| s.0 == id)
                .unwrap_or_else(|| panic!("step {id} is not listed"))
                .1
        };
        match p.at {
            At::Step => out.push((path, format!("{prefix}x_{}", step(p.local_id)), None)),
            At::StepTime => out.push((
                path,
                format!("{prefix}t_{}.ET", step(p.local_id)),
                Some(format!("{prefix}x_{}", step(p.local_id))),
            )),
            At::Action => {
                let twin = actions
                    .iter()
                    .find(|a| a.0 == p.local_id)
                    .unwrap_or_else(|| panic!("association {} is not listed", p.local_id))
                    .1;
                out.push((path, format!("{prefix}{twin}"), None));
            }
            ref other => panic!("{other:?} in a chart"),
        }
    }
    out
}

fn compare(
    name: &str,
    sfc: impl Fn() -> Run,
    st: impl Fn() -> Run,
    inputs: &[&str],
    pairs: Pairs,
    odds: u64,
) {
    let (f, s) = (sfc(), st());
    let kept =
        f.c.symbols
            .iter()
            .filter(|sym| sym.path.contains("__SFC_") || sym.private)
            .count();
    eprintln!(
        "{name}: the chart is {} bytes of code and {} of memory, {kept} private variables; its twin {} and {}",
        f.c.code_size, f.c.memory_size, s.c.code_size, s.c.memory_size
    );
    // The program's own variables: declared by it, not the chart's.
    let shared: Vec<String> =
        f.c.symbols
            .iter()
            .filter(|sym| !sym.private && sym.array.is_none())
            .map(|sym| sym.path.clone())
            .collect();
    for path in &shared {
        let twin =
            s.c.symbol(path)
                .unwrap_or_else(|| panic!("{name}: the ST twin has no {path}"));
        assert_eq!(twin.ty, f.c.symbol(path).unwrap().ty, "{name}: {path}");
    }
    for (a, b, _) in &pairs {
        assert!(f.c.symbol(a).is_some(), "{name}: the chart has no {a}");
        assert!(s.c.symbol(b).is_some(), "{name}: the twin has no {b}");
    }
    assert!(shared.len() >= 3, "{name}: something to compare");
    let mut values = 0usize;
    let mut moved = 0usize;
    for seed in 1..=SEQUENCES {
        let (mut f, mut s) = (sfc(), st());
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let mut now = 0i64;
        let mut last: Vec<Value> = Vec::new();
        for scan in 0..SCANS {
            for input in inputs {
                let v = Value::Bool(rng.one_in(odds));
                f.set(input, v);
                s.set(input, v);
            }
            // The first scan at the clock's zero, as a runtime's first scan after RUN: an
            // initial step's time counts from there (its initialization), a TON's from its
            // first call.
            f.scan_at(now);
            s.scan_at(now);
            now += 1 + (rng.next() % 15) as i64;
            for path in &shared {
                assert_eq!(
                    f.get(path),
                    s.get(path),
                    "{name}: {path} differs after scan {} of sequence {seed}",
                    scan + 1
                );
                values += 1;
            }
            let mut flags = Vec::new();
            for (a, b, when) in &pairs {
                if let Some(w) = when {
                    if !s.bool(w) {
                        continue;
                    }
                }
                assert_eq!(
                    f.get(a),
                    s.get(b),
                    "{name}: {a} differs from the twin's {b} after scan {} of sequence {seed}",
                    scan + 1
                );
                if a.ends_with(".X") {
                    flags.push(f.get(a));
                }
                values += 1;
            }
            if flags != last {
                moved += 1;
            }
            last = flags;
        }
    }
    // The inputs moved the charts: a step became active or inactive in many scans.
    assert!(
        moved > SEQUENCES as usize * SCANS / 10,
        "{name}: steps moved in only {moved} scans"
    );
    eprintln!(
        "{name}: {} variables and {} pairs, {values} values equal; steps moved in {moved} scans",
        shared.len(),
        pairs.len()
    );
}

fn sfc_run(pous: Vec<Pou>, st: Option<&'static str>) -> impl Fn() -> Run {
    let doc = project(&pous);
    move || {
        Run::of(compile_sources(
            &Sources {
                st: st.map(|s| ("main.st", s)),
                tc6: &[("chart.xml", doc.as_str())],
            },
            &TargetProfile::pc(),
        ))
    }
}

fn st_run(src: &'static str) -> impl Fn() -> Run {
    move || Run::of(compile(src, &TargetProfile::pc()))
}

#[test]
fn a_sequence_with_selections_and_qualifiers_equals_its_st_twin() {
    // Idle -start-> Fill; Fill chooses, leftmost first: a to Heat, b to Mix, or after 50 ms
    // straight to Done (a skip); Heat and Mix converge to Done; Done -stop-> back to Idle by a
    // jump. Fill: N fill, S heat, P Count. Heat: L lamp 30 ms, D horn 20 ms. Mix: DS mix 25 ms,
    // N an inline Stir. Done: R heat, R mix, P1 chime.
    let chart = Pou::program("Main")
        .sfc()
        .vars(&[
            ("start", "BOOL"),
            ("a", "BOOL"),
            ("b", "BOOL"),
            ("c", "BOOL"),
            ("stop", "BOOL"),
            ("fill", "BOOL"),
            ("heat", "BOOL"),
            ("lamp", "BOOL"),
            ("horn", "BOOL"),
            ("mix", "BOOL"),
            ("chime", "BOOL"),
            ("n", "INT"),
            ("stirs", "INT"),
        ])
        .action("Count", "n := n + 1;")
        .step(1, 100, 0, "Idle", true, &[])
        .trans(2, 100, 30, "start", &["1"])
        .step(3, 100, 60, "Filling", false, &["2"])
        .sel_div(4, 100, 90, &["3"])
        .trans(5, 0, 110, "a", &["4"])
        .trans(6, 100, 110, "b", &["4"])
        .trans(7, 200, 110, "Filling.T >= T#50ms", &["4"])
        .step(8, 0, 140, "Heating", false, &["5"])
        .trans(9, 0, 170, "Heating.T >= T#40ms OR c", &["8"])
        .step(10, 100, 140, "Mixing", false, &["6"])
        .trans(11, 100, 170, "NOT b", &["10"])
        .sel_conv(12, 100, 200, &["9", "11", "7"])
        .step(13, 100, 230, "Done", false, &["12"])
        .trans(14, 100, 260, "stop", &["13"])
        .jump_step(15, 100, 290, "Idle", &["14"])
        .actions_of(
            20,
            300,
            60,
            "3",
            &[
                (21, "N", None, Act::Name("fill")),
                (22, "S", None, Act::Name("heat")),
                (23, "P", None, Act::Name("Count")),
            ],
        )
        .actions_of(
            24,
            300,
            140,
            "8",
            &[
                (25, "L", Some("T#30ms"), Act::Name("lamp")),
                (26, "D", Some("T#20ms"), Act::Name("horn")),
            ],
        )
        .actions_of(
            27,
            300,
            150,
            "10",
            &[
                (28, "DS", Some("T#25ms"), Act::Name("mix")),
                (29, "N", None, Act::St("stirs := stirs + 1;")),
            ],
        )
        .actions_of(
            30,
            300,
            230,
            "13",
            &[
                (31, "R", None, Act::Name("heat")),
                (32, "R", None, Act::Name("mix")),
                (33, "P1", None, Act::Name("chime")),
            ],
        );
    const TWIN: &str = "
PROGRAM Main
VAR
    start, a, b, c, stop : BOOL;
    fill, heat, lamp, horn, mix, chime : BOOL;
    n, stirs : INT;
    x_Idle : BOOL := TRUE;
    x_Filling, x_Heating, x_Mixing, x_Done : BOOL;
    t_Idle, t_Filling, t_Heating, t_Mixing, t_Done : TON;
    heat_S_FF : RS;
    lamp_L_TMR, horn_D_TMR, mix_DS_TMR : TON;
    mix_DS_FF : RS;
    count_P_TRIG, chime_P1_TRIG : R_TRIG;
    q_count, q_stir : BOOL;
    e_2, c_2, e_5, c_5, e_6, c_6, e_7, c_7, e_9, c_9, e_11, c_11, e_14, c_14 : BOOL;
    f2, f5, f6, f7, f9, f11, f14 : BOOL;
END_VAR
    t_Idle(IN := x_Idle, PT := T#1h);
    t_Filling(IN := x_Filling, PT := T#1h);
    t_Heating(IN := x_Heating, PT := T#1h);
    t_Mixing(IN := x_Mixing, PT := T#1h);
    t_Done(IN := x_Done, PT := T#1h);
    (* Figure 23 b), the parts each action uses. *)
    fill := x_Filling;
    heat_S_FF(S := x_Filling, R1 := x_Done);
    heat := NOT x_Done AND heat_S_FF.Q1;
    count_P_TRIG(CLK := x_Filling);
    q_count := count_P_TRIG.Q;
    IF q_count THEN n := n + 1; END_IF;
    lamp_L_TMR(IN := x_Heating, PT := SEL(x_Heating, T#0s, T#30ms));
    lamp := x_Heating AND NOT lamp_L_TMR.Q;
    horn_D_TMR(IN := x_Heating, PT := SEL(x_Heating, T#0s, T#20ms));
    horn := horn_D_TMR.Q;
    mix_DS_TMR(IN := x_Mixing, PT := SEL(x_Mixing, T#0s, T#25ms));
    mix_DS_FF(S := mix_DS_TMR.Q, R1 := x_Done);
    mix := NOT x_Done AND mix_DS_FF.Q1;
    q_stir := x_Mixing;
    IF q_stir THEN stirs := stirs + 1; END_IF;
    chime_P1_TRIG(CLK := x_Done);
    chime := chime_P1_TRIG.Q;
    (* Transitions, each evaluated where it is enabled. *)
    e_2 := x_Idle; IF e_2 THEN c_2 := start; END_IF;
    e_5 := x_Filling; IF e_5 THEN c_5 := a; END_IF;
    e_6 := x_Filling; IF e_6 THEN c_6 := b; END_IF;
    e_7 := x_Filling; IF e_7 THEN c_7 := t_Filling.ET >= T#50ms; END_IF;
    e_9 := x_Heating; IF e_9 THEN c_9 := t_Heating.ET >= T#40ms OR c; END_IF;
    e_11 := x_Mixing; IF e_11 THEN c_11 := NOT b; END_IF;
    e_14 := x_Done; IF e_14 THEN c_14 := stop; END_IF;
    (* Clearing: left to right in Fill's selection. *)
    f2 := e_2 AND c_2;
    f5 := e_5 AND c_5;
    f6 := e_6 AND c_6 AND NOT c_5;
    f7 := e_7 AND c_7 AND NOT c_5 AND NOT c_6;
    f9 := e_9 AND c_9;
    f11 := e_11 AND c_11;
    f14 := e_14 AND c_14;
    IF f2 THEN x_Idle := FALSE; END_IF;
    IF f5 OR f6 OR f7 THEN x_Filling := FALSE; END_IF;
    IF f9 THEN x_Heating := FALSE; END_IF;
    IF f11 THEN x_Mixing := FALSE; END_IF;
    IF f14 THEN x_Done := FALSE; END_IF;
    IF f14 THEN x_Idle := TRUE; END_IF;
    IF f2 THEN x_Filling := TRUE; END_IF;
    IF f5 THEN x_Heating := TRUE; END_IF;
    IF f6 THEN x_Mixing := TRUE; END_IF;
    IF f7 OR f9 OR f11 THEN x_Done := TRUE; END_IF;
    t_Idle(IN := x_Idle, PT := T#1h);
    t_Filling(IN := x_Filling, PT := T#1h);
    t_Heating(IN := x_Heating, PT := T#1h);
    t_Mixing(IN := x_Mixing, PT := T#1h);
    t_Done(IN := x_Done, PT := T#1h);
END_PROGRAM
";
    let sfc = sfc_run(vec![chart], None);
    let pairs = pairs_of(
        &sfc(),
        "Main",
        "",
        &[
            (1, "Idle"),
            (3, "Filling"),
            (8, "Heating"),
            (10, "Mixing"),
            (13, "Done"),
        ],
        &[
            (21, "fill"),
            (22, "heat"),
            (23, "q_count"),
            (25, "lamp"),
            (26, "horn"),
            (28, "mix"),
            (29, "q_stir"),
            (31, "heat"),
            (32, "mix"),
            (33, "chime"),
        ],
    );
    compare(
        "selections and qualifiers",
        sfc,
        st_run(TWIN),
        &["start", "a", "b", "c", "stop"],
        pairs,
        3,
    );
}

#[test]
fn simultaneous_sequences_equal_their_st_twin() {
    // Start -go-> A1 and B1 at once (4a); A1 -ADone-> A2; B1 -x-> B2; A2 and B2 join (4c), then
    // a selection after the join (4d), by number: y (2) to Join, z (1) back to Start. Join
    // -Join.T >= 10 ms, a named transition negated-> back to Start. A1 and B1 share the action
    // busy (N in both); A2 has an inline P1; B2 an L lamp of 15 ms.
    let chart = Pou::program("Main")
        .sfc()
        .vars(&[
            ("go", "BOOL"),
            ("p", "BOOL"),
            ("x", "BOOL"),
            ("y", "BOOL"),
            ("z", "BOOL"),
            ("busy", "BOOL"),
            ("lamp", "BOOL"),
            ("count_a", "INT"),
        ])
        .transition("ADone", "p AND A1.T >= T#5ms")
        .transition("Young", "Join.T < T#10ms")
        .step(1, 100, 0, "Start", true, &[])
        .trans(2, 100, 30, "go", &["1"])
        .sim_div(3, 100, 50, &["2"])
        .step(4, 50, 70, "A1", false, &["3"])
        .trans_ref(5, 50, 100, "ADone", &["4"])
        .step(6, 50, 130, "A2", false, &["5"])
        .step(7, 150, 70, "B1", false, &["3"])
        .trans(8, 150, 100, "x", &["7"])
        .step(9, 150, 130, "B2", false, &["8"])
        .sim_conv(10, 100, 160, &["6", "9"])
        .sel_div(11, 100, 170, &["10"])
        .trans_prio(12, 50, 190, 2, "y", &["11"])
        .trans_prio(13, 150, 190, 1, "z", &["11"])
        .jump_step(14, 150, 220, "Start", &["13"])
        .step(15, 50, 220, "Join", false, &["12"])
        .raw("<transition localId=\"16\"><position x=\"50\" y=\"250\"/><connectionPointIn><connection refLocalId=\"15\"/></connectionPointIn><connectionPointOut/><condition negated=\"true\"><reference name=\"Young\"/></condition></transition>")
        .jump_step(17, 50, 280, "Start", &["16"])
        .actions_of(20, 300, 70, "4", &[(21, "N", None, Act::Name("busy"))])
        .actions_of(22, 300, 75, "7", &[(23, "N", None, Act::Name("busy"))])
        .actions_of(24, 300, 130, "6", &[(25, "P1", None, Act::St("count_a := count_a + 1;"))])
        .actions_of(26, 300, 135, "9", &[(27, "L", Some("T#15ms"), Act::Name("lamp"))]);
    const TWIN: &str = "
PROGRAM Main
VAR
    go, p, x, y, z, busy, lamp : BOOL;
    count_a : INT;
    x_Start : BOOL := TRUE;
    x_A1, x_A2, x_B1, x_B2, x_Join : BOOL;
    t_Start, t_A1, t_A2, t_B1, t_B2, t_Join : TON;
    a2_P1_TRIG : R_TRIG;
    lamp_L_TMR : TON;
    q_a2 : BOOL;
    e_2, c_2, e_5, c_5, e_8, c_8, e_12, c_12, e_13, c_13, e_16, c_16 : BOOL;
    f2, f5, f8, f12, f13, f16 : BOOL;
END_VAR
    t_Start(IN := x_Start, PT := T#1h);
    t_A1(IN := x_A1, PT := T#1h);
    t_A2(IN := x_A2, PT := T#1h);
    t_B1(IN := x_B1, PT := T#1h);
    t_B2(IN := x_B2, PT := T#1h);
    t_Join(IN := x_Join, PT := T#1h);
    busy := x_A1 OR x_B1;
    a2_P1_TRIG(CLK := x_A2);
    q_a2 := a2_P1_TRIG.Q;
    IF q_a2 THEN count_a := count_a + 1; END_IF;
    lamp_L_TMR(IN := x_B2, PT := SEL(x_B2, T#0s, T#15ms));
    lamp := x_B2 AND NOT lamp_L_TMR.Q;
    e_2 := x_Start; IF e_2 THEN c_2 := go; END_IF;
    e_5 := x_A1; IF e_5 THEN c_5 := p AND t_A1.ET >= T#5ms; END_IF;
    e_8 := x_B1; IF e_8 THEN c_8 := x; END_IF;
    e_12 := x_A2 AND x_B2; IF e_12 THEN c_12 := y; END_IF;
    e_13 := x_A2 AND x_B2; IF e_13 THEN c_13 := z; END_IF;
    e_16 := x_Join; IF e_16 THEN c_16 := NOT (t_Join.ET < T#10ms); END_IF;
    f2 := e_2 AND c_2;
    f5 := e_5 AND c_5;
    f8 := e_8 AND c_8;
    f13 := e_13 AND c_13;
    f12 := e_12 AND c_12 AND NOT c_13;
    f16 := e_16 AND c_16;
    IF f2 THEN x_Start := FALSE; END_IF;
    IF f5 THEN x_A1 := FALSE; END_IF;
    IF f8 THEN x_B1 := FALSE; END_IF;
    IF f12 OR f13 THEN x_A2 := FALSE; x_B2 := FALSE; END_IF;
    IF f16 THEN x_Join := FALSE; END_IF;
    IF f2 THEN x_A1 := TRUE; x_B1 := TRUE; END_IF;
    IF f5 THEN x_A2 := TRUE; END_IF;
    IF f8 THEN x_B2 := TRUE; END_IF;
    IF f12 THEN x_Join := TRUE; END_IF;
    IF f13 OR f16 THEN x_Start := TRUE; END_IF;
    t_Start(IN := x_Start, PT := T#1h);
    t_A1(IN := x_A1, PT := T#1h);
    t_A2(IN := x_A2, PT := T#1h);
    t_B1(IN := x_B1, PT := T#1h);
    t_B2(IN := x_B2, PT := T#1h);
    t_Join(IN := x_Join, PT := T#1h);
END_PROGRAM
";
    let sfc = sfc_run(vec![chart], None);
    let pairs = pairs_of(
        &sfc(),
        "Main",
        "",
        &[
            (1, "Start"),
            (4, "A1"),
            (6, "A2"),
            (7, "B1"),
            (9, "B2"),
            (15, "Join"),
        ],
        &[(21, "busy"), (23, "busy"), (25, "q_a2"), (27, "lamp")],
    );
    compare(
        "simultaneous sequences",
        sfc,
        st_run(TWIN),
        &["go", "p", "x", "y", "z"],
        pairs,
        3,
    );
}

#[test]
fn a_chart_in_a_function_block_called_from_st_equals_its_st_twin() {
    // Ready -t-> Busy (N working, D done after 20 ms) -NOT t OR Busy.T >= 50 ms-> Ready; two
    // instances driven by the program.
    let seq = Pou::function_block("Seq")
        .sfc()
        .inputs(&[("t", "BOOL")])
        .outputs(&[("working", "BOOL"), ("done", "BOOL")])
        .step(1, 100, 0, "Ready", true, &[])
        .sel_conv(5, 100, 50, &["4"])
        .trans(2, 100, 30, "t", &["1"])
        .step(3, 100, 70, "Busy", false, &["2"])
        .trans(4, 100, 100, "NOT t OR Busy.T >= T#50ms", &["3"])
        .jump_step(6, 100, 130, "Ready", &["5"])
        .actions_of(
            7,
            200,
            70,
            "3",
            &[
                (8, "N", None, Act::Name("working")),
                (9, "D", Some("T#20ms"), Act::Name("done")),
            ],
        );
    const MAIN: &str = "
PROGRAM Main
VAR
    u, v : BOOL;
    a, b : Seq;
    wa, wb, da, db : BOOL;
END_VAR
    a(t := u);
    b(t := v);
    wa := a.working; da := a.done;
    wb := b.working; db := b.done;
END_PROGRAM
";
    const TWIN: &str = "
FUNCTION_BLOCK Seq
VAR_INPUT t : BOOL; END_VAR
VAR_OUTPUT working, done : BOOL; END_VAR
VAR
    x_Ready : BOOL := TRUE;
    x_Busy : BOOL;
    t_Ready, t_Busy : TON;
    done_D_TMR : TON;
    e_2, c_2, e_4, c_4 : BOOL;
END_VAR
    t_Ready(IN := x_Ready, PT := T#1h);
    t_Busy(IN := x_Busy, PT := T#1h);
    working := x_Busy;
    done_D_TMR(IN := x_Busy, PT := SEL(x_Busy, T#0s, T#20ms));
    done := done_D_TMR.Q;
    e_2 := x_Ready; IF e_2 THEN c_2 := t; END_IF;
    e_4 := x_Busy; IF e_4 THEN c_4 := NOT t OR t_Busy.ET >= T#50ms; END_IF;
    IF e_2 AND c_2 THEN x_Ready := FALSE; x_Busy := TRUE;
    ELSIF e_4 AND c_4 THEN x_Busy := FALSE; x_Ready := TRUE; END_IF;
    t_Ready(IN := x_Ready, PT := T#1h);
    t_Busy(IN := x_Busy, PT := T#1h);
END_FUNCTION_BLOCK

PROGRAM Main
VAR
    u, v : BOOL;
    a, b : Seq;
    wa, wb, da, db : BOOL;
END_VAR
    a(t := u);
    b(t := v);
    wa := a.working; da := a.done;
    wb := b.working; db := b.done;
END_PROGRAM
";
    let sfc = sfc_run(vec![seq], Some(MAIN));
    let mut pairs = Vec::new();
    for inst in ["a.", "b."] {
        pairs.extend(pairs_of(
            &sfc(),
            "Seq",
            inst,
            &[(1, "Ready"), (3, "Busy")],
            &[(8, "working"), (9, "done")],
        ));
    }
    compare(
        "a chart in a function block",
        sfc,
        st_run(TWIN),
        &["u", "v"],
        pairs,
        2,
    );
}
