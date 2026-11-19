//! Version agreement: the gate that makes the versioning pipeline unable to rot silently.
//!
//! Nothing in this repository asserted a version before this test. That is how the pipeline came
//! to compute a tag, write it nowhere, and hardcode `BASE_VERSION="0.1.0"`: a workflow that always
//! succeeds while publishing nothing looks exactly like a working workflow in the Actions list.
//!
//! The rule enforced here:
//!   1. every manifest that carries a version agrees with every other one, and
//!   2. the manifests agree with the BASE version of the latest release tag.
//!
//! (2) compares against the tag's base, not its full string, because `v0.2.0-rc.4` is a build OF
//! 0.2.0: the manifests carry 0.2.0 and the prerelease identifier belongs to the build, not to the
//! artifact. A test that demanded `0.2.0-rc.4` in package.json would make a released artifact's
//! version depend on which build number happened to be pushed.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root resolves from CARGO_MANIFEST_DIR")
}

/// Every frontend manifest that carries a version. Directories are listed explicitly by pattern
/// but each hit is checked to exist, and the caller asserts a minimum count: a glob that silently
/// matches nothing is the same failure mode as a check that silently does not run.
fn frontend_manifests(root: &Path) -> Vec<PathBuf> {
    let mut out = vec![root.join("frontend/package.json")];
    for group in ["frontend/packages"] {
        let dir = root.join(group);
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let candidate = entry.path().join("package.json");
            if candidate.is_file() {
                out.push(candidate);
            }
        }
    }
    out.sort();
    out
}

fn manifest_json(path: &Path) -> serde_json::Value {
    let text =
        fs::read_to_string(path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("{} is not valid JSON: {e}", path.display()))
}

/// The module crate a frontend package is the UI of (`"mythos": {"crate": "plugins/hmi"}`), if
/// it is one. A module's UI carries the module's version, not the platform's (ADR-011 §2.6).
fn module_crate_of(path: &Path) -> Option<String> {
    manifest_json(path)["mythos"]["crate"]
        .as_str()
        .map(str::to_string)
}

/// `version = "…"` in the `[package]` section of a Cargo.toml.
fn crate_version(cargo_toml: &Path) -> String {
    let text = fs::read_to_string(cargo_toml)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", cargo_toml.display()));
    let mut in_package = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_package = line == "[package]";
        } else if in_package {
            if let Some(rest) = line.strip_prefix("version") {
                if let Some(value) = rest.trim().strip_prefix('=') {
                    return value.trim().trim_matches('"').to_string();
                }
            }
        }
    }
    panic!(
        "{} has no [package] version of its own",
        cargo_toml.display()
    )
}

fn manifest_version(path: &Path) -> String {
    let json = manifest_json(path);
    json.get("version")
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| panic!("{} has no string \"version\" field", path.display()))
        .to_string()
}

#[test]
fn every_manifest_agrees_with_the_crate_version() {
    let root = workspace_root();
    let crate_version = env!("CARGO_PKG_VERSION");
    let manifests = frontend_manifests(&root);

    // The workspace root and the workbench package. A lower count means the discovery above
    // matched nothing, which would make every assertion below vacuous.
    assert!(
        manifests.len() >= 2,
        "expected at least 2 frontend manifests, found {}: {:?}",
        manifests.len(),
        manifests
    );

    let mut mismatches = Vec::new();
    // A module's UI follows its module, checked below.
    for path in manifests.iter().filter(|p| module_crate_of(p).is_none()) {
        let found = manifest_version(path);
        if found != crate_version {
            let rel = path.strip_prefix(&root).unwrap_or(path);
            mismatches.push(format!(
                "{}: {} (Cargo.toml [workspace.package] says {})",
                rel.display(),
                found,
                crate_version
            ));
        }
    }

    assert!(
        mismatches.is_empty(),
        "version drift between the Rust workspace and the frontend manifests:\n  {}\n\
         Fix by running scripts/version.sh --stamp, not by hand-editing one file.",
        mismatches.join("\n  ")
    );
}

/// A module's workbench UI and its executable are one module: they carry one version, the
/// module's own, which is not the platform's (ADR-011 §2.6).
#[test]
fn a_module_ui_carries_its_module_version() {
    let root = workspace_root();
    let uis: Vec<(PathBuf, String)> = frontend_manifests(&root)
        .into_iter()
        .filter_map(|p| module_crate_of(&p).map(|c| (p, c)))
        .collect();
    assert!(
        uis.iter().any(|(_, c)| c == "plugins/hmi"),
        "the HMI's UI was not found among the module UIs: {uis:?}"
    );
    for (ui, module) in &uis {
        let expected = crate_version(&root.join(module).join("Cargo.toml"));
        assert_eq!(
            manifest_version(ui),
            expected,
            "{} is the UI of {module}, which is version {expected}",
            ui.strip_prefix(&root).unwrap_or(ui).display()
        );
    }
}

#[test]
fn manifests_agree_with_the_latest_release_tag() {
    let root = workspace_root();
    let output = Command::new("git")
        .arg("-C")
        .arg(&root)
        .args(["describe", "--tags", "--abbrev=0"])
        .output();

    let tag = match output {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).trim().to_string(),
        _ => String::new(),
    };

    if tag.is_empty() {
        // Shallow checkout (CI's default) or no tag exists yet. Say so rather than pass quietly:
        // a skipped check and a passing check must not look the same in the log.
        eprintln!(
            "SKIP manifests_agree_with_the_latest_release_tag: no reachable tag in this checkout \
             (shallow clone, or the first tag has not been created yet). \
             the agreement test above still ran."
        );
        return;
    }

    let tag_body = tag.trim_start_matches('v');
    let tag_base = tag_body
        .split('-')
        .next()
        .unwrap_or(tag_body)
        .split('+')
        .next()
        .unwrap_or(tag_body);
    let crate_version = env!("CARGO_PKG_VERSION");

    assert_eq!(
        tag_base, crate_version,
        "latest tag {tag} describes base version {tag_base}, but the manifests say {crate_version}. \
         A tag and the manifests it claims to describe must not disagree."
    );
}
