//! Reading a library package file (ADR-015 step 1, the P1 contract R1 to R9 and §9 items 6 to
//! 8). Every zip here is built in memory with the zip crate's writer; nothing touches a disk.

use std::io::{Cursor, Write};

use mythos_logic_package::{
    read, MAX_PACKAGE_BYTES, MAX_PACKAGE_ENTRIES, MAX_PACKAGE_ID_LEN, MAX_UNPACKED_BYTES,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use zip::write::FileOptions;
use zip::{CompressionMethod, ZipWriter};

// -----------------------------------------------------------------------------
// Building packages
// -----------------------------------------------------------------------------

const MOTOR: &str = "FUNCTION_BLOCK Motor\nVAR_INPUT start : BOOL; END_VAR\nEND_FUNCTION_BLOCK\n";
const RAMP: &str = "FUNCTION Ramp : INT\nVAR_INPUT x : INT; END_VAR\nRamp := x;\nEND_FUNCTION\n";

fn sha(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn deflated() -> FileOptions {
    FileOptions::default().compression_method(CompressionMethod::Deflated)
}

/// A zip of exactly these entries, in this order.
fn zip_of(entries: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in entries {
        if name.ends_with('/') {
            writer.add_directory(name.as_str(), deflated()).unwrap();
        } else {
            writer.start_file(name.as_str(), deflated()).unwrap();
            writer.write_all(bytes).unwrap();
        }
    }
    writer.finish().unwrap().into_inner()
}

/// `checksums.json` listing every given entry that is a file.
fn checksums_for(entries: &[(String, Vec<u8>)]) -> Vec<u8> {
    let map: serde_json::Map<String, Value> = entries
        .iter()
        .filter(|(name, _)| !name.ends_with('/'))
        .map(|(name, bytes)| (name.clone(), json!(sha(bytes))))
        .collect();
    serde_json::to_vec(&Value::Object(map)).unwrap()
}

/// These entries, then a `checksums.json` that lists every one of them.
fn with_checksums(mut entries: Vec<(String, Vec<u8>)>) -> Vec<u8> {
    let checksums = checksums_for(&entries);
    entries.push(("checksums.json".to_string(), checksums));
    zip_of(&entries)
}

fn entry(name: &str, bytes: &[u8]) -> (String, Vec<u8>) {
    (name.to_string(), bytes.to_vec())
}

fn manifest_for(files: &[&str]) -> Value {
    json!({
        "format": 1, "id": "org.example.motors", "name": "Motor blocks", "version": "1.2.0",
        "form": "source", "languages": ["st"], "files": files,
        "depends": [{ "id": "org.example.base", "version": "1.0.0" }], "license": "MIT",
        "authors": ["Example GmbH"], "description": "Start/stop and ramp blocks", "targets": ["pc"]
    })
}

fn manifest() -> Value {
    manifest_for(&["src/motor.st", "src/ramp.st"])
}

/// A package of this manifest and the two sources, checksums right.
fn package_with(manifest: &Value) -> Vec<u8> {
    with_checksums(vec![
        entry("manifest.json", &serde_json::to_vec(manifest).unwrap()),
        entry("src/", b""),
        entry("src/ramp.st", RAMP.as_bytes()),
        entry("src/motor.st", MOTOR.as_bytes()),
    ])
}

fn valid() -> Vec<u8> {
    package_with(&manifest())
}

/// A package whose manifest is changed by `change`.
fn with_manifest(change: impl FnOnce(&mut serde_json::Map<String, Value>)) -> Vec<u8> {
    let mut m = manifest();
    change(m.as_object_mut().unwrap());
    package_with(&m)
}

fn refused(file: &[u8]) -> String {
    match read(file) {
        Ok(package) => panic!("taken, expected a refusal: {package:?}"),
        Err(words) => words,
    }
}

/// The offsets of the local and the central header of the entry `name`.
fn headers_of(zip: &[u8], name: &str) -> (usize, usize) {
    let name = name.as_bytes();
    let at = |signature: &[u8; 4], name_at: usize| {
        (0..zip.len())
            .find(|&i| {
                zip[i..].starts_with(signature)
                    && zip.get(i + name_at..i + name_at + name.len()) == Some(name)
            })
            .unwrap()
    };
    (at(b"PK\x03\x04", 30), at(b"PK\x01\x02", 46))
}

/// Writes `value` into the local (at `local_offset`) and the central (at `central_offset`)
/// header of the entry `name`.
fn patch(zip: &mut [u8], name: &str, local_offset: usize, central_offset: usize, value: &[u8]) {
    let (local, central) = headers_of(zip, name);
    zip[local + local_offset..local + local_offset + value.len()].copy_from_slice(value);
    zip[central + central_offset..central + central_offset + value.len()].copy_from_slice(value);
}

// -----------------------------------------------------------------------------
// R9: a valid package is read whole
// -----------------------------------------------------------------------------

#[test]
fn a_valid_package_is_read_whole_with_its_sources_in_byte_order() {
    let file = valid();
    let package = read(&file).unwrap();
    assert_eq!(
        package.sources,
        vec![
            ("src/motor.st".to_string(), MOTOR.to_string()),
            ("src/ramp.st".to_string(), RAMP.to_string()),
        ]
    );
    assert_eq!(package.size, file.len() as u64);
    let m = &package.manifest;
    assert_eq!(m.format, 1);
    assert_eq!(m.id, "org.example.motors");
    assert_eq!(m.name, "Motor blocks");
    assert_eq!(m.version, "1.2.0");
    assert_eq!(m.form, "source");
    assert_eq!(m.languages, vec!["st"]);
    assert_eq!(m.files, vec!["src/motor.st", "src/ramp.st"]);
    assert_eq!(m.depends.len(), 1);
    assert_eq!(m.depends[0].id, "org.example.base");
    assert_eq!(m.depends[0].version, "1.0.0");
    assert_eq!(m.license, "MIT");
    assert_eq!(m.authors, Some(vec!["Example GmbH".to_string()]));
    assert_eq!(m.description.as_deref(), Some("Start/stop and ramp blocks"));
    assert_eq!(m.targets, Some(vec!["pc".to_string()]));
    assert_eq!(serde_json::to_value(m).unwrap(), manifest());
}

#[test]
fn an_absent_optional_field_is_answered_absent_and_serialised_absent() {
    let file = with_manifest(|m| {
        m.remove("authors");
        m.remove("description");
        m.remove("targets");
    });
    let manifest = read(&file).unwrap().manifest;
    assert_eq!(manifest.authors, None);
    assert_eq!(manifest.description, None);
    assert_eq!(manifest.targets, None);
    let value = serde_json::to_value(&manifest).unwrap();
    for key in [
        "authors",
        "description",
        "targets",
        "interface",
        "signature",
    ] {
        assert!(value.get(key).is_none(), "{key} was filled: {value}");
    }
}

#[test]
fn the_reserved_interface_and_signature_are_taken_empty_and_answered_as_given() {
    let file = with_manifest(|m| {
        m.insert("interface".into(), json!([]));
        m.insert("signature".into(), Value::Null);
    });
    let manifest = read(&file).unwrap().manifest;
    let value = serde_json::to_value(&manifest).unwrap();
    assert_eq!(value["interface"], json!([]));
    assert_eq!(value.get("signature"), Some(&Value::Null));
}

// -----------------------------------------------------------------------------
// R1: format first
// -----------------------------------------------------------------------------

#[test]
fn a_package_of_a_newer_format_is_refused_naming_both_numbers() {
    let file = with_manifest(|m| {
        m.insert("format".into(), json!(2));
    });
    assert_eq!(
        refused(&file),
        "the package is of format 2; this logic module takes format 1"
    );
}

#[test]
fn a_package_of_a_newer_format_is_refused_by_its_format_before_its_entries() {
    // A newer format may hold entries format 1 does not know.
    let file = with_checksums(vec![
        entry(
            "manifest.json",
            &serde_json::to_vec(&json!({"format": 2})).unwrap(),
        ),
        entry("LICENSE", b"MIT"),
    ]);
    assert_eq!(
        refused(&file),
        "the package is of format 2; this logic module takes format 1"
    );
}

#[test]
fn a_manifest_with_no_format_is_refused() {
    for format in [
        None,
        Some(json!("1")),
        Some(json!(0)),
        Some(json!(1.5)),
        Some(json!(-1)),
    ] {
        let file = with_manifest(|m| match format {
            None => {
                m.remove("format");
            }
            Some(value) => {
                m.insert("format".into(), value);
            }
        });
        assert_eq!(
            refused(&file),
            "manifest.json has no format: an integer from 1"
        );
    }
}

// -----------------------------------------------------------------------------
// R2: every other field refused naming it
// -----------------------------------------------------------------------------

#[test]
fn a_manifest_that_is_not_json_is_refused() {
    let file = with_checksums(vec![
        entry("manifest.json", b"{ not json"),
        entry("src/motor.st", MOTOR.as_bytes()),
    ]);
    assert!(refused(&file).starts_with("manifest.json is not a package manifest: "));
}

#[test]
fn a_manifest_holding_a_key_twice_is_refused() {
    let text = r#"{"format": 1, "id": "org.example.motors", "id": "org.example.other"}"#;
    let file = with_checksums(vec![entry("manifest.json", text.as_bytes())]);
    let words = refused(&file);
    assert!(
        words.starts_with("manifest.json is not a package manifest: the key 'id' is there twice"),
        "{words}"
    );
}

#[test]
fn a_manifest_with_an_unknown_key_is_refused_naming_the_key() {
    let file = with_manifest(|m| {
        m.insert("homepage".into(), json!("https://example.org"));
    });
    assert_eq!(
        refused(&file),
        "manifest.json has the key 'homepage', which format 1 does not have"
    );
}

#[test]
fn a_manifest_whose_form_is_not_source_is_refused_naming_the_field() {
    let file = with_manifest(|m| {
        m.insert("form".into(), json!("compiled"));
    });
    assert_eq!(
        refused(&file),
        "form 'compiled' is not taken: only 'source' (a compiled package is ADR-015 step 6)"
    );
}

#[test]
fn a_manifest_whose_languages_are_not_st_is_refused_naming_the_field() {
    let file = with_manifest(|m| {
        m.insert("languages".into(), json!(["st", "ld"]));
    });
    assert_eq!(
        refused(&file),
        "languages 'st, ld' are not taken: only st (LD, FBD and SFC sources are ADR-015 step 4)"
    );
}

#[test]
fn a_manifest_whose_interface_is_not_empty_is_refused_naming_the_field() {
    let file = with_manifest(|m| {
        m.insert("interface".into(), json!(["Motor"]));
    });
    assert!(refused(&file).starts_with("interface is reserved"));
}

#[test]
fn a_manifest_whose_signature_is_not_null_is_refused_naming_the_field() {
    let file = with_manifest(|m| {
        m.insert("signature".into(), json!("abc"));
    });
    assert_eq!(
        refused(&file),
        "signature is reserved; nothing verifies it before the IEC 62443 work (ADR-015 §2.8)"
    );
}

#[test]
fn a_manifest_whose_required_field_is_missing_is_refused_naming_the_field() {
    for field in [
        "id",
        "name",
        "version",
        "form",
        "languages",
        "files",
        "depends",
        "license",
    ] {
        let file = with_manifest(|m| {
            m.remove(field);
        });
        assert_eq!(refused(&file), format!("manifest.json has no {field}"));
    }
}

#[test]
fn a_manifest_whose_field_is_of_the_wrong_kind_is_refused_naming_the_field() {
    let cases = [
        ("id", json!(7), "manifest.json's id is not a string"),
        ("name", json!(["x"]), "manifest.json's name is not a string"),
        (
            "files",
            json!("src/motor.st"),
            "manifest.json's files is not a list of strings",
        ),
        (
            "languages",
            json!([1]),
            "manifest.json's languages is not a list of strings",
        ),
        (
            "depends",
            json!({}),
            "manifest.json's depends is not a list",
        ),
        (
            "authors",
            json!("Example"),
            "manifest.json's authors is not a list of strings",
        ),
        (
            "description",
            json!(1),
            "manifest.json's description is not a string",
        ),
        (
            "targets",
            json!([true]),
            "manifest.json's targets is not a list of strings",
        ),
    ];
    for (field, value, words) in cases {
        let file = with_manifest(|m| {
            m.insert(field.into(), value);
        });
        assert_eq!(refused(&file), words, "{field}");
    }
}

#[test]
fn a_manifest_whose_name_or_license_is_empty_is_refused_naming_the_field() {
    for field in ["name", "license"] {
        let file = with_manifest(|m| {
            m.insert(field.into(), json!("  "));
        });
        assert_eq!(refused(&file), format!("manifest.json's {field} is empty"));
    }
}

#[test]
fn a_manifest_whose_depends_is_wrong_is_refused_naming_the_field() {
    let cases = [
        (
            json!([{"id": "org.example.base"}]),
            "without an id and a version",
        ),
        (
            json!([{"id": "org.example.base", "version": "1.0.0", "range": "^1"}]),
            "has the key 'range'",
        ),
        (
            json!([{"id": "base", "version": "1.0.0"}]),
            "is not a package id",
        ),
        (
            json!([{"id": "org.example.base", "version": "1.0"}]),
            "is not a version",
        ),
        (
            json!([{"id": "ORG.example.MOTORS", "version": "1.0.0"}]),
            "names the package itself",
        ),
        (
            json!([{"id": "org.a.b", "version": "1.0.0"}, {"id": "org.A.B", "version": "2.0.0"}]),
            "names org.A.B twice",
        ),
    ];
    for (depends, words) in cases {
        let file = with_manifest(|m| {
            m.insert("depends".into(), depends);
        });
        let refusal = refused(&file);
        assert!(refusal.starts_with("manifest.json's depends"), "{refusal}");
        assert!(refusal.contains(words), "{refusal}");
    }
}

#[test]
fn a_manifest_with_no_dependency_is_taken() {
    let file = with_manifest(|m| {
        m.insert("depends".into(), json!([]));
    });
    assert!(read(&file).unwrap().manifest.depends.is_empty());
}

// -----------------------------------------------------------------------------
// R3: id and version
// -----------------------------------------------------------------------------

fn with_id(id: &str) -> Vec<u8> {
    with_manifest(|m| {
        m.insert("id".into(), json!(id));
    })
}

#[test]
fn an_id_of_one_part_is_refused() {
    let words = refused(&with_id("motors"));
    assert!(
        words.starts_with("manifest.json's id: 'motors' is not a package id: two or more parts"),
        "{words}"
    );
}

#[test]
fn an_id_with_a_part_named_con_is_refused() {
    let words = refused(&with_id("org.con.motors"));
    assert_eq!(
        words,
        "manifest.json's id: 'org.con.motors' is not a package id: its part 'con' is a device name on Windows (CON, PRN, AUX, NUL, COM0 to COM9, LPT0 to LPT9)"
    );
}

#[test]
fn an_id_with_a_letter_that_is_not_ascii_is_refused_saying_why() {
    let words = refused(&with_id("org.exämple.motors"));
    assert!(words.contains("ASCII letter"), "{words}");
    assert!(words.contains("directory name on every OS"), "{words}");
}

#[test]
fn an_id_with_an_empty_part_or_a_leading_digit_is_refused() {
    for id in [
        "org..motors",
        "org.example.",
        ".org.example",
        "org.1example",
        "org.ex-ample",
    ] {
        assert!(
            refused(&with_id(id)).contains("is not a package id"),
            "{id}"
        );
    }
}

#[test]
fn an_id_longer_than_64_characters_is_refused_stating_64() {
    let long = format!("org.{}", "a".repeat(MAX_PACKAGE_ID_LEN - 3));
    assert_eq!(long.len(), MAX_PACKAGE_ID_LEN + 1);
    assert_eq!(
        refused(&with_id(&long)),
        "manifest.json's id: a package id is at most 64 characters; this one is 65"
    );
    let longest = format!("org.{}", "a".repeat(MAX_PACKAGE_ID_LEN - 4));
    assert_eq!(read(&with_id(&longest)).unwrap().manifest.id, longest);
}

fn with_version(version: &str) -> Vec<u8> {
    with_manifest(|m| {
        m.insert("version".into(), json!(version));
    })
}

#[test]
fn a_version_with_build_metadata_is_refused() {
    let words = refused(&with_version("1.2.0+build.5"));
    assert!(
        words.starts_with("manifest.json's version '1.2.0+build.5' is not a version"),
        "{words}"
    );
    assert!(
        words.contains("no + (build metadata is not taken)"),
        "{words}"
    );
}

#[test]
fn a_version_that_is_not_major_minor_patch_is_refused() {
    for version in [
        "1.2",
        "1.2.3.4",
        "01.2.3",
        "1.02.3",
        "1.2.3-",
        "1.2.3-a..b",
        "1.2.3-a_b",
        "v1.2.3",
        "",
    ] {
        assert!(
            refused(&with_version(version)).contains("is not a version"),
            "{version}"
        );
    }
}

#[test]
fn a_version_with_a_pre_release_is_taken() {
    for version in ["0.0.0", "1.2.3-rc.1", "10.20.30-alpha-2.x"] {
        assert_eq!(
            read(&with_version(version)).unwrap().manifest.version,
            version
        );
    }
}

#[test]
fn a_version_longer_than_32_characters_is_refused_stating_32() {
    let version = format!("1.2.3-{}", "a".repeat(27));
    assert_eq!(version.len(), 33);
    assert_eq!(
        refused(&with_version(&version)),
        "manifest.json's version is at most 32 characters; this one is 33"
    );
}

// -----------------------------------------------------------------------------
// R4: the bounds of the file
// -----------------------------------------------------------------------------

#[test]
fn a_file_over_the_bound_is_refused_stating_the_bound() {
    let file = vec![0u8; MAX_PACKAGE_BYTES + 1];
    assert_eq!(
        refused(&file),
        "the package is 1048577 bytes; at most 1048576 are taken"
    );
}

#[test]
fn a_file_that_is_not_a_zip_is_refused() {
    let words = refused(b"FUNCTION_BLOCK Motor END_FUNCTION_BLOCK, not a zip at all");
    assert!(words.starts_with("it is not a zip file: "), "{words}");
    assert!(refused(b"").starts_with("it is not a zip file: "));
}

#[test]
fn a_package_that_expands_past_the_bound_is_refused_and_not_read_further() {
    // A deflated run of zeros past the bound, whose checksum is wrong: had it been read to its
    // end, the zip crate would have refused the checksum instead.
    let zeros = vec![0u8; MAX_UNPACKED_BYTES as usize + 1_000_000];
    let mut file = with_checksums(vec![
        entry("src/zeros.st", &zeros),
        entry(
            "manifest.json",
            &serde_json::to_vec(&manifest_for(&["src/zeros.st"])).unwrap(),
        ),
    ]);
    assert!(file.len() < MAX_PACKAGE_BYTES);
    patch(&mut file, "src/zeros.st", 14, 16, &[1, 2, 3, 4]);
    assert_eq!(
        refused(&file),
        "the package holds more than 2097152 bytes once unpacked"
    );
}

#[test]
fn a_package_of_exactly_the_unpacked_bound_is_read_and_one_byte_more_is_refused() {
    let build = |extra: usize| {
        // The manifest and checksums are of fixed length, so the source fills what is left.
        let manifest = serde_json::to_vec(&manifest_for(&["src/big.st"])).unwrap();
        let checksums_len =
            checksums_for(&[entry("manifest.json", &manifest), entry("src/big.st", b"")]).len();
        let source_len = MAX_UNPACKED_BYTES as usize - manifest.len() - checksums_len + extra;
        with_checksums(vec![
            entry("manifest.json", &manifest),
            entry("src/big.st", &vec![b' '; source_len]),
        ])
    };
    let package = read(&build(0)).unwrap();
    assert_eq!(package.sources.len(), 1);
    assert_eq!(
        refused(&build(1)),
        "the package holds more than 2097152 bytes once unpacked"
    );
}

#[test]
fn a_package_of_more_entries_than_the_bound_is_refused() {
    let entries: Vec<(String, Vec<u8>)> = (0..=MAX_PACKAGE_ENTRIES)
        .map(|i| (format!("src/s{i}.st"), Vec::new()))
        .collect();
    let file = zip_of(&entries);
    assert_eq!(
        refused(&file),
        "the package holds 1025 entries; at most 1024 are taken"
    );
}

#[test]
fn a_package_of_exactly_the_bound_of_entries_is_read() {
    let names: Vec<String> = (0..MAX_PACKAGE_ENTRIES - 2)
        .map(|i| format!("src/s{i}.st"))
        .collect();
    let files: Vec<&str> = names.iter().map(String::as_str).collect();
    let mut entries = vec![entry(
        "manifest.json",
        &serde_json::to_vec(&manifest_for(&files)).unwrap(),
    )];
    entries.extend(names.iter().map(|name| entry(name, b"")));
    let package = read(&with_checksums(entries)).unwrap();
    assert_eq!(package.sources.len(), MAX_PACKAGE_ENTRIES - 2);
}

// -----------------------------------------------------------------------------
// §9 item 7: no damaged file makes it panic or read past a bound
// -----------------------------------------------------------------------------

#[test]
fn every_prefix_of_a_valid_package_is_refused_and_none_panics() {
    let file = valid();
    read(&file).unwrap();
    for length in 0..file.len() {
        assert!(
            read(&file[..length]).is_err(),
            "a prefix of {length} bytes was taken"
        );
    }
}

#[test]
fn a_package_whose_entry_lies_about_its_size_is_refused_by_the_bytes_read() {
    // Declares 16 bytes and holds more than the bound: refused by the bound, as read.
    let zeros = vec![0u8; MAX_UNPACKED_BYTES as usize + 1];
    let mut file = with_checksums(vec![
        entry("src/zeros.st", &zeros),
        entry(
            "manifest.json",
            &serde_json::to_vec(&manifest_for(&["src/zeros.st"])).unwrap(),
        ),
    ]);
    patch(&mut file, "src/zeros.st", 22, 24, &16u32.to_le_bytes());
    assert_eq!(
        refused(&file),
        "the package holds more than 2097152 bytes once unpacked"
    );

    // Declares 2 GiB and holds a few bytes: nothing is allocated by the declaration, and the
    // entry is refused by what it holds.
    let mut file = valid();
    patch(
        &mut file,
        "src/motor.st",
        22,
        24,
        &0x7FFF_FFF0u32.to_le_bytes(),
    );
    assert_eq!(
        refused(&file),
        format!(
            "src/motor.st holds {} bytes, not the 2147483632 its header declares",
            MOTOR.len()
        )
    );
}

// -----------------------------------------------------------------------------
// §9 item 8: what is hashed is what is read
// -----------------------------------------------------------------------------

#[test]
fn a_file_with_bytes_before_the_first_entry_is_refused() {
    let file = [b"MZ junk ".to_vec(), valid()].concat();
    assert!(refused(&file).starts_with("the file holds bytes that are not part of the zip"));
}

#[test]
fn a_file_with_bytes_after_the_end_of_the_central_directory_is_refused() {
    let file = [valid(), b"appended".to_vec()].concat();
    assert!(refused(&file).starts_with("the file holds bytes that are not part of the zip"));
}

#[test]
fn a_zip_comment_is_refused() {
    let entries = vec![entry(
        "manifest.json",
        &serde_json::to_vec(&manifest_for(&[])).unwrap(),
    )];
    let checksums = checksums_for(&entries);
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in entries.iter().chain([&entry("checksums.json", &checksums)]) {
        writer.start_file(name.as_str(), deflated()).unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.set_comment("a comment");
    let file = writer.finish().unwrap().into_inner();
    assert!(refused(&file).starts_with("the file holds bytes that are not part of the zip"));
}

// -----------------------------------------------------------------------------
// R5: entry names
// -----------------------------------------------------------------------------

fn with_entry(name: &str) -> Vec<u8> {
    with_checksums(vec![
        entry("manifest.json", &serde_json::to_vec(&manifest()).unwrap()),
        entry("src/motor.st", MOTOR.as_bytes()),
        entry("src/ramp.st", RAMP.as_bytes()),
        entry(name, b"x"),
    ])
}

fn not_a_package_file(name: &str) -> String {
    format!(
        "{name} is not a file a package may hold: manifest.json, checksums.json and src/<name>.st"
    )
}

#[test]
fn a_source_named_con_st_inside_a_package_is_refused() {
    for name in ["src/CON.st", "src/con.st", "src/Lpt1.st"] {
        let stem = &name[4..name.len() - 3];
        assert_eq!(
            refused(&with_entry(name)),
            format!(
                "{}; '{stem}' cannot be a library source name: it is a device name on Windows (CON, PRN, AUX, NUL, COM0 to COM9, LPT0 to LPT9)",
                not_a_package_file(name)
            )
        );
    }
}

#[test]
fn an_entry_that_climbs_out_with_dot_dot_is_refused() {
    for name in ["../x.st", "src/../x.st", "src/../../etc/passwd"] {
        assert!(
            refused(&with_entry(name)).starts_with(&not_a_package_file(name)),
            "{name}"
        );
    }
}

#[test]
fn an_entry_with_an_absolute_path_is_refused() {
    let name = "/src/x.st";
    assert!(refused(&with_entry(name)).starts_with(&not_a_package_file(name)));
}

#[test]
fn an_entry_with_a_backslash_is_refused() {
    let name = "src\\x.st";
    assert!(refused(&with_entry(name)).starts_with(&not_a_package_file("src\\\\x.st")));
}

#[test]
fn an_entry_with_a_drive_letter_is_refused() {
    let name = "C:/src/x.st";
    assert!(refused(&with_entry(name)).starts_with(&not_a_package_file(name)));
}

#[test]
fn a_source_in_a_nested_folder_is_refused() {
    let name = "src/lib/x.st";
    assert!(refused(&with_entry(name)).starts_with(&not_a_package_file(name)));
}

#[test]
fn a_source_whose_extension_is_not_exactly_st_is_refused() {
    for name in [
        "src/x.ST",
        "src/x.St",
        "src/x.st.bak",
        "src/x",
        "src/.st",
        "src/1x.st",
    ] {
        assert!(
            refused(&with_entry(name)).starts_with(&not_a_package_file(name)),
            "{name}"
        );
    }
}

#[test]
fn a_file_other_than_the_three_kinds_is_refused() {
    for name in [
        "LICENSE",
        "README.md",
        "Manifest.json",
        "SRC/x.st",
        "lib/",
        "src/sub/",
    ] {
        assert!(
            refused(&with_entry(name)).starts_with(&not_a_package_file(name)),
            "{name}"
        );
    }
}

#[test]
fn a_source_name_longer_than_64_is_refused() {
    let name = format!("src/{}.st", "a".repeat(65));
    assert!(refused(&with_entry(&name)).contains("at most 64"));
}

#[test]
fn two_entries_whose_names_differ_only_in_case_are_refused() {
    // Everything else about this package is right, so only the rule on case can refuse it.
    let entries = vec![
        entry(
            "manifest.json",
            &serde_json::to_vec(&manifest_for(&["src/Motor.st", "src/MOTOR.st"])).unwrap(),
        ),
        entry("src/Motor.st", MOTOR.as_bytes()),
        entry("src/MOTOR.st", MOTOR.as_bytes()),
    ];
    assert_eq!(
        refused(&with_checksums(entries)),
        "src/Motor.st and src/MOTOR.st are one name"
    );
}

#[test]
fn two_entries_of_the_same_name_are_refused() {
    let mut entries = vec![
        entry(
            "manifest.json",
            &serde_json::to_vec(&manifest_for(&["src/motor.st"])).unwrap(),
        ),
        entry("src/motor.st", MOTOR.as_bytes()),
    ];
    let checksums = checksums_for(&entries);
    entries.push(entry(
        "src/motor.st",
        b"FUNCTION_BLOCK Evil END_FUNCTION_BLOCK",
    ));
    entries.push(entry("checksums.json", &checksums));
    assert_eq!(
        refused(&zip_of(&entries)),
        "src/motor.st and src/motor.st are one name"
    );
}

#[test]
fn a_symbolic_link_entry_is_refused() {
    // Everything else about this package is right, the link listed in the manifest and the
    // checksums as if it were a file, so only the rule on links can refuse it.
    let target = b"../../etc/passwd";
    let mut entries = vec![
        entry(
            "manifest.json",
            &serde_json::to_vec(&manifest_for(&["src/motor.st", "src/link.st"])).unwrap(),
        ),
        entry("src/motor.st", MOTOR.as_bytes()),
    ];
    let mut listed = entries.clone();
    listed.push(entry("src/link.st", target));
    entries.push(entry("checksums.json", &checksums_for(&listed)));
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in &entries {
        writer.start_file(name.as_str(), deflated()).unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer
        .add_symlink("src/link.st", "../../etc/passwd", FileOptions::default())
        .unwrap();
    let file = writer.finish().unwrap().into_inner();
    assert_eq!(
        refused(&file),
        "src/link.st is a symbolic link; a package holds files"
    );
}

#[test]
fn an_encrypted_entry_is_refused_naming_it() {
    let mut file = valid();
    patch(&mut file, "src/motor.st", 6, 8, &1u16.to_le_bytes());
    assert_eq!(
        refused(&file),
        "src/motor.st: unsupported Zip archive: Password required to decrypt file"
    );
}

#[test]
fn an_entry_of_an_unsupported_method_is_refused_naming_it() {
    let mut file = valid();
    patch(&mut file, "src/motor.st", 8, 10, &12u16.to_le_bytes());
    assert_eq!(
        refused(&file),
        "src/motor.st: unsupported Zip archive: Compression method not supported"
    );
}

// -----------------------------------------------------------------------------
// R6: checksums and files
// -----------------------------------------------------------------------------

#[test]
fn a_file_that_does_not_match_its_checksum_is_refused() {
    let mut entries = vec![
        entry("manifest.json", &serde_json::to_vec(&manifest()).unwrap()),
        entry("src/motor.st", MOTOR.as_bytes()),
        entry("src/ramp.st", RAMP.as_bytes()),
    ];
    let checksums = checksums_for(&entries);
    entries[1] = entry("src/motor.st", b"FUNCTION_BLOCK Motor END_FUNCTION_BLOCK\n");
    entries.push(entry("checksums.json", &checksums));
    assert_eq!(
        refused(&zip_of(&entries)),
        "src/motor.st does not match its checksum"
    );
}

#[test]
fn a_manifest_that_does_not_match_its_checksum_is_refused() {
    let mut entries = vec![
        entry("manifest.json", &serde_json::to_vec(&manifest()).unwrap()),
        entry("src/motor.st", MOTOR.as_bytes()),
        entry("src/ramp.st", RAMP.as_bytes()),
    ];
    let checksums = checksums_for(&entries);
    let mut changed = manifest();
    changed["license"] = json!("Proprietary");
    entries[0] = entry("manifest.json", &serde_json::to_vec(&changed).unwrap());
    entries.push(entry("checksums.json", &checksums));
    assert_eq!(
        refused(&zip_of(&entries)),
        "manifest.json does not match its checksum"
    );
}

#[test]
fn a_package_with_no_checksums_is_refused() {
    let file = zip_of(&[
        entry("manifest.json", &serde_json::to_vec(&manifest()).unwrap()),
        entry("src/motor.st", MOTOR.as_bytes()),
        entry("src/ramp.st", RAMP.as_bytes()),
    ]);
    assert_eq!(refused(&file), "the package has no checksums.json");
}

#[test]
fn a_package_with_no_manifest_is_refused() {
    let file = with_checksums(vec![entry("src/motor.st", MOTOR.as_bytes())]);
    assert_eq!(refused(&file), "the package has no manifest.json");
}

fn with_checksums_text(checksums: &Value) -> Vec<u8> {
    zip_of(&[
        entry("manifest.json", &serde_json::to_vec(&manifest()).unwrap()),
        entry("src/motor.st", MOTOR.as_bytes()),
        entry("src/ramp.st", RAMP.as_bytes()),
        entry("checksums.json", &serde_json::to_vec(checksums).unwrap()),
    ])
}

fn right_checksums() -> serde_json::Map<String, Value> {
    let mut map = serde_json::Map::new();
    map.insert(
        "manifest.json".into(),
        json!(sha(&serde_json::to_vec(&manifest()).unwrap())),
    );
    map.insert("src/motor.st".into(), json!(sha(MOTOR.as_bytes())));
    map.insert("src/ramp.st".into(), json!(sha(RAMP.as_bytes())));
    map
}

#[test]
fn checksums_that_are_not_a_map_of_sha256_are_refused() {
    let mut upper = right_checksums();
    upper.insert(
        "src/ramp.st".into(),
        json!(sha(RAMP.as_bytes()).to_uppercase()),
    );
    for checksums in [
        json!(["x"]),
        json!({"src/motor.st": 1}),
        Value::Object(upper),
    ] {
        let words = refused(&with_checksums_text(&checksums));
        assert!(
            words.starts_with("checksums.json is not a map of each file to its SHA-256"),
            "{words}"
        );
    }
    assert!(read(&with_checksums_text(&Value::Object(right_checksums()))).is_ok());
}

#[test]
fn a_manifest_not_listed_in_checksums_is_refused() {
    let mut checksums = right_checksums();
    checksums.remove("manifest.json");
    assert_eq!(
        refused(&with_checksums_text(&Value::Object(checksums))),
        "manifest.json is not listed in checksums.json"
    );
}

#[test]
fn a_source_not_listed_in_checksums_is_refused() {
    let mut checksums = right_checksums();
    checksums.remove("src/ramp.st");
    assert_eq!(
        refused(&with_checksums_text(&Value::Object(checksums))),
        "src/ramp.st is not listed in checksums.json"
    );
}

#[test]
fn a_checksum_listed_for_a_file_the_package_does_not_hold_is_refused() {
    let mut checksums = right_checksums();
    checksums.insert("src/gone.st".into(), json!(sha(b"")));
    assert_eq!(
        refused(&with_checksums_text(&Value::Object(checksums))),
        "checksums.json lists src/gone.st, which the package does not hold"
    );
}

#[test]
fn a_manifest_file_not_among_the_entries_is_refused() {
    let file = with_checksums(vec![
        entry(
            "manifest.json",
            &serde_json::to_vec(&manifest_for(&[
                "src/motor.st",
                "src/ramp.st",
                "src/gone.st",
            ]))
            .unwrap(),
        ),
        entry("src/motor.st", MOTOR.as_bytes()),
        entry("src/ramp.st", RAMP.as_bytes()),
    ]);
    assert_eq!(
        refused(&file),
        "manifest.json lists src/gone.st, which the package does not hold"
    );
}

#[test]
fn a_source_not_among_the_manifest_files_is_refused() {
    let file = with_checksums(vec![
        entry(
            "manifest.json",
            &serde_json::to_vec(&manifest_for(&["src/motor.st"])).unwrap(),
        ),
        entry("src/motor.st", MOTOR.as_bytes()),
        entry("src/ramp.st", RAMP.as_bytes()),
    ]);
    assert_eq!(
        refused(&file),
        "src/ramp.st is not among the files of manifest.json"
    );
}

#[test]
fn a_manifest_naming_a_file_twice_is_refused() {
    let file = with_manifest(|m| {
        m.insert(
            "files".into(),
            json!(["src/motor.st", "src/ramp.st", "src/Ramp.st"]),
        );
    });
    assert_eq!(
        refused(&file),
        "manifest.json's files name src/Ramp.st twice"
    );
}

// -----------------------------------------------------------------------------
// R7: source text
// -----------------------------------------------------------------------------

fn with_motor_bytes(bytes: &[u8]) -> Vec<u8> {
    with_checksums(vec![
        entry("manifest.json", &serde_json::to_vec(&manifest()).unwrap()),
        entry("src/motor.st", bytes),
        entry("src/ramp.st", RAMP.as_bytes()),
    ])
}

#[test]
fn a_source_that_is_not_utf8_is_refused() {
    assert_eq!(
        refused(&with_motor_bytes(
            b"FUNCTION_BLOCK Motor (* \xFF *) END_FUNCTION_BLOCK"
        )),
        "src/motor.st is not UTF-8 text"
    );
}

#[test]
fn a_source_with_a_byte_order_mark_is_refused() {
    let bytes = [b"\xEF\xBB\xBF".as_slice(), MOTOR.as_bytes()].concat();
    assert_eq!(
        refused(&with_motor_bytes(&bytes)),
        "src/motor.st begins with a byte order mark"
    );
}

#[test]
fn a_source_with_crlf_line_ends_checks_as_with_lf() {
    let crlf = MOTOR.replace('\n', "\r\n");
    let package = read(&with_motor_bytes(crlf.as_bytes())).unwrap();
    let lf = read(&valid()).unwrap();
    assert_eq!(
        package.sources[0],
        ("src/motor.st".to_string(), crlf.clone())
    );
    assert_eq!(package.sources[0].1.replace("\r\n", "\n"), lf.sources[0].1);
    assert_eq!(package.manifest, lf.manifest);
}

// -----------------------------------------------------------------------------
// R8: the SHA-256 of the file
// -----------------------------------------------------------------------------

#[test]
fn the_sha256_answered_is_of_the_file_not_of_its_sources() {
    let file = valid();
    let package = read(&file).unwrap();
    assert_eq!(package.sha256, sha(&file));
    assert_eq!(package.sha256.len(), 64);
    assert!(package
        .sha256
        .bytes()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)));

    // The same sources in another zip are another file with another SHA-256.
    let stored = {
        let entries = vec![
            entry("manifest.json", &serde_json::to_vec(&manifest()).unwrap()),
            entry("src/motor.st", MOTOR.as_bytes()),
            entry("src/ramp.st", RAMP.as_bytes()),
        ];
        let checksums = checksums_for(&entries);
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        let stored = FileOptions::default().compression_method(CompressionMethod::Stored);
        for (name, bytes) in entries.iter().chain([&entry("checksums.json", &checksums)]) {
            writer.start_file(name.as_str(), stored).unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap().into_inner()
    };
    let other = read(&stored).unwrap();
    assert_eq!(other.sources, package.sources);
    assert_eq!(other.sha256, sha(&stored));
    assert_ne!(other.sha256, package.sha256);
}
