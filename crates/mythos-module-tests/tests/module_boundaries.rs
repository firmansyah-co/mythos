//! The boundaries that keep modules modules (ADR-011 §2.6), checked from the real dependency
//! graph (`cargo metadata`), so a module can be moved into a repository of its own on the day
//! that is wanted, and the core can run without any of them:
//!
//! 1. A module (a crate under `plugins/`) never depends on the core (`mythos-core`): it speaks
//!    the protocol (`mythos-module`), nothing else of the server.
//! 2. A module never depends on another module.
//! 3. Nothing of the platform depends on a module, except this test crate and dev-dependencies:
//!    the server, the CLI and the updater run with none.
//! 4. A module carries its own version, not the platform's (`version.workspace = true`).
//!
//! Normal and build dependencies count, transitively; dev-dependencies do not (tests may run a
//! module beside the server).

use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// The crates of the workspace and their normal and build dependencies, by package name.
struct Graph {
    /// name -> manifest path
    packages: BTreeMap<String, PathBuf>,
    /// name -> names it depends on (normal and build only), workspace and external alike
    deps: BTreeMap<String, BTreeSet<String>>,
}

impl Graph {
    /// Read `cargo metadata` output.
    fn from_metadata(metadata: &Value) -> Graph {
        let by_id: BTreeMap<&str, &Value> = metadata["packages"]
            .as_array()
            .expect("packages")
            .iter()
            .map(|p| (p["id"].as_str().unwrap(), p))
            .collect();
        let members: BTreeSet<&str> = metadata["workspace_members"]
            .as_array()
            .expect("workspace_members")
            .iter()
            .map(|m| m.as_str().unwrap())
            .collect();
        let name = |id: &str| by_id[id]["name"].as_str().unwrap().to_string();
        let packages = members
            .iter()
            .map(|id| {
                (
                    name(id),
                    PathBuf::from(by_id[id]["manifest_path"].as_str().unwrap()),
                )
            })
            .collect();
        let mut deps = BTreeMap::new();
        for node in metadata["resolve"]["nodes"].as_array().expect("resolve") {
            let from = name(node["id"].as_str().unwrap());
            let to: BTreeSet<String> = node["deps"]
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
                .map(|d| name(d["pkg"].as_str().unwrap()))
                .collect();
            deps.insert(from, to);
        }
        Graph { packages, deps }
    }

    /// Everything `from` reaches, with the path to each, through normal and build dependencies.
    fn reach(&self, from: &str) -> BTreeMap<String, Vec<String>> {
        let mut seen: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut queue = vec![(from.to_string(), vec![from.to_string()])];
        while let Some((at, path)) = queue.pop() {
            for next in self.deps.get(&at).into_iter().flatten() {
                if seen.contains_key(next) {
                    continue;
                }
                let mut path = path.clone();
                path.push(next.clone());
                seen.insert(next.clone(), path.clone());
                queue.push((next.clone(), path));
            }
        }
        seen
    }
}

/// The crates that are modules: the workspace members under `plugins/`.
fn modules(graph: &Graph, root: &Path) -> BTreeSet<String> {
    graph
        .packages
        .iter()
        .filter(|(_, manifest)| manifest.starts_with(root.join("plugins")))
        .map(|(name, _)| name.clone())
        .collect()
}

/// Platform crates that may depend on a module: only this one, which runs them in its tests.
const MAY_DEPEND_ON_MODULES: &[&str] = &["mythos-module-tests"];
const CORE: &str = "mythos-core";

/// Every broken boundary, each naming the path that breaks it.
fn violations(graph: &Graph, root: &Path) -> Vec<String> {
    let modules = modules(graph, root);
    let mut found = Vec::new();
    for module in &modules {
        for (reached, path) in graph.reach(module) {
            if reached == CORE {
                found.push(format!(
                    "module {module} depends on the core: {}",
                    path.join(" -> ")
                ));
            } else if modules.contains(&reached) {
                found.push(format!(
                    "module {module} depends on module {reached}: {}",
                    path.join(" -> ")
                ));
            }
        }
    }
    for platform in graph
        .packages
        .keys()
        .filter(|p| !modules.contains(*p) && !MAY_DEPEND_ON_MODULES.contains(&p.as_str()))
    {
        for (reached, path) in graph.reach(platform) {
            if modules.contains(&reached) {
                found.push(format!(
                    "{platform} depends on module {reached}: {}",
                    path.join(" -> ")
                ));
            }
        }
    }
    for module in &modules {
        let manifest = &graph.packages[module];
        let text = std::fs::read_to_string(manifest).unwrap_or_default();
        if inherits_workspace_version(&text) {
            found.push(format!(
                "module {module} takes the platform's version ({}): give it its own `version = \"…\"`",
                manifest.display()
            ));
        }
    }
    found
}

/// Whether a Cargo.toml's `[package]` says `version.workspace = true`.
fn inherits_workspace_version(cargo_toml: &str) -> bool {
    let mut in_package = false;
    for line in cargo_toml.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_package = line == "[package]";
        } else if in_package {
            let compact: String = line.chars().filter(|c| !c.is_whitespace()).collect();
            if compact.starts_with("version.workspace=true")
                || compact.starts_with("version={workspace=true")
            {
                return true;
            }
        }
    }
    false
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("the workspace root")
        .to_path_buf()
}

fn metadata(root: &Path) -> Value {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let out = std::process::Command::new(cargo)
        .current_dir(root)
        .args(["metadata", "--format-version", "1", "--locked"])
        .output()
        .expect("cargo metadata runs");
    assert!(
        out.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("cargo metadata prints JSON")
}

#[test]
fn modules_stay_modules() {
    let root = workspace_root();
    let graph = Graph::from_metadata(&metadata(&root));

    // The check means nothing if it found nothing to check.
    let modules = modules(&graph, &root);
    for expected in ["mythos-hmi", "driver-modbus", "mythos-data"] {
        assert!(
            modules.contains(expected),
            "{expected} is not among the modules found: {modules:?}"
        );
    }
    assert!(graph.packages.contains_key(CORE), "the core was not found");

    let found = violations(&graph, &root);
    assert!(
        found.is_empty(),
        "module boundaries broken (ADR-011 §2.6):\n  {}",
        found.join("\n  ")
    );
}

/// The guard itself: fed a graph that breaks each rule, it names each break.
#[test]
fn the_guard_names_every_kind_of_break() {
    let root = std::env::temp_dir().join("mythos-boundary-fixture");
    let plugin = |name: &str| root.join("plugins").join(name).join("Cargo.toml");
    std::fs::create_dir_all(plugin("a").parent().unwrap()).unwrap();
    std::fs::create_dir_all(plugin("b").parent().unwrap()).unwrap();
    std::fs::write(
        plugin("a"),
        "[package]\nname = \"a\"\nversion.workspace = true\n",
    )
    .unwrap();
    std::fs::write(
        plugin("b"),
        "[package]\nname = \"b\"\nversion = \"2.0.0\"\n",
    )
    .unwrap();

    let pkg = |id: &str, manifest: PathBuf| serde_json::json!({"id": id, "name": id, "manifest_path": manifest});
    let dep = |id: &str, kind: Option<&str>| serde_json::json!({"pkg": id, "dep_kinds": [{"kind": kind}]});
    let metadata = serde_json::json!({
        "packages": [
            pkg("a", plugin("a")), pkg("b", plugin("b")),
            pkg("mythos-core", root.join("crates/core/Cargo.toml")),
            pkg("mythos-cli", root.join("crates/cli/Cargo.toml")),
            pkg("helper", root.join("crates/helper/Cargo.toml")),
            pkg("mythos-module-tests", root.join("crates/tests/Cargo.toml")),
        ],
        "workspace_members": ["a", "b", "mythos-core", "mythos-cli", "helper", "mythos-module-tests"],
        "resolve": {"nodes": [
            // a reaches the core through a helper, and depends on module b.
            {"id": "a", "deps": [dep("helper", None), dep("b", None)]},
            {"id": "helper", "deps": [dep("mythos-core", None)]},
            // b depends on the core only to test: allowed.
            {"id": "b", "deps": [dep("mythos-core", Some("dev"))]},
            // The CLI builds a module in: refused. Its test use of b would be allowed.
            {"id": "mythos-cli", "deps": [dep("b", Some("build"))]},
            {"id": "mythos-core", "deps": [dep("a", Some("dev"))]},
            {"id": "mythos-module-tests", "deps": [dep("a", None), dep("b", None)]},
        ]}
    });
    let graph = Graph::from_metadata(&metadata);
    let mut found = violations(&graph, &root);
    found.sort();
    assert_eq!(
        found.len(),
        4,
        "expected exactly four breaks:\n  {}",
        found.join("\n  ")
    );
    assert!(
        found
            .iter()
            .any(|f| f.starts_with("module a depends on the core: a -> helper -> mythos-core")),
        "{found:?}"
    );
    assert!(
        found
            .iter()
            .any(|f| f.starts_with("module a depends on module b: a -> b")),
        "{found:?}"
    );
    assert!(
        found
            .iter()
            .any(|f| f.starts_with("mythos-cli depends on module b")),
        "{found:?}"
    );
    assert!(
        found
            .iter()
            .any(|f| f.starts_with("module a takes the platform's version")),
        "{found:?}"
    );
    // helper reaches the core, not a module, and is a platform crate: no break of its own.
    assert!(!found.iter().any(|f| f.starts_with("helper ")), "{found:?}");
}

#[test]
fn a_version_is_inherited_only_when_the_package_section_says_so() {
    assert!(inherits_workspace_version(
        "[package]\nversion.workspace = true\n"
    ));
    assert!(inherits_workspace_version(
        "[package]\nversion = { workspace = true }\n"
    ));
    assert!(!inherits_workspace_version(
        "[package]\nversion = \"1.2.0\"\n"
    ));
    // Another section's `version.workspace` is not the package's.
    assert!(!inherits_workspace_version(
        "[package]\nversion = \"1.2.0\"\n[dependencies.x]\nversion.workspace = true\n"
    ));
}
