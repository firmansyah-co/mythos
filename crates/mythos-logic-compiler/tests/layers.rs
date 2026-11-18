//! The layers that keep control logic modular (ADR-013 §2.1), checked from the real dependency
//! graph (`cargo metadata`), so that a language can be added without touching a target and a
//! target without touching a language:
//!
//! - the IR depends on nothing of the workspace: no language, no backend, no VM;
//! - the VM depends on nothing at all, so it builds for a microcontroller with `core` alone;
//! - a front-end (a language) depends on the IR only; a graphical one (Ladder Diagram) also
//!   on the Structured Text front-end, whose declarations, types, literals, text and
//!   standard library it shares (ADR-013 §2.15), and never on a backend or the VM;
//! - a backend depends on the IR and on its own target only.
//!
//! Every `mythos-logic-*` crate must be listed here with its layer; a new one that is not is a
//! failure, so nobody adds one without deciding where it sits.

use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// Each logic crate and the workspace crates it may depend on (normal dependencies, direct or
/// not). `EXTERNAL_FREE` crates may have no dependency at all.
const LAYERS: &[(&str, &[&str])] = &[
    ("mythos-logic-ir", &[]),
    ("mythos-logic-vm", &[]),
    ("mythos-logic-st", &["mythos-logic-ir"]),
    ("mythos-logic-ld", &["mythos-logic-ir", "mythos-logic-st"]),
    (
        "mythos-logic-backend-vm",
        &["mythos-logic-ir", "mythos-logic-vm"],
    ),
    (
        "mythos-logic-compiler",
        &[
            "mythos-logic-ir",
            "mythos-logic-st",
            "mythos-logic-ld",
            "mythos-logic-backend-vm",
            "mythos-logic-vm",
        ],
    ),
];
const EXTERNAL_FREE: &[&str] = &["mythos-logic-vm"];

/// name -> (workspace crates it reaches, number of direct dependencies of any kind)
type Graph = BTreeMap<String, (BTreeSet<String>, usize)>;

fn graph(metadata: &Value) -> Graph {
    let name_of: BTreeMap<&str, &str> = metadata["packages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| (p["id"].as_str().unwrap(), p["name"].as_str().unwrap()))
        .collect();
    let members: BTreeSet<&str> = metadata["workspace_members"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| name_of[m.as_str().unwrap()])
        .collect();
    let mut direct: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for node in metadata["resolve"]["nodes"].as_array().unwrap() {
        let deps = node["deps"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|d| {
                d["dep_kinds"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|k| k["kind"].is_null() || k["kind"] == "build")
            })
            .map(|d| name_of[d["pkg"].as_str().unwrap()])
            .collect();
        direct.insert(name_of[node["id"].as_str().unwrap()], deps);
    }
    let mut out = Graph::new();
    for m in &members {
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        let mut queue: Vec<&str> = direct.get(m).cloned().unwrap_or_default();
        while let Some(at) = queue.pop() {
            if seen.insert(at) {
                queue.extend(direct.get(at).cloned().unwrap_or_default());
            }
        }
        let reached = seen
            .into_iter()
            .filter(|d| members.contains(d))
            .map(String::from)
            .collect();
        out.insert(m.to_string(), (reached, direct.get(m).map_or(0, Vec::len)));
    }
    out
}

/// Every broken layer, each naming what breaks it.
fn violations(graph: &Graph) -> Vec<String> {
    let mut found = Vec::new();
    for (name, (reached, direct)) in graph {
        if !name.starts_with("mythos-logic-") {
            continue;
        }
        let Some((_, allowed)) = LAYERS.iter().find(|(n, _)| n == name) else {
            found.push(format!(
                "{name} has no layer: list it in LAYERS with what it may depend on"
            ));
            continue;
        };
        for dep in reached {
            if !allowed.contains(&dep.as_str()) {
                found.push(format!(
                    "{name} depends on {dep}, which its layer does not allow"
                ));
            }
        }
        if EXTERNAL_FREE.contains(&name.as_str()) && *direct != 0 {
            found.push(format!(
                "{name} has {direct} dependencies; it must have none to build for bare metal"
            ));
        }
    }
    for (name, _) in LAYERS {
        if !graph.contains_key(*name) {
            found.push(format!(
                "{name} is listed in LAYERS but is not in the workspace"
            ));
        }
    }
    found
}

fn fake(edges: &[(&str, &[&str], usize)]) -> Graph {
    let mut g: Graph = LAYERS
        .iter()
        .map(|(n, allowed)| {
            (
                n.to_string(),
                (
                    allowed.iter().map(|a| a.to_string()).collect(),
                    allowed.len(),
                ),
            )
        })
        .collect();
    g.get_mut("mythos-logic-vm").unwrap().1 = 0;
    for (name, reached, direct) in edges {
        g.insert(
            name.to_string(),
            (reached.iter().map(|r| r.to_string()).collect(), *direct),
        );
    }
    g
}

#[test]
fn the_logic_crates_keep_their_layers() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap();
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let out = std::process::Command::new(cargo)
        .current_dir(root)
        .args(["metadata", "--format-version", "1", "--locked"])
        .output()
        .expect("cargo metadata runs");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let g = graph(&serde_json::from_slice(&out.stdout).unwrap());
    // The check means nothing if it found nothing to check.
    for (name, _) in LAYERS {
        assert!(g.contains_key(*name), "{name} not found in the workspace");
    }
    let v = violations(&g);
    assert!(v.is_empty(), "{}", v.join("\n"));
}

#[test]
fn the_guard_sees_every_kind_of_break() {
    assert!(
        violations(&fake(&[])).is_empty(),
        "the model graph itself is clean"
    );

    let v = violations(&fake(&[("mythos-logic-ir", &["mythos-logic-vm"], 2)]));
    assert_eq!(
        v,
        ["mythos-logic-ir depends on mythos-logic-vm, which its layer does not allow"]
    );

    let v = violations(&fake(&[(
        "mythos-logic-st",
        &["mythos-logic-ir", "mythos-logic-backend-vm"],
        2,
    )]));
    assert_eq!(
        v,
        ["mythos-logic-st depends on mythos-logic-backend-vm, which its layer does not allow"]
    );

    let v = violations(&fake(&[(
        "mythos-logic-backend-vm",
        &["mythos-logic-ir", "mythos-logic-vm", "mythos-logic-st"],
        3,
    )]));
    assert_eq!(
        v,
        ["mythos-logic-backend-vm depends on mythos-logic-st, which its layer does not allow"]
    );

    let v = violations(&fake(&[("mythos-logic-vm", &[], 1)]));
    assert_eq!(
        v,
        ["mythos-logic-vm has 1 dependencies; it must have none to build for bare metal"]
    );

    let v = violations(&fake(&[("mythos-logic-fbd", &["mythos-logic-ir"], 1)]));
    assert_eq!(
        v,
        ["mythos-logic-fbd has no layer: list it in LAYERS with what it may depend on"]
    );

    // A graphical front-end reads ST's common elements, never a target; and ST never
    // depends on a graphical language.
    let v = violations(&fake(&[(
        "mythos-logic-ld",
        &["mythos-logic-ir", "mythos-logic-st", "mythos-logic-vm"],
        3,
    )]));
    assert_eq!(
        v,
        ["mythos-logic-ld depends on mythos-logic-vm, which its layer does not allow"]
    );
    let v = violations(&fake(&[(
        "mythos-logic-st",
        &["mythos-logic-ir", "mythos-logic-ld"],
        2,
    )]));
    assert_eq!(
        v,
        ["mythos-logic-st depends on mythos-logic-ld, which its layer does not allow"]
    );

    let v = violations(&fake(&[("mythos-logic-st", &["mythos-core"], 1)]));
    assert_eq!(
        v,
        ["mythos-logic-st depends on mythos-core, which its layer does not allow"]
    );
}
