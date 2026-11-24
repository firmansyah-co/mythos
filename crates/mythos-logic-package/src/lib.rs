//! A library package of the logic module (ADR-015 step 1): one zip file holding
//! `manifest.json`, `checksums.json` and Structured Text sources `src/<name>.st`.
//!
//! [`read`] takes the file's bytes and gives the whole package or refuses it with words that
//! say what was wrong. It opens no file, reads no environment and writes nothing, so the logic
//! module, the command line and the data module can all use it. The file comes from outside the
//! plant and is hostile until read:
//!
//! - nothing is allocated or read by a size the file declares: the bound on unpacked bytes is
//!   on the bytes really read, entry by entry, and reading stops at it;
//! - the zip's own structure is walked here before the zip crate is given the file, so the
//!   crate only ever sees a plain zip with no byte outside its entries and central directory,
//!   no zip64 record and at most [`MAX_PACKAGE_ENTRIES`] entries;
//! - one allow-list of entry names is applied before any entry is read;
//! - every bound is a named constant, and the words of its refusal are built from it.

use std::collections::HashSet;
use std::fmt;
use std::io::{Cursor, Read};

use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use serde::Serialize;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

/// The manifest format this reader takes.
pub const PACKAGE_FORMAT: u32 = 1;
/// The largest package file taken, in bytes. Basis: base64 of it inside a JSON body stays under
/// axum's default body limit of 2_097_152 bytes on a module route (ADR-015 P1 contract, H2).
pub const MAX_PACKAGE_BYTES: usize = 1_048_576;
/// The base64 form of [`MAX_PACKAGE_BYTES`]: 4·⌈1_048_576/3⌉ characters.
pub const MAX_PACKAGE_BASE64_LEN: usize = 1_398_104;
/// The most bytes all entries of a package may hold once unpacked, counted as they are read.
pub const MAX_UNPACKED_BYTES: u64 = 2_097_152;
pub const MAX_PACKAGE_ENTRIES: usize = 1024;
pub const MAX_PACKAGE_ID_LEN: usize = 64;
pub const MAX_VERSION_LEN: usize = 32;
pub const MANIFEST_FILE: &str = "manifest.json";
pub const CHECKSUMS_FILE: &str = "checksums.json";
/// The longest name of a library source, the stem of `src/<name>.st`.
pub const MAX_SOURCE_NAME_LEN: usize = 64;

/// The folder of a package's sources and the one directory entry a package may hold.
const SOURCE_DIR: &str = "src/";
/// The extension of a source, exactly.
const SOURCE_EXTENSION: &str = ".st";
/// The only value `form` takes in format 1.
const FORM_SOURCE: &str = "source";
/// The only language format 1 takes.
const LANGUAGE_ST: &str = "st";

/// Names that are devices on Windows whatever their case or extension.
const WINDOWS_DEVICE_NAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM0", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
    "COM8", "COM9", "LPT0", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// The keys of a format 1 manifest; any other key is refused by name.
const MANIFEST_KEYS: &[&str] = &[
    "format",
    "id",
    "name",
    "version",
    "form",
    "languages",
    "files",
    "depends",
    "license",
    "authors",
    "description",
    "targets",
    "interface",
    "signature",
];

/// A package read whole.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
    pub manifest: Manifest,
    /// Each source as `(path in the package, text as written)`, in byte order of path.
    pub sources: Vec<(String, String)>,
    /// The lowercase hex SHA-256 of the file exactly as given.
    pub sha256: String,
    /// The file's length in bytes.
    pub size: u64,
}

/// A package's `manifest.json`, format 1. An optional field that is absent is `None` and is
/// left out when serialised: it is never filled with a default.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Manifest {
    pub format: u32,
    pub id: String,
    pub name: String,
    pub version: String,
    pub form: String,
    pub languages: Vec<String>,
    pub files: Vec<String>,
    pub depends: Vec<Dependency>,
    pub license: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authors: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub targets: Option<Vec<String>>,
    /// Reserved: when present it is `[]`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interface: Option<Vec<String>>,
    /// Reserved: when present it is `null`, serialised as `null`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<()>,
}

/// One entry of a manifest's `depends`. The version is kept as text and not compared
/// (ADR-015 §5 question 1 is open).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Dependency {
    pub id: String,
    pub version: String,
}

/// Reads a package file: the whole package, or the words of why it is refused.
pub fn read(file: &[u8]) -> Result<Package, String> {
    if file.len() > MAX_PACKAGE_BYTES {
        return Err(format!(
            "the package is {} bytes; at most {MAX_PACKAGE_BYTES} are taken",
            file.len()
        ));
    }
    let layout = zip_layout(file)?;
    let names: Vec<String> = layout
        .iter()
        .map(|e| String::from_utf8_lossy(&e.name).into_owned())
        .collect();

    let mut archive = zip::ZipArchive::new(Cursor::new(file))
        .map_err(|e| format!("it is not a zip file: {e}"))?;
    if archive.len() != names.len() {
        return Err(format!(
            "it is not a zip file: its central directory holds {} entries and the zip crate read {}",
            names.len(),
            archive.len()
        ));
    }
    let mut unpacked: u64 = 0;

    if let Some(problem) = names_problem(&names) {
        // A package of a newer format may hold entries format 1 does not know; its format is
        // the better answer. manifest.json passes the name rule, so reading it here reads
        // nothing the rule refuses.
        let manifests: Vec<usize> = (0..names.len())
            .filter(|&i| names[i] == MANIFEST_FILE)
            .collect();
        if let [index] = manifests[..] {
            let bytes = read_entry(&mut archive, index, &names[index], &mut unpacked)?;
            manifest_object(&bytes)?;
        }
        return Err(problem);
    }

    let mut manifest_bytes = None;
    let mut checksums_bytes = None;
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    for (index, name) in names.iter().enumerate() {
        let bytes = read_entry(&mut archive, index, name, &mut unpacked)?;
        if name == SOURCE_DIR {
            if !bytes.is_empty() {
                return Err(format!("{SOURCE_DIR} is a directory and holds bytes"));
            }
            continue;
        }
        if name == MANIFEST_FILE {
            manifest_bytes = Some(bytes.clone());
        } else if name == CHECKSUMS_FILE {
            checksums_bytes = Some(bytes.clone());
        }
        files.push((name.clone(), bytes));
    }

    let manifest_bytes =
        manifest_bytes.ok_or_else(|| format!("the package has no {MANIFEST_FILE}"))?;
    let manifest_map = manifest_object(&manifest_bytes)?;
    let checksums_bytes =
        checksums_bytes.ok_or_else(|| format!("the package has no {CHECKSUMS_FILE}"))?;
    let checksums = checksums_of(&checksums_bytes)?;
    for (name, bytes) in files.iter().filter(|(name, _)| name != CHECKSUMS_FILE) {
        let listed = checksums
            .get(name)
            .ok_or_else(|| format!("{name} is not listed in {CHECKSUMS_FILE}"))?;
        if !listed.eq_ignore_ascii_case(&sha256_hex(bytes)) {
            return Err(format!("{name} does not match its checksum"));
        }
    }
    for listed in checksums.keys() {
        if listed == CHECKSUMS_FILE || !files.iter().any(|(name, _)| name == listed) {
            return Err(format!(
                "{CHECKSUMS_FILE} lists {}, which the package does not hold",
                shown(listed)
            ));
        }
    }

    let manifest = manifest_of(manifest_map)?;

    let mut sources: Vec<(String, String)> = Vec::new();
    for (name, bytes) in files {
        if !name.starts_with(SOURCE_DIR) {
            continue;
        }
        if !manifest.files.contains(&name) {
            return Err(format!("{name} is not among the files of {MANIFEST_FILE}"));
        }
        sources.push((name.clone(), source_text(&name, bytes)?));
    }
    for listed in &manifest.files {
        if !sources.iter().any(|(name, _)| name == listed) {
            return Err(format!(
                "{MANIFEST_FILE} lists {}, which the package does not hold",
                shown(listed)
            ));
        }
    }
    sources.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));

    let sha256 = sha256_hex(file);
    Ok(Package {
        manifest,
        sources,
        sha256,
        size: file.len() as u64,
    })
}

/// What is wrong with `id` as a package id, as the words of the refusal; `None` when it is one:
/// two or more parts joined by `.`, each an ASCII letter followed by ASCII letters, digits or
/// `_`, no part a device name of Windows, at most [`MAX_PACKAGE_ID_LEN`] characters in all.
/// The id becomes a directory name on every OS, so no other letter is taken.
pub fn id_problem(id: &str) -> Option<String> {
    let length = id.chars().count();
    if length > MAX_PACKAGE_ID_LEN {
        return Some(format!(
            "a package id is at most {MAX_PACKAGE_ID_LEN} characters; this one is {length}"
        ));
    }
    let parts: Vec<&str> = id.split('.').collect();
    let well_formed = parts.len() >= 2 && parts.iter().all(|part| ascii_identifier(part));
    if !well_formed {
        return Some(format!(
            "'{}' is not a package id: two or more parts joined by '.', each an ASCII letter followed by ASCII letters, ASCII digits or _ (an id becomes a directory name on every OS, so no other letter is taken)",
            shown(id)
        ));
    }
    if let Some(part) = parts.iter().find(|part| device_name(part)) {
        return Some(format!(
            "'{id}' is not a package id: its part '{part}' is a device name on Windows (CON, PRN, AUX, NUL, COM0 to COM9, LPT0 to LPT9)"
        ));
    }
    None
}

/// What is wrong with `name` as the name of a library source (the stem of `src/<name>.st`), as
/// the words of the refusal; `None` when it is one.
///
/// This re-states, rule and words, `library_name_problem` of the logic module
/// (`plugins/logic/src/lib.rs`, with `valid_name` and `device_name`): an ASCII letter, then
/// ASCII letters, digits or `_`, at most 64 characters, and not a device name of Windows in any
/// case. A platform crate cannot depend on a module, so the module cannot be called from here;
/// a later slice makes the module use this one.
pub fn source_name_problem(name: &str) -> Option<String> {
    if !(ascii_identifier(name) && name.len() <= MAX_SOURCE_NAME_LEN) {
        return Some(format!(
            "'{}' is not a library source name: a letter, then letters, digits or _, at most {MAX_SOURCE_NAME_LEN}",
            shown(name)
        ));
    }
    if device_name(name) {
        return Some(format!(
            "'{name}' cannot be a library source name: it is a device name on Windows (CON, PRN, AUX, NUL, COM0 to COM9, LPT0 to LPT9)"
        ));
    }
    None
}

fn ascii_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn device_name(name: &str) -> bool {
    WINDOWS_DEVICE_NAMES
        .iter()
        .any(|d| d.eq_ignore_ascii_case(name))
}

/// What is wrong with `version`, as the words of the refusal; `None` when it is
/// `MAJOR.MINOR.PATCH` (decimal, no leading zero), optionally `-<pre>` of `[0-9A-Za-z.-]` with
/// no empty part, no `+`, at most [`MAX_VERSION_LEN`] characters.
fn version_problem(field: &str, version: &str) -> Option<String> {
    let length = version.chars().count();
    if length > MAX_VERSION_LEN {
        return Some(format!(
            "{field} is at most {MAX_VERSION_LEN} characters; this one is {length}"
        ));
    }
    let (core, pre) = match version.split_once('-') {
        Some((core, pre)) => (core, Some(pre)),
        None => (version, None),
    };
    let numbers: Vec<&str> = core.split('.').collect();
    let core_ok = numbers.len() == 3
        && numbers.iter().all(|n| {
            !n.is_empty()
                && n.bytes().all(|b| b.is_ascii_digit())
                && (n.len() == 1 || !n.starts_with('0'))
        });
    let pre_ok = pre.is_none_or(|pre| {
        pre.split('.').all(|part| {
            !part.is_empty() && part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
    });
    if core_ok && pre_ok {
        return None;
    }
    Some(format!(
        "{field} '{}' is not a version: MAJOR.MINOR.PATCH in decimal with no leading zero, optionally -<pre> of letters, digits, '.' and '-' with no empty part, and no + (build metadata is not taken)",
        shown(version)
    ))
}

/// The one rule for entry names: what is wrong with `name` as the name of an entry of a
/// package; `None` when it is `manifest.json`, `checksums.json`, `src/` or `src/<stem>.st` with
/// `<stem>` a library source name and the extension exactly `.st`.
fn entry_name_problem(name: &str) -> Option<String> {
    if name == MANIFEST_FILE || name == CHECKSUMS_FILE || name == SOURCE_DIR {
        return None;
    }
    let refusal = format!(
        "{} is not a file a package may hold: {MANIFEST_FILE}, {CHECKSUMS_FILE} and {SOURCE_DIR}<name>{SOURCE_EXTENSION}",
        shown(name)
    );
    let Some(stem) = name
        .strip_prefix(SOURCE_DIR)
        .and_then(|rest| rest.strip_suffix(SOURCE_EXTENSION))
    else {
        return Some(refusal);
    };
    source_name_problem(stem).map(|why| format!("{refusal}; {why}"))
}

/// The first entry name that is not taken, or the first two that are one name without regard to
/// case.
fn names_problem(names: &[String]) -> Option<String> {
    let mut seen: Vec<String> = Vec::with_capacity(names.len());
    for name in names {
        let name = name.clone();
        if let Some(problem) = entry_name_problem(&name) {
            return Some(problem);
        }
        if seen.iter().any(|s: &String| s.eq_ignore_ascii_case(&name)) {
            let first = seen
                .iter()
                .find(|s| s.eq_ignore_ascii_case(&name))
                .map_or(name.as_str(), String::as_str);
            return Some(format!("{first} and {name} are one name"));
        }
        seen.push(name);
    }
    None
}

/// Reads entry `index` whole, counting its bytes against [`MAX_UNPACKED_BYTES`] as they are
/// read: never more than one byte past what is left is read, and never by its declared size.
fn read_entry(
    archive: &mut zip::ZipArchive<Cursor<&[u8]>>,
    index: usize,
    name: &str,
    unpacked: &mut u64,
) -> Result<Vec<u8>, String> {
    let mut entry = archive
        .by_index(index)
        .map_err(|e| format!("{name}: {e}"))?;
    if let Some(mode) = entry.unix_mode() {
        if mode & 0o170000 == 0o120000 {
            return Err(format!("{name} is a symbolic link; a package holds files"));
        }
    }
    let budget = MAX_UNPACKED_BYTES.saturating_sub(*unpacked);
    let mut bytes = Vec::new();
    let taken = (&mut entry).take(budget + 1).read_to_end(&mut bytes);
    taken.map_err(|e| format!("{name} cannot be read: {e}"))?;
    let read = bytes.len() as u64;
    if read > budget {
        return Err(format!(
            "the package holds more than {MAX_UNPACKED_BYTES} bytes once unpacked"
        ));
    }
    if read != entry.size() {
        return Err(format!(
            "{name} holds {read} bytes, not the {} its header declares",
            entry.size()
        ));
    }
    *unpacked += read;
    Ok(bytes)
}

/// A source's text exactly as written: UTF-8 with no byte order mark; CR LF is kept.
fn source_text(name: &str, bytes: Vec<u8>) -> Result<String, String> {
    let text = String::from_utf8(bytes).map_err(|_| format!("{name} is not UTF-8 text"))?;
    if text.starts_with('\u{feff}') {
        return Err(format!("{name} begins with a byte order mark"));
    }
    Ok(text)
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// An entry name or a text of the file as it may be put into words: escaped, and cut after 120
/// characters with the length it had.
fn shown(text: &str) -> String {
    const SHOWN_CHARS: usize = 120;
    let escaped: String = text.escape_debug().collect();
    let count = escaped.chars().count();
    if count <= SHOWN_CHARS {
        return escaped;
    }
    let cut: String = escaped.chars().take(SHOWN_CHARS).collect();
    format!("{cut}… ({} bytes)", text.len())
}

// -----------------------------------------------------------------------------
// The manifest
// -----------------------------------------------------------------------------

/// The manifest as a JSON object whose format this reader takes (R1): `format` is read first,
/// as a value, before any other field.
fn manifest_object(bytes: &[u8]) -> Result<Map<String, Value>, String> {
    let value = json_value(bytes)
        .map_err(|why| format!("{MANIFEST_FILE} is not a package manifest: {why}"))?;
    let Value::Object(map) = value else {
        return Err(format!(
            "{MANIFEST_FILE} is not a package manifest: it is not a JSON object"
        ));
    };
    let format = map
        .get("format")
        .and_then(Value::as_u64)
        .filter(|&format| format >= 1)
        .ok_or_else(|| format!("{MANIFEST_FILE} has no format: an integer from 1"))?;
    if format > u64::from(PACKAGE_FORMAT) {
        return Err(format!(
            "the package is of format {format}; this logic module takes format {PACKAGE_FORMAT}"
        ));
    }
    Ok(map)
}

/// Every other field of a format 1 manifest, each refused with words naming it.
fn manifest_of(map: Map<String, Value>) -> Result<Manifest, String> {
    if let Some(key) = map.keys().find(|k| !MANIFEST_KEYS.contains(&k.as_str())) {
        return Err(format!(
            "{MANIFEST_FILE} has the key '{}', which format {PACKAGE_FORMAT} does not have",
            shown(key)
        ));
    }

    let id = required_text(&map, "id")?;
    if let Some(problem) = id_problem(&id) {
        return Err(format!("{MANIFEST_FILE}'s id: {problem}"));
    }
    let name = required_text(&map, "name")?;
    non_blank("name", &name)?;
    let version = required_text(&map, "version")?;
    if let Some(problem) = version_problem("version", &version) {
        return Err(format!("{MANIFEST_FILE}'s {problem}"));
    }
    let form = required_text(&map, "form")?;
    if form != FORM_SOURCE {
        return Err(format!(
            "form '{}' is not taken: only '{FORM_SOURCE}' (a compiled package is ADR-015 step 6)",
            shown(&form)
        ));
    }
    let languages = required_texts(&map, "languages")?;
    if languages != [LANGUAGE_ST] {
        return Err(format!(
            "languages '{}' are not taken: only {LANGUAGE_ST} (LD, FBD and SFC sources are ADR-015 step 4)",
            shown(&languages.join(", "))
        ));
    }
    let files = required_texts(&map, "files")?;
    let mut seen: Vec<&String> = Vec::with_capacity(files.len());
    for file in &files {
        if seen.iter().any(|s| s.eq_ignore_ascii_case(file)) {
            return Err(format!(
                "{MANIFEST_FILE}'s files name {} twice",
                shown(file)
            ));
        }
        seen.push(file);
    }
    let depends = dependencies(&map, &id)?;
    let license = required_text(&map, "license")?;
    non_blank("license", &license)?;
    let authors = optional_texts(&map, "authors")?;
    let description = match map.get("description") {
        None => None,
        Some(Value::String(text)) => Some(text.clone()),
        Some(_) => return Err(not_a(MANIFEST_FILE, "description", "a string")),
    };
    let targets = optional_texts(&map, "targets")?;
    let interface = match map.get("interface") {
        None => None,
        Some(Value::Array(items)) if items.is_empty() => Some(Vec::new()),
        Some(_) => return Err(
            "interface is reserved: only [] is taken (a package's interface comes after format 1)"
                .to_string(),
        ),
    };
    let signature = match map.get("signature") {
        None => None,
        Some(Value::Null) => Some(()),
        Some(_) => return Err(
            "signature is reserved; nothing verifies it before the IEC 62443 work (ADR-015 §2.8)"
                .to_string(),
        ),
    };

    Ok(Manifest {
        format: PACKAGE_FORMAT,
        id,
        name,
        version,
        form,
        languages,
        files,
        depends,
        license,
        authors,
        description,
        targets,
        interface,
        signature,
    })
}

fn not_a(file: &str, field: &str, what: &str) -> String {
    format!("{file}'s {field} is not {what}")
}

fn required_text(map: &Map<String, Value>, field: &str) -> Result<String, String> {
    match map.get(field) {
        None => Err(format!("{MANIFEST_FILE} has no {field}")),
        Some(Value::String(text)) => Ok(text.clone()),
        Some(_) => Err(not_a(MANIFEST_FILE, field, "a string")),
    }
}

fn non_blank(field: &str, text: &str) -> Result<(), String> {
    if text.trim().is_empty() {
        return Err(format!("{MANIFEST_FILE}'s {field} is empty"));
    }
    Ok(())
}

fn texts(field: &str, value: &Value) -> Result<Vec<String>, String> {
    let Value::Array(items) = value else {
        return Err(not_a(MANIFEST_FILE, field, "a list of strings"));
    };
    items
        .iter()
        .map(|item| match item {
            Value::String(text) => Ok(text.clone()),
            _ => Err(not_a(MANIFEST_FILE, field, "a list of strings")),
        })
        .collect()
}

fn required_texts(map: &Map<String, Value>, field: &str) -> Result<Vec<String>, String> {
    match map.get(field) {
        None => Err(format!("{MANIFEST_FILE} has no {field}")),
        Some(value) => texts(field, value),
    }
}

fn optional_texts(map: &Map<String, Value>, field: &str) -> Result<Option<Vec<String>>, String> {
    map.get(field).map(|value| texts(field, value)).transpose()
}

fn dependencies(map: &Map<String, Value>, own_id: &str) -> Result<Vec<Dependency>, String> {
    let Some(value) = map.get("depends") else {
        return Err(format!("{MANIFEST_FILE} has no depends"));
    };
    let Value::Array(items) = value else {
        return Err(not_a(MANIFEST_FILE, "depends", "a list"));
    };
    let mut depends: Vec<Dependency> = Vec::with_capacity(items.len());
    for item in items {
        let Value::Object(entry) = item else {
            return Err(format!(
                "{MANIFEST_FILE}'s depends holds an entry that is not an object of id and version"
            ));
        };
        if let Some(key) = entry.keys().find(|k| *k != "id" && *k != "version") {
            return Err(format!(
                "{MANIFEST_FILE}'s depends has the key '{}', which format {PACKAGE_FORMAT} does not have",
                shown(key)
            ));
        }
        let (Some(Value::String(id)), Some(Value::String(version))) =
            (entry.get("id"), entry.get("version"))
        else {
            return Err(format!(
                "{MANIFEST_FILE}'s depends holds an entry without an id and a version, both strings"
            ));
        };
        if let Some(problem) = id_problem(id) {
            return Err(format!("{MANIFEST_FILE}'s depends: {problem}"));
        }
        if let Some(problem) = version_problem("the version of a dependency", version) {
            return Err(format!("{MANIFEST_FILE}'s depends: {problem}"));
        }
        if id.eq_ignore_ascii_case(own_id) {
            return Err(format!(
                "{MANIFEST_FILE}'s depends names the package itself, {id}"
            ));
        }
        if depends.iter().any(|d| d.id.eq_ignore_ascii_case(id)) {
            return Err(format!("{MANIFEST_FILE}'s depends names {id} twice"));
        }
        depends.push(Dependency {
            id: id.clone(),
            version: version.clone(),
        });
    }
    Ok(depends)
}

// -----------------------------------------------------------------------------
// checksums.json
// -----------------------------------------------------------------------------

/// `checksums.json`: a map of each entry's path to its SHA-256, 64 lowercase hex digits.
fn checksums_of(bytes: &[u8]) -> Result<std::collections::BTreeMap<String, String>, String> {
    let words = || format!("{CHECKSUMS_FILE} is not a map of each file to its SHA-256");
    let value = json_value(bytes).map_err(|why| format!("{}: {why}", words()))?;
    let Value::Object(map) = value else {
        return Err(words());
    };
    let mut checksums = std::collections::BTreeMap::new();
    for (path, sum) in map {
        let Value::String(sum) = sum else {
            return Err(format!(
                "{}: the value of {} is not a string",
                words(),
                shown(&path)
            ));
        };
        let hex_ok = sum.len() == 64
            && sum
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
        if !hex_ok {
            return Err(format!(
                "{}: the value of {} is not 64 lowercase hex digits",
                words(),
                shown(&path)
            ));
        }
        checksums.insert(path, sum);
    }
    Ok(checksums)
}

// -----------------------------------------------------------------------------
// JSON with every key once
// -----------------------------------------------------------------------------

/// Parses `bytes` as JSON refusing a byte order mark and any object that holds a key twice
/// (a JSON reader keeps one of the two silently, and readers differ in which).
fn json_value(bytes: &[u8]) -> Result<Value, String> {
    if bytes.starts_with(b"\xEF\xBB\xBF") {
        return Err("it begins with a byte order mark".to_string());
    }
    serde_json::from_slice::<KeysOnce>(bytes).map_err(|e| e.to_string())?;
    serde_json::from_slice::<Value>(bytes).map_err(|e| e.to_string())
}

/// A JSON value walked only to refuse a key that is there twice.
struct KeysOnce;

impl<'de> Deserialize<'de> for KeysOnce {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(KeysOnceVisitor)
    }
}

struct KeysOnceVisitor;

impl<'de> Visitor<'de> for KeysOnceVisitor {
    type Value = KeysOnce;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a JSON value")
    }

    fn visit_bool<E: de::Error>(self, _: bool) -> Result<KeysOnce, E> {
        Ok(KeysOnce)
    }

    fn visit_i64<E: de::Error>(self, _: i64) -> Result<KeysOnce, E> {
        Ok(KeysOnce)
    }

    fn visit_u64<E: de::Error>(self, _: u64) -> Result<KeysOnce, E> {
        Ok(KeysOnce)
    }

    fn visit_f64<E: de::Error>(self, _: f64) -> Result<KeysOnce, E> {
        Ok(KeysOnce)
    }

    fn visit_str<E: de::Error>(self, _: &str) -> Result<KeysOnce, E> {
        Ok(KeysOnce)
    }

    fn visit_unit<E: de::Error>(self) -> Result<KeysOnce, E> {
        Ok(KeysOnce)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<KeysOnce, A::Error> {
        while seq.next_element::<KeysOnce>()?.is_some() {}
        Ok(KeysOnce)
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<KeysOnce, A::Error> {
        let mut keys: HashSet<String> = HashSet::new();
        while let Some(key) = map.next_key::<String>()? {
            if keys.contains(&key) {
                return Err(de::Error::custom(format!(
                    "the key '{}' is there twice",
                    shown(&key)
                )));
            }
            map.next_value::<KeysOnce>()?;
            keys.insert(key);
        }
        Ok(KeysOnce)
    }
}

// -----------------------------------------------------------------------------
// The zip's structure, walked before the zip crate is given the file
// -----------------------------------------------------------------------------

const LOCAL_HEADER_SIGNATURE: u32 = 0x0403_4b50;
const CENTRAL_HEADER_SIGNATURE: u32 = 0x0201_4b50;
const END_SIGNATURE: u32 = 0x0605_4b50;
const ZIP64_LOCATOR_SIGNATURE: u32 = 0x0706_4b50;
const DATA_DESCRIPTOR_SIGNATURE: u32 = 0x0807_4b50;
const LOCAL_HEADER_LEN: usize = 30;
const CENTRAL_HEADER_LEN: usize = 46;
const END_LEN: usize = 22;
const ZIP64_LOCATOR_LEN: usize = 20;
/// A 32-bit size or offset of this value means "see the zip64 record".
const ZIP64_MARK: u32 = u32::MAX;
const ZIP64_EXTRA_ID: u16 = 0x0001;
const AES_EXTRA_ID: u16 = 0x9901;
const FLAG_ENCRYPTED: u16 = 1;
const FLAG_DATA_DESCRIPTOR: u16 = 1 << 3;

/// One entry of the central directory, as far as [`read`] needs it before the zip crate.
struct CentralEntry {
    name: Vec<u8>,
}

fn not_zip(why: &str) -> String {
    format!("it is not a zip file: {why}")
}

fn outside(why: &str) -> String {
    format!("the file holds bytes that are not part of the zip: {why}")
}

fn u16_at(bytes: &[u8], at: usize) -> Option<u16> {
    let end = at.checked_add(2)?;
    bytes.get(at..end).map(|b| u16::from_le_bytes([b[0], b[1]]))
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    let end = at.checked_add(4)?;
    bytes
        .get(at..end)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// Walks the zip's end record, central directory and local headers, refusing anything a plain
/// zip of at most [`MAX_PACKAGE_ENTRIES`] entries with no byte outside them would not hold: a
/// comment, bytes before the first entry, between entries or after the end record, a zip64
/// record, more than one disk, entries that overlap, local headers that disagree with the
/// central directory. The zip crate is given the file only after this, so it never allocates by
/// a count the file declares beyond the bound and never meets an offset outside the file.
fn zip_layout(file: &[u8]) -> Result<Vec<CentralEntry>, String> {
    let truncated = || not_zip("it ends inside its own structure");
    if file.len() < END_LEN {
        return Err(not_zip("it is shorter than the end record of a zip"));
    }
    let end = file.len() - END_LEN;
    if u32_at(file, end) != Some(END_SIGNATURE) {
        // A zip whose end record is not the last 22 bytes holds a comment or bytes after it.
        let lowest = end.saturating_sub(usize::from(u16::MAX));
        let found = (lowest..end)
            .rev()
            .find(|&at| u32_at(file, at) == Some(END_SIGNATURE));
        return Err(match found {
            Some(at) => outside(&format!(
                "{} bytes after the end of its central directory (a comment or bytes appended)",
                file.len() - at - END_LEN
            )),
            None => not_zip("it has no end of central directory record"),
        });
    }
    let field16 = |at: usize| u16_at(file, end + at).ok_or_else(truncated);
    let field32 = |at: usize| u32_at(file, end + at).ok_or_else(truncated);
    if field16(20)? != 0 {
        return Err(not_zip(
            "its end record declares a comment that runs past the end of the file",
        ));
    }
    if field16(4)? != 0 || field16(6)? != 0 || field16(8)? != field16(10)? {
        return Err(not_zip("a zip of more than one disk is not taken"));
    }
    let count = usize::from(field16(10)?);
    let directory_size = field32(12)?;
    let directory_offset = field32(16)?;
    let zip64_locator = end
        .checked_sub(ZIP64_LOCATOR_LEN)
        .is_some_and(|at| u32_at(file, at) == Some(ZIP64_LOCATOR_SIGNATURE));
    if count == usize::from(u16::MAX)
        || directory_size == ZIP64_MARK
        || directory_offset == ZIP64_MARK
        || zip64_locator
    {
        return Err(not_zip(&format!(
            "it is a zip64 file; a package of at most {MAX_PACKAGE_BYTES} bytes is a plain zip"
        )));
    }
    if count > MAX_PACKAGE_ENTRIES {
        return Err(format!(
            "the package holds {count} entries; at most {MAX_PACKAGE_ENTRIES} are taken"
        ));
    }
    let directory_start = directory_offset as usize;
    let directory_end = directory_start
        .checked_add(directory_size as usize)
        .ok_or_else(truncated)?;
    if directory_end > end {
        return Err(not_zip("its central directory runs past its end record"));
    }
    if directory_end < end {
        return Err(outside(&format!(
            "{} bytes before its first entry or between its central directory and its end record",
            end - directory_end
        )));
    }

    let mut entries: Vec<CentralEntry> = Vec::with_capacity(count);
    // (where the entry's local header begins, where its data and descriptor end)
    let mut spans: Vec<(usize, usize)> = Vec::with_capacity(count);
    let mut at = directory_start;
    for _ in 0..count {
        let header = file.get(at..directory_end).ok_or_else(truncated)?;
        let c16 = |offset: usize| u16_at(header, offset).ok_or_else(truncated);
        let c32 = |offset: usize| u32_at(header, offset).ok_or_else(truncated);
        if c32(0)? != CENTRAL_HEADER_SIGNATURE {
            return Err(not_zip(
                "an entry of its central directory has no signature",
            ));
        }
        let flags = c16(8)?;
        let method = c16(10)?;
        let crc = c32(16)?;
        let compressed = c32(20)?;
        let size = c32(24)?;
        let name_len = usize::from(c16(28)?);
        let extra_len = usize::from(c16(30)?);
        let comment_len = usize::from(c16(32)?);
        let disk = c16(34)?;
        let local = c32(42)?;
        let name_end = CENTRAL_HEADER_LEN + name_len;
        let extra_end = name_end + extra_len;
        let entry_len = extra_end + comment_len;
        let name = header
            .get(CENTRAL_HEADER_LEN..name_end)
            .ok_or_else(truncated)?
            .to_vec();
        let extra = header.get(name_end..extra_end).ok_or_else(truncated)?;
        if header.len() < entry_len {
            return Err(truncated());
        }
        let shown_name = shown(&String::from_utf8_lossy(&name));
        if comment_len != 0 {
            return Err(outside(&format!("{shown_name} carries a comment")));
        }
        if disk != 0 {
            return Err(not_zip("a zip of more than one disk is not taken"));
        }
        if compressed == ZIP64_MARK || size == ZIP64_MARK || local == ZIP64_MARK {
            return Err(not_zip(&format!("{shown_name} carries zip64 sizes")));
        }
        let mut field = 0;
        while field < extra.len() {
            let id = u16_at(extra, field).ok_or_else(truncated)?;
            let len = usize::from(u16_at(extra, field + 2).ok_or_else(truncated)?);
            field += 4 + len;
            if field > extra.len() {
                return Err(not_zip(&format!(
                    "the extra field of {shown_name} runs past its end"
                )));
            }
            if id == ZIP64_EXTRA_ID {
                return Err(not_zip(&format!("{shown_name} carries zip64 sizes")));
            }
            if id == AES_EXTRA_ID && flags & FLAG_ENCRYPTED == 0 {
                return Err(format!(
                    "{shown_name} declares AES encryption; a package holds files that are not encrypted"
                ));
            }
        }

        // Its local header: at its offset, the same name, method and encryption, and (when no
        // data descriptor follows) the same checksum and sizes.
        let local = local as usize;
        let no_local = || not_zip(&format!("{shown_name} has no local header at its offset"));
        if !local
            .checked_add(LOCAL_HEADER_LEN)
            .is_some_and(|header_end| header_end <= directory_start)
        {
            return Err(no_local());
        }
        // `local` lies before the central directory, so these offsets stay inside the file.
        let l16 = |offset: usize| u16_at(file, local + offset);
        let l32 = |offset: usize| u32_at(file, local + offset);
        let disagree = || {
            not_zip(&format!(
                "the local header of {shown_name} disagrees with its central directory"
            ))
        };
        if l32(0) != Some(LOCAL_HEADER_SIGNATURE) {
            return Err(no_local());
        }
        let local_flags = l16(6).ok_or_else(truncated)?;
        let local_name_len = usize::from(l16(26).ok_or_else(truncated)?);
        let local_extra_len = usize::from(l16(28).ok_or_else(truncated)?);
        let local_name_start = local + LOCAL_HEADER_LEN;
        let data_start = local_name_start + local_name_len + local_extra_len;
        if file.get(local_name_start..local_name_start + local_name_len) != Some(&name[..])
            || l16(8) != Some(method)
            || local_flags & (FLAG_ENCRYPTED | FLAG_DATA_DESCRIPTOR)
                != flags & (FLAG_ENCRYPTED | FLAG_DATA_DESCRIPTOR)
        {
            return Err(disagree());
        }
        if flags & FLAG_DATA_DESCRIPTOR == 0
            && (l32(14) != Some(crc) || l32(18) != Some(compressed) || l32(22) != Some(size))
        {
            return Err(disagree());
        }
        let data_end = data_start
            .checked_add(compressed as usize)
            .ok_or_else(truncated)?;
        let span_end = if flags & FLAG_DATA_DESCRIPTOR == 0 {
            data_end
        } else if u32_at(file, data_end) == Some(DATA_DESCRIPTOR_SIGNATURE) {
            data_end.saturating_add(16)
        } else {
            data_end.saturating_add(12)
        };
        if span_end > directory_start {
            return Err(not_zip(&format!(
                "the data of {shown_name} runs into its central directory"
            )));
        }
        spans.push((local, span_end));
        entries.push(CentralEntry { name });
        at += entry_len;
    }
    if at != directory_end {
        return Err(outside(
            "bytes inside its central directory after its last entry",
        ));
    }

    // The entries, in the order they lie, cover every byte before the central directory once.
    spans.sort_unstable();
    let mut next = 0;
    for (start, span_end) in spans {
        if start < next {
            return Err(not_zip("two of its entries overlap"));
        }
        if start > next {
            return Err(outside(&format!(
                "{} bytes before an entry, at byte {next}",
                start - next
            )));
        }
        next = span_end;
    }
    if next != directory_start {
        return Err(outside(&format!(
            "{} bytes between its last entry and its central directory",
            directory_start - next
        )));
    }
    Ok(entries)
}
