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
    for group in ["frontend/packages", "frontend/plugins"] {
        let dir = root.join(group);
        let Ok(entries) = fs::read_dir(&dir) else { continue };
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

fn manifest_version(path: &Path) -> String {
    let text = fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    let json: serde_json::Value = serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("{} is not valid JSON: {e}", path.display()));
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

    assert!(
        manifests.len() >= 7,
        "expected at least 7 frontend manifests, found {}: {:?}",
        manifests.len(),
        manifests
    );

    let mut mismatches = Vec::new();
    for path in &manifests {
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
