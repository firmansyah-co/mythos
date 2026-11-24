//! A project's library packages (ADR-015 step 1, stage P1b): each package file kept whole in
//! `<project>/logic/packages/<id>/<version>.zip`, listed with its SHA-256 and origin in
//! `<project>/logic/packages.json`.
//!
//! A package is kept and listed; **programs are not compiled with it yet** (that is the next
//! stage): `read_project`, what a compile is given, the library check and the builds do not
//! look at packages.
//!
//! The file is hostile until [`mythos_logic_package::read`] has read it: nothing of it reaches
//! the disk, a path or an answer before that, and every bound is the crate's constant. What is
//! answered about a stored package is read again from its file and checked against the SHA-256
//! `packages.json` records, at every read: a file that is missing, changed or no longer reads is
//! said with words and never answered as a package in order.
//!
//! Every write goes through [`write_synced`] or [`remove_stored`] under the module's lock. An add
//! writes the file first and the list second; a remove writes the list first and removes the
//! file second. A file the list does not name is not a package, so a stop between the two
//! writes leaves a state the routes describe truthfully.

use super::{
    about_to_write, block_json, body, error, read_project, reply, write_synced, Refusal,
    PACKAGES_DIR, PACKAGES_FILE,
};
use base64::Engine as _;
use mythos_logic_compiler::{Diagnostic, Sources};
use mythos_logic_package::{
    Dependency, Manifest, Package, MAX_PACKAGE_BASE64_LEN, MAX_PACKAGE_BYTES, MAX_VERSION_LEN,
};
use mythos_logic_st::ast::{Lost, PouKind};
use mythos_logic_st::project::MAX_SOURCES;
use mythos_module::protocol::{HttpCall, HttpReply};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::io::Read as _;
use std::path::{Path, PathBuf};

/// The longest file name an origin keeps, in characters.
pub(crate) const MAX_FILE_NAME_LEN: usize = 255;

/// The extension of a stored package file.
const PACKAGE_EXTENSION: &str = ".zip";

// -----------------------------------------------------------------------------
// What is stored
// -----------------------------------------------------------------------------

/// `packages.json`. Unknown keys are refused, so a file written by a newer module is not read
/// as if it said less than it does.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PackagesFile {
    pub(crate) packages: Vec<PackageEntry>,
}

/// One package of the project: its manifest's id and version, the SHA-256 of its file as it
/// was added, and where it came from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PackageEntry {
    pub(crate) id: String,
    pub(crate) version: String,
    pub(crate) sha256: String,
    pub(crate) origin: Origin,
}

/// Where a package came from. In P1 a file only; its name only when the caller gave one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Origin {
    pub(crate) kind: OriginKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) file_name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum OriginKind {
    File,
}

fn unreadable(why: impl std::fmt::Display) -> Refusal {
    error(
        500,
        format!("logic/{PACKAGES_FILE} cannot be read as the project's packages: {why}"),
    )
}

/// The project's packages as `packages.json` lists them. A missing file is no package; it is
/// never made by a read. Every entry is checked as an add makes one (its id and version by the
/// rules of a package, its SHA-256 64 lowercase hexadecimal digits, its file name as one is
/// taken, no id twice in any case): a file that breaks one is not read at all, so no path is
/// ever built from an id or a version that did not pass.
pub(crate) fn load(data: &Path) -> Result<PackagesFile, Refusal> {
    let file: PackagesFile = match std::fs::read(data.join(PACKAGES_FILE)) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(unreadable)?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(PackagesFile::default()),
        Err(e) => return Err(unreadable(e)),
    };
    for (at, entry) in file.packages.iter().enumerate() {
        if let Some(problem) = mythos_logic_package::id_problem(&entry.id) {
            return Err(unreadable(problem));
        }
        if let Some(problem) = version_problem(&entry.version) {
            return Err(unreadable(format!("package '{}': {problem}", entry.id)));
        }
        let hex = |s: &str| {
            s.len() == 64
                && s.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        };
        if !hex(&entry.sha256) {
            return Err(unreadable(format!(
                "the SHA-256 of package '{}' is not 64 lowercase hexadecimal digits",
                entry.id
            )));
        }
        if let Some(problem) = entry
            .origin
            .file_name
            .as_deref()
            .and_then(file_name_problem)
        {
            return Err(unreadable(format!("package '{}': {problem}", entry.id)));
        }
        if let Some(other) = file.packages[..at]
            .iter()
            .find(|p| p.id.eq_ignore_ascii_case(&entry.id))
        {
            return Err(unreadable(format!(
                "it lists package '{}' and package '{}', one id (ids that differ only in case are one id)",
                other.id, entry.id
            )));
        }
    }
    Ok(file)
}

/// Write `packages.json`, sorted by id in byte order of its lowercase form.
fn save(data: &Path, file: &mut PackagesFile) -> Result<(), Refusal> {
    file.packages.sort_by(|a, b| {
        a.id.to_ascii_lowercase()
            .as_bytes()
            .cmp(b.id.to_ascii_lowercase().as_bytes())
    });
    let bytes = serde_json::to_vec_pretty(&*file).map_err(|e| error(500, e))?;
    write_synced(&data.join(PACKAGES_FILE), &bytes)
}

/// What is wrong with `version` as the version of a stored package, as words; `None` when it is
/// one. The rule of the package reader (`version_problem` of `mythos-logic-package`, which is
/// not public): `MAJOR.MINOR.PATCH` in decimal with no leading zero, optionally `-<pre>` of
/// letters, digits, `.` and `-` with no empty part and no number with a leading zero, no `+`,
/// at most [`MAX_VERSION_LEN`] characters. Applied to what `packages.json` says, so a hand edit
/// cannot make a path of a version the reader would have refused.
fn version_problem(version: &str) -> Option<String> {
    let numeric = |n: &str| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit());
    let no_leading_zero = |n: &str| n.len() == 1 || !n.starts_with('0');
    let (core, pre) = match version.split_once('-') {
        Some((core, pre)) => (core, Some(pre)),
        None => (version, None),
    };
    let numbers: Vec<&str> = core.split('.').collect();
    let core_ok = numbers.len() == 3
        && numbers
            .iter()
            .all(|n| numeric(n) && no_leading_zero(n) && n.parse::<u64>().is_ok());
    let pre_ok = pre.is_none_or(|pre| {
        pre.split('.').all(|part| {
            !part.is_empty()
                && part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
                && (!numeric(part) || no_leading_zero(part))
        })
    });
    if version.chars().count() <= MAX_VERSION_LEN && core_ok && pre_ok {
        return None;
    }
    Some(format!(
        "'{}' is not a package version: MAJOR.MINOR.PATCH, optionally -<pre>, at most {MAX_VERSION_LEN} characters",
        version.escape_debug()
    ))
}

/// What is wrong with `name` as the file name of an origin; `None` when it is one: 1 to
/// [`MAX_FILE_NAME_LEN`] characters, no control character. It is kept as the caller gave it
/// and never used as a path.
fn file_name_problem(name: &str) -> Option<String> {
    let length = name.chars().count();
    if length == 0 {
        return Some("file_name is empty: leave it out when there is none".into());
    }
    if length > MAX_FILE_NAME_LEN {
        return Some(format!(
            "file_name is at most {MAX_FILE_NAME_LEN} characters; this one is {length}"
        ));
    }
    if name.chars().any(char::is_control) {
        return Some("file_name holds a control character".into());
    }
    None
}

/// The directory and the file name of a stored package, in `logic/packages/`: its id and its
/// version, each lower-cased, the version followed by `.zip`. Only an id and a version that
/// passed the rules of a package get here ([`mythos_logic_package::read`] or [`load`]), so
/// neither holds a separator, a `.` or `..` segment or a device name of Windows; lower-cased, ids
/// that differ only in case are one directory on every file system.
fn stored_names(id: &str, version: &str) -> (String, String) {
    (
        id.to_ascii_lowercase(),
        format!("{}{PACKAGE_EXTENSION}", version.to_ascii_lowercase()),
    )
}

fn stored_path(data: &Path, id: &str, version: &str) -> PathBuf {
    let (dir, file) = stored_names(id, version);
    data.join(PACKAGES_DIR).join(dir).join(file)
}

/// The stored file as the answers name it, from the project's directory.
fn shown_path(id: &str, version: &str) -> String {
    let (dir, file) = stored_names(id, version);
    format!("logic/{PACKAGES_DIR}/{dir}/{file}")
}

/// A listed package's file, read again: the package, or the words of why it is not one in
/// order (missing, unreadable, not a package any more, not the SHA-256 recorded, or another
/// package than the one listed). Nothing is read past the reader's bound.
fn read_stored(data: &Path, entry: &PackageEntry) -> Result<Package, String> {
    let shown = shown_path(&entry.id, &entry.version);
    let file = match std::fs::File::open(stored_path(data, &entry.id, &entry.version)) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(format!(
            "{shown} is missing: logic/{PACKAGES_FILE} lists the package and its file is not there"
        ))
        }
        Err(e) => return Err(format!("{shown} cannot be read: {e}")),
    };
    let mut bytes = Vec::new();
    file.take(MAX_PACKAGE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("{shown} cannot be read: {e}"))?;
    let package = mythos_logic_package::read(&bytes)
        .map_err(|why| format!("{shown} no longer reads as a package: {why}"))?;
    if package.sha256() != entry.sha256 {
        return Err(format!(
            "{shown} does not have the SHA-256 logic/{PACKAGES_FILE} records; it was changed after it was added"
        ));
    }
    let manifest = package.manifest();
    if manifest.id != entry.id || manifest.version != entry.version {
        return Err(format!(
            "{shown} holds package {} {}, not {} {} that logic/{PACKAGES_FILE} lists",
            manifest.id, manifest.version, entry.id, entry.version
        ));
    }
    Ok(package)
}

fn entry_json(entry: &PackageEntry) -> Value {
    serde_json::to_value(entry).unwrap_or(Value::Null)
}

// -----------------------------------------------------------------------------
// What a package declares, and the check with its dependencies
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Block,
    Type,
    Program,
}

/// Each name `text` declares, with what it is, as the parser reads it (as `library_refusals`
/// reads a library source).
fn declarations(text: &str) -> Vec<(String, Kind)> {
    read_declarations(text).0
}

/// [`declarations`], and whether `text` also declares a function, a function block or a type
/// whose name the parser cannot read: one inside NAMESPACE, CLASS or INTERFACE, which it does
/// not read yet, or one whose name is broken (the parser's `lost`). A program whose name is lost
/// is not counted: no name a package declares is compared with a program's.
fn read_declarations(text: &str) -> (Vec<(String, Kind)>, bool) {
    let (read, _) =
        mythos_logic_st::parser::Parser::new(mythos_logic_st::lexer::tokens(text)).source();
    let lost = read.lost.iter().any(|l| *l != Lost::Program);
    let mut found: Vec<(String, Kind)> = read
        .types
        .iter()
        .map(|t| (t.name.clone(), Kind::Type))
        .collect();
    found.extend(read.pous.iter().map(|p| {
        let kind = match p.kind {
            PouKind::Program => Kind::Program,
            PouKind::Function | PouKind::FunctionBlock => Kind::Block,
        };
        (p.name.clone(), kind)
    }));
    (found, lost)
}

/// The name a package source is given for the compiler: `logic/packages/<id>/<version>/<path>`.
fn source_name(id: &str, version: &str, path: &str) -> String {
    format!("logic/{PACKAGES_DIR}/{id}/{version}/{path}")
}

/// The stored packages `depends` reaches, transitively, each read again, in byte order of
/// their lowercase ids; the words of the first that the project lacks or that does not read.
fn dependencies_of(
    data: &Path,
    file: &PackagesFile,
    own_id: &str,
    depends: &[Dependency],
) -> Result<Vec<(PackageEntry, Package)>, String> {
    let mut found: Vec<(PackageEntry, Package)> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::from([own_id.to_ascii_lowercase()]);
    let mut wanted: Vec<String> = depends.iter().map(|d| d.id.clone()).collect();
    while let Some(id) = wanted.pop() {
        if !seen.insert(id.to_ascii_lowercase()) {
            continue;
        }
        let entry = file
            .packages
            .iter()
            .find(|p| p.id.eq_ignore_ascii_case(&id))
            .ok_or_else(|| format!("the project has no package '{id}'"))?;
        let package = read_stored(data, entry)?;
        wanted.extend(package.manifest().depends.iter().map(|d| d.id.clone()));
        found.push((entry.clone(), package));
    }
    found.sort_by(|(a, _), (b, _)| {
        a.id.to_ascii_lowercase()
            .as_bytes()
            .cmp(b.id.to_ascii_lowercase().as_bytes())
    });
    Ok(found)
}

/// The package checked by the compiler with the sources of its dependencies and nothing else
/// of the project (R18): its own blocks as `POST /library/check` answers blocks, or the
/// compiler's diagnostics.
fn check_with(
    dependencies: &[(PackageEntry, Package)],
    manifest: &Manifest,
    sources: &[(String, String)],
) -> Result<Vec<Value>, Vec<Diagnostic>> {
    let mut named: Vec<(String, &str)> = Vec::new();
    for (entry, package) in dependencies {
        for (path, text) in package.sources() {
            named.push((source_name(&entry.id, &entry.version, path), text));
        }
    }
    for (path, text) in sources {
        named.push((source_name(&manifest.id, &manifest.version, path), text));
    }
    let library: Vec<(&str, &str)> = named.iter().map(|(n, t)| (n.as_str(), *t)).collect();
    let blocks = mythos_logic_compiler::library_blocks(&Sources {
        library: &library,
        ..Default::default()
    })?;
    let own: BTreeSet<String> = sources
        .iter()
        .flat_map(|(_, text)| declarations(text))
        .filter(|(_, kind)| *kind == Kind::Block)
        .map(|(name, _)| name.to_ascii_lowercase())
        .collect();
    Ok(blocks
        .iter()
        .filter(|b| own.contains(&b.name.to_ascii_lowercase()))
        .map(block_json)
        .collect())
}

/// The ids of the other stored packages that depend on `id`, each read again; and those whose
/// file does not read, with why, of which it cannot be told.
fn dependents_of(
    data: &Path,
    file: &PackagesFile,
    id: &str,
) -> (Vec<String>, Vec<(String, String)>) {
    let mut dependents = Vec::new();
    let mut untold = Vec::new();
    for other in file
        .packages
        .iter()
        .filter(|p| !p.id.eq_ignore_ascii_case(id))
    {
        match read_stored(data, other) {
            Ok(package) => {
                if package
                    .manifest()
                    .depends
                    .iter()
                    .any(|d| d.id.eq_ignore_ascii_case(id))
                {
                    dependents.push(other.id.clone());
                }
            }
            Err(problem) => untold.push((other.id.clone(), problem)),
        }
    }
    (dependents, untold)
}

/// The lowercase ids of the stored packages `entry` reaches through what it depends on,
/// transitively, each read again; a package whose file does not read reaches nothing further.
/// Empty when `entry`'s own file does not read.
fn reached_from(data: &Path, file: &PackagesFile, entry: &PackageEntry) -> BTreeSet<String> {
    let mut reached = BTreeSet::new();
    let mut wanted: Vec<String> = match read_stored(data, entry) {
        Ok(package) => package
            .manifest()
            .depends
            .iter()
            .map(|d| d.id.clone())
            .collect(),
        Err(_) => return reached,
    };
    while let Some(id) = wanted.pop() {
        if !reached.insert(id.to_ascii_lowercase()) {
            continue;
        }
        let Some(other) = file
            .packages
            .iter()
            .find(|p| p.id.eq_ignore_ascii_case(&id))
        else {
            continue;
        };
        if let Ok(package) = read_stored(data, other) {
            wanted.extend(package.manifest().depends.iter().map(|d| d.id.clone()));
        }
    }
    reached
}

// -----------------------------------------------------------------------------
// The add, assessed once for the add and for its check
// -----------------------------------------------------------------------------

/// The body of `POST /packages` and `POST /packages/check`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FileBody {
    file: String,
    #[serde(default)]
    file_name: Option<String>,
}

/// The file a POST gives, decoded: the base64's length is checked before it is decoded.
fn given_file(call: &HttpCall) -> Result<(Vec<u8>, Option<String>), Refusal> {
    let given: FileBody = body(call)?;
    let not_base64 = || {
        error(
            400,
            "the file is not base64 of the standard alphabet with padding",
        )
    };
    if given.file.len() > MAX_PACKAGE_BASE64_LEN {
        // The bound is counted in bytes, which are characters only when the text is ASCII, as
        // base64 is: a text that is not is said as what it is, not as a count it does not have.
        if !given.file.is_ascii() {
            return Err(not_base64());
        }
        return Err(error(
            400,
            format!(
                "the file is {} characters of base64; at most {MAX_PACKAGE_BASE64_LEN} are taken, the base64 of {MAX_PACKAGE_BYTES} bytes",
                given.file.len()
            ),
        ));
    }
    if let Some(problem) = given.file_name.as_deref().and_then(file_name_problem) {
        return Err(error(400, problem));
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(given.file.as_bytes())
        .map_err(|_| not_base64())?;
    Ok((bytes, given.file_name))
}

/// A package read from a POST, as the add looks at it.
struct Incoming<'a> {
    manifest: &'a Manifest,
    sources: &'a [(String, String)],
}

/// A name the package declares that is declared elsewhere too (R17).
#[derive(Debug, Clone, Serialize)]
struct Clash {
    name: String,
    package_source: String,
    other: String,
}

/// Everything the add decides on, computed once, so that the check answers `ok` exactly when
/// the add would add.
struct Assessment {
    /// The package already in the project under its id, in any case.
    present: Option<PackageEntry>,
    /// Dependencies the project does not have.
    missing: Vec<Dependency>,
    /// Stored packages whose file does not read, and sources of the project (library sources
    /// and the data module's types) that do not read as text or declare something whose name
    /// cannot be read: what they declare is unknown, so whether the package clashes is too.
    unreadable: Vec<String>,
    /// A TYPE or a PROGRAM in a source of the package.
    misplaced: Vec<String>,
    clashes: Vec<Clash>,
    /// The library sources of the project with the package, as a compile would be given them.
    sources: usize,
    /// The package checked with its dependencies; `None` when one is missing or does not
    /// read, and the add is refused for that.
    checked: Option<Result<Vec<Value>, Vec<Diagnostic>>>,
}

fn assess(data: &Path, package: &Incoming) -> Result<Assessment, Refusal> {
    let file = load(data)?;
    let project = read_project(data)?;
    let id = &package.manifest.id;

    let present: Vec<&PackageEntry> = file
        .packages
        .iter()
        .filter(|p| p.id.eq_ignore_ascii_case(&package.manifest.id))
        .collect();
    let present = present.first().map(|p| (*p).clone());
    let missing: Vec<Dependency> = package
        .manifest
        .depends
        .iter()
        .filter(|d| {
            !file
                .packages
                .iter()
                .any(|p| p.id.eq_ignore_ascii_case(&d.id))
        })
        .cloned()
        .collect();

    let mut unreadable = Vec::new();
    let mut others: Vec<(PackageEntry, Package)> = Vec::new();
    for entry in file
        .packages
        .iter()
        .filter(|p| !p.id.eq_ignore_ascii_case(id))
    {
        match read_stored(data, entry) {
            Ok(stored) => others.push((entry.clone(), stored)),
            Err(problem) => unreadable.push(format!(
                "package '{}' {} of the project does not read, so what it declares is not known: {problem}; remove it or put its file back first",
                entry.id, entry.version
            )),
        }
    }

    let mut misplaced = Vec::new();
    let mut own: Vec<(String, Kind, &str)> = Vec::new();
    for (path, text) in package.sources {
        for (name, kind) in declarations(text) {
            match kind {
                Kind::Type => misplaced.push(format!(
                    "{name} is a TYPE in {id}'s {path}; types in a package come with ADR-015 step 2"
                )),
                Kind::Program => misplaced.push(format!(
                    "{name} is a PROGRAM in {id}'s {path}; a package holds functions and function blocks"
                )),
                Kind::Block => {}
            }
            if kind != Kind::Program {
                own.push((name, kind, path));
            }
        }
    }

    // Every other declaration, with how its place is said; a source of which not every name
    // is known is said, not taken as declaring nothing more.
    let version = &package.manifest.version;
    let unknown = |what: String| {
        format!(
            "{what}, so whether {id} {version} clashes with it is not known; correct it or take it out first"
        )
    };
    for d in &project.unreadable_library {
        unreadable.push(unknown(d.message.clone()));
    }
    // A type file that is not text: its source is named; the data's other problems name none.
    for d in project.problems.iter().filter(|d| d.source.is_some()) {
        unreadable.push(unknown(d.message.clone()));
    }
    let mut elsewhere: Vec<(String, String)> = Vec::new();
    for (sources, place) in [
        (&project.library, "the library source"),
        (&project.types, "the type"),
    ] {
        for (source, text) in sources {
            let (declared, lost) = read_declarations(text);
            if lost {
                unreadable.push(unknown(format!(
                    "{place} '{source}' declares something whose name cannot be read (inside NAMESPACE, CLASS or INTERFACE, which are not read yet, or a declaration whose name is broken)"
                )));
            }
            for (name, kind) in declared {
                if kind != Kind::Program {
                    elsewhere.push((name, format!("{place} '{source}'")));
                }
            }
        }
    }
    for (entry, stored) in &others {
        for (path, text) in stored.sources() {
            for (name, kind) in declarations(text) {
                if kind != Kind::Program {
                    elsewhere.push((name, format!("{} {} in {path}", entry.id, entry.version)));
                }
            }
        }
    }
    let mut clashes = Vec::new();
    for (name, _, path) in &own {
        for (other_name, other) in &elsewhere {
            if other_name.eq_ignore_ascii_case(name) {
                clashes.push(Clash {
                    name: name.clone(),
                    package_source: (*path).to_string(),
                    other: other.clone(),
                });
            }
        }
    }

    let sources = project.types.len()
        + project.library.len()
        + others.iter().map(|(_, p)| p.sources().len()).sum::<usize>()
        + package.sources.len();

    let checked = if missing.is_empty() && unreadable.is_empty() {
        // A dependency of a dependency the project lacks (after a hand edit) is said as the
        // check's diagnostic.
        Some(
            dependencies_of(data, &file, id, &package.manifest.depends)
                .map_err(|why| vec![Diagnostic::whole(why)])
                .and_then(|dependencies| {
                    check_with(&dependencies, package.manifest, package.sources)
                }),
        )
    } else {
        None
    };
    Ok(Assessment {
        present,
        missing,
        unreadable,
        misplaced,
        clashes,
        sources,
        checked,
    })
}

impl Assessment {
    /// Why the add refuses the package, in the order the add looks; `None` when it adds it.
    fn refusal(&self, manifest: &Manifest) -> Option<Refusal> {
        let (id, version) = (&manifest.id, &manifest.version);
        if let Some(stored) = &self.present {
            return Some(error(
                409,
                format!(
                    "the project already has package '{}' {}; remove it first (an update is ADR-015 step 3)",
                    stored.id, stored.version
                ),
            ));
        }
        if !self.missing.is_empty() {
            let named: Vec<&str> = self.missing.iter().map(|d| d.id.as_str()).collect();
            let them = if named.len() == 1 { "it" } else { "them" };
            return Some((
                409,
                json!({
                    "error": format!(
                        "{id} {version} depends on {}, which the project does not have: add {them} first",
                        named.join(", ")
                    ),
                    "missing": self.missing_json(),
                    "versions_checked": false,
                }),
            ));
        }
        if !self.unreadable.is_empty() {
            return Some(error(409, self.unreadable.join("; ")));
        }
        if !self.misplaced.is_empty() {
            return Some(error(400, self.misplaced.join("; ")));
        }
        if !self.clashes.is_empty() {
            let words: Vec<String> = self
                .clashes
                .iter()
                .map(|c| {
                    format!(
                        "{} is declared by {id} {version} in {} and by {}",
                        c.name, c.package_source, c.other
                    )
                })
                .collect();
            return Some((
                409,
                json!({ "error": words.join("; "), "clashes": self.clashes }),
            ));
        }
        if self.sources > MAX_SOURCES {
            return Some(error(
                409,
                format!(
                    "with {id} {version} the project would have {} library sources; at most {MAX_SOURCES} are taken",
                    self.sources
                ),
            ));
        }
        match &self.checked {
            Some(Ok(_)) => None,
            Some(Err(diagnostics)) => Some((
                400,
                json!({
                    "error": format!("{id} {version} does not check"),
                    "diagnostics": diagnostics,
                    "versions_checked": false,
                }),
            )),
            // Only when `missing` or `unreadable` refused it above.
            None => Some(error(
                409,
                format!("{id} {version} is not checked: a package it depends on is missing or does not read"),
            )),
        }
    }

    fn missing_json(&self) -> Vec<Value> {
        self.missing
            .iter()
            .map(|d| json!({ "id": d.id, "version": d.version }))
            .collect()
    }
}

// -----------------------------------------------------------------------------
// The routes
// -----------------------------------------------------------------------------

fn no_package(id: &str) -> Refusal {
    error(404, format!("the project has no package '{id}'"))
}

/// `GET /packages`: each listed package with its name, licence and format read again from its
/// file, or `problem` and no manifest field.
fn list(data: &Path) -> Result<Value, Refusal> {
    let mut file = load(data)?;
    file.packages.sort_by(|a, b| {
        a.id.to_ascii_lowercase()
            .as_bytes()
            .cmp(b.id.to_ascii_lowercase().as_bytes())
    });
    let packages: Vec<Value> = file
        .packages
        .iter()
        .map(|entry| {
            let mut answer = entry_json(entry);
            match read_stored(data, entry) {
                Ok(package) => {
                    let manifest = package.manifest();
                    answer["name"] = json!(manifest.name);
                    answer["license"] = json!(manifest.license);
                    answer["format"] = json!(manifest.format);
                }
                Err(problem) => answer["problem"] = json!(problem),
            }
            answer
        })
        .collect();
    Ok(json!({ "packages": packages }))
}

/// `GET /packages/:package`: the entry, and when its file reads, the manifest, the sources,
/// the blocks of the package checked with its dependencies and the packages depending on it.
fn get(data: &Path, id: &str) -> Result<Value, Refusal> {
    let file = load(data)?;
    let entry = file
        .packages
        .iter()
        .find(|p| p.id.eq_ignore_ascii_case(id))
        .ok_or_else(|| no_package(id))?;
    let package = match read_stored(data, entry) {
        Ok(package) => package,
        Err(problem) => return Ok(json!({ "entry": entry_json(entry), "problem": problem })),
    };
    let manifest = package.manifest();
    let checked = dependencies_of(data, &file, &manifest.id, &manifest.depends)
        .map_err(|why| vec![Diagnostic::whole(why)])
        .and_then(|dependencies| check_with(&dependencies, manifest, package.sources()));
    let (dependents, untold) = dependents_of(data, &file, &entry.id);
    let mut answer = json!({
        "entry": entry_json(entry),
        "manifest": manifest,
        "sources": package
            .sources()
            .iter()
            .map(|(path, source)| json!({ "path": path, "source": source }))
            .collect::<Vec<_>>(),
        "dependents": dependents,
        "versions_checked": false,
    });
    match checked {
        Ok(blocks) => {
            answer["blocks"] = json!(blocks);
            answer["diagnostics"] = json!([]);
        }
        Err(diagnostics) => answer["diagnostics"] = json!(diagnostics),
    }
    if !untold.is_empty() {
        answer["dependents_unknown"] = untold
            .iter()
            .map(|(id, problem)| json!({ "id": id, "problem": problem }))
            .collect();
    }
    Ok(answer)
}

/// `POST /packages/check`: what the add would do, writing nothing.
fn check(call: &HttpCall, data: &Path) -> Result<Value, Refusal> {
    let (bytes, _) = given_file(call)?;
    let read = mythos_logic_package::read(&bytes).map_err(|why| error(400, why))?;
    let package = Incoming {
        manifest: read.manifest(),
        sources: read.sources(),
    };
    let assessed = assess(data, &package)?;
    let refusal = assessed.refusal(package.manifest);
    let mut answer = json!({
        "ok": refusal.is_none(),
        "package": package.manifest,
        "sha256": read.sha256(),
        "size": read.size(),
        "diagnostics": [],
        "clashes": assessed.clashes,
        "missing": assessed.missing_json(),
        "present": assessed
            .present
            .as_ref()
            .map(|p| json!({ "version": p.version, "sha256": p.sha256 })),
        "versions_checked": false,
    });
    match &assessed.checked {
        Some(Ok(blocks)) => answer["blocks"] = json!(blocks),
        Some(Err(diagnostics)) => answer["diagnostics"] = json!(diagnostics),
        None => {}
    }
    if let Some((status, words)) = refusal {
        answer["refusal"] = json!({ "status": status, "error": words["error"] });
    }
    Ok(answer)
}

/// `POST /packages`: the package added, the file written first and the list second.
fn add(call: &HttpCall, data: &Path) -> Result<(Value, PackageEntry), Refusal> {
    let (bytes, file_name) = given_file(call)?;
    let read = mythos_logic_package::read(&bytes).map_err(|why| error(400, why))?;
    let package = Incoming {
        manifest: read.manifest(),
        sources: read.sources(),
    };
    let assessed = assess(data, &package)?;
    if let Some(refusal) = assessed.refusal(package.manifest) {
        return Err(refusal);
    }
    let blocks = match assessed.checked {
        Some(Ok(blocks)) => blocks,
        _ => Vec::new(),
    };
    let entry = PackageEntry {
        id: package.manifest.id.clone(),
        version: package.manifest.version.clone(),
        sha256: read.sha256().to_string(),
        origin: Origin {
            kind: OriginKind::File,
            file_name,
        },
    };
    write_synced(&stored_path(data, &entry.id, &entry.version), &bytes)?;
    let mut file = load(data)?;
    file.packages.push(entry.clone());
    save(data, &mut file)?;
    Ok((
        json!({ "added": entry_json(&entry), "blocks": blocks, "versions_checked": false }),
        entry,
    ))
}

/// Remove a stored file of a package: a write, gated as [`write_synced`] is. A file already
/// gone is not an error.
fn remove_stored(path: &Path) -> Result<(), String> {
    about_to_write(path).map_err(|(_, body)| body["error"].as_str().unwrap_or("").to_string())?;
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

/// `DELETE /packages/:package`: the list written first, the file removed second, then its
/// directory when it is empty.
fn remove(data: &Path, id: &str) -> Result<(Value, PackageEntry), Refusal> {
    let mut file = load(data)?;
    let at = file
        .packages
        .iter()
        .position(|p| p.id.eq_ignore_ascii_case(id))
        .ok_or_else(|| no_package(id))?;
    let entry = file.packages[at].clone();
    let (dependents, untold) = dependents_of(data, &file, &entry.id);
    // A dependent this package itself depends on, transitively, is in a cycle with it (made by
    // hand: an add needs its dependencies present). Neither could be removed first, so it does
    // not refuse the remove; it is answered.
    let reached = reached_from(data, &file, &entry);
    let (in_cycle, dependents): (Vec<String>, Vec<String>) = dependents
        .into_iter()
        .partition(|d| reached.contains(&d.to_ascii_lowercase()));
    if !dependents.is_empty() {
        return Err((
            409,
            json!({
                "error": format!(
                    "package '{}' is a dependency of {}: remove those first",
                    entry.id,
                    dependents.join(", ")
                ),
                "dependents": dependents,
            }),
        ));
    }
    file.packages.remove(at);
    save(data, &mut file)?;
    let path = stored_path(data, &entry.id, &entry.version);
    let mut answer = json!({ "removed": { "id": entry.id, "version": entry.version } });
    // A package whose file does not read may depend on this one; that cannot be told, and it
    // does not refuse the remove (two such packages would otherwise refuse each other's): it is
    // said, as `get` says it.
    if !untold.is_empty() {
        answer["dependents_unknown"] = untold
            .iter()
            .map(|(id, problem)| json!({ "id": id, "problem": problem }))
            .collect();
    }
    if !in_cycle.is_empty() {
        answer["dependents_in_a_cycle"] = json!(in_cycle);
    }
    if let Err(why) = remove_stored(&path) {
        // The list no longer names it, so it is not a package; the file is said, not hidden.
        answer["left"] = json!(format!(
            "{} is no longer a package of the project but its file is left: {why}",
            shown_path(&entry.id, &entry.version)
        ));
    }
    if let Some(dir) = path.parent() {
        if about_to_write(dir).is_ok() {
            // Only an empty directory is removed; one that still holds a file stays.
            let _ = std::fs::remove_dir(dir);
        }
    }
    Ok((answer, entry))
}

/// The routes of the project's packages: the reply, and the data of the event to send when a
/// package was added or removed.
pub(crate) fn packages(call: &HttpCall, data: &Path) -> (HttpReply, Option<Value>) {
    let project = call.project.clone().unwrap_or_default();
    let id = call.params.get("package").map(String::as_str).unwrap_or("");
    let changed = |entry: &PackageEntry, what: &str| json!({ "project": project, "package": entry.id, "version": entry.version, "what": what });
    match (call.method.as_str(), call.route.as_str()) {
        ("GET", "/packages") => (reply(list(data), 200), None),
        ("GET", "/packages/:package") => (reply(get(data, id), 200), None),
        ("POST", "/packages/check") => (reply(check(call, data), 200), None),
        ("POST", "/packages") => match add(call, data) {
            Ok((answer, entry)) => (HttpReply::json(201, answer), Some(changed(&entry, "added"))),
            Err(refusal) => (reply(Err(refusal), 201), None),
        },
        ("DELETE", "/packages/:package") => match remove(data, id) {
            Ok((answer, entry)) => (
                HttpReply::json(200, answer),
                Some(changed(&entry, "removed")),
            ),
            Err(refusal) => (reply(Err(refusal), 200), None),
        },
        (method, route) => (
            HttpReply::error(
                404,
                format!("the logic module has no route {method} {route}"),
            ),
            None,
        ),
    }
}

#[cfg(test)]
mod tests;
