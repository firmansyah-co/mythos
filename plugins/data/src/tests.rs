//! The data module's routes, called as the server calls them: an `HttpCall` whose data
//! directory is a directory of the test's own.

use super::*;
use std::collections::BTreeSet;

const RECIPE: &str = "TYPE RecipeData : STRUCT
  speed : INT;
  temp : REAL;
END_STRUCT; END_TYPE
";

const LINE: &str = "TYPE LineState : STRUCT running : BOOL; count : DINT; END_STRUCT; END_TYPE\n";

/// A project's data directory, as the core makes it (`<project>/data`, empty).
struct Project {
    dir: tempfile::TempDir,
    data: PathBuf,
}

fn project() -> Project {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("plant").join("data");
    std::fs::create_dir_all(&data).unwrap();
    Project { dir, data }
}

/// Every file and directory under `root`, relative to it, with `/` between the parts.
fn listing(root: &Path) -> BTreeSet<String> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeSet<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries {
            let path = e.unwrap().path();
            let rel: Vec<String> = path
                .strip_prefix(root)
                .unwrap()
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect();
            out.insert(rel.join("/"));
            if path.is_dir() {
                walk(root, &path, out);
            }
        }
    }
    let mut out = BTreeSet::new();
    walk(root, root, &mut out);
    out
}

struct Answer {
    status: u16,
    body: Value,
    event: Option<Value>,
}

async fn call(
    data: &Data,
    project: &Project,
    method: &str,
    route: &str,
    params: &[(&str, &str)],
    body: Option<Value>,
) -> Answer {
    let call = HttpCall {
        method: method.into(),
        route: route.into(),
        params: params
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        query: Default::default(),
        body,
        identity: None,
        project: Some("plant".into()),
        data_dir: Some(project.data.display().to_string()),
        tags: None,
    };
    let (reply, event) = data.serve(&call).await;
    Answer {
        status: reply.status,
        body: reply.body,
        event,
    }
}

async fn put_type(data: &Data, p: &Project, name: &str, source: &str) -> Answer {
    call(
        data,
        p,
        "PUT",
        "/types/:type",
        &[("type", name)],
        Some(json!({ "source": source })),
    )
    .await
}

async fn put_block(data: &Data, p: &Project, name: &str, ty: &str, retain: bool) -> Answer {
    call(
        data,
        p,
        "PUT",
        "/blocks/:block",
        &[("block", name)],
        Some(json!({ "type": ty, "retain": retain })),
    )
    .await
}

async fn get_blocks(data: &Data, p: &Project) -> Answer {
    call(data, p, "GET", "/blocks", &[], None).await
}

fn ok(a: &Answer) -> &Value {
    assert_eq!(a.status, 200, "{}", a.body);
    &a.body
}

fn keys(v: &Value) -> BTreeSet<&str> {
    v.as_object().unwrap().keys().map(String::as_str).collect()
}

fn messages(body: &Value) -> Vec<String> {
    body["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["message"].as_str().unwrap().to_string())
        .collect()
}

#[tokio::test]
async fn a_type_is_stored_as_its_own_file_and_read_back() {
    let (data, p) = (Data::default(), project());
    let saved = put_type(&data, &p, "RecipeData", RECIPE).await;
    let body = ok(&saved);
    assert_eq!(
        *body,
        json!({ "name": "RecipeData", "created": true, "blocks": [], "changed": [] })
    );
    assert_eq!(
        saved.event,
        Some(
            json!({ "project": "plant", "what": "type", "name": "RecipeData", "change": "saved" })
        )
    );
    assert_eq!(
        std::fs::read_to_string(p.data.join("types").join("RecipeData.st")).unwrap(),
        RECIPE
    );
    assert_eq!(
        listing(&p.data),
        BTreeSet::from(["types".to_string(), "types/RecipeData.st".to_string()])
    );

    let read = call(
        &data,
        &p,
        "GET",
        "/types/:type",
        &[("type", "RecipeData")],
        None,
    )
    .await;
    assert_eq!(
        *ok(&read),
        json!({ "name": "RecipeData", "source": RECIPE })
    );
    assert_eq!(read.event, None);
    let listed = call(&data, &p, "GET", "/types", &[], None).await;
    assert_eq!(*ok(&listed), json!({ "types": [{ "name": "RecipeData" }] }));

    // The same spelling again replaces it.
    let again = put_type(
        &data,
        &p,
        "RecipeData",
        LINE.replace("LineState", "RecipeData").as_str(),
    )
    .await;
    assert_eq!(ok(&again)["created"], false);
    assert_eq!(
        std::fs::read_to_string(p.data.join("types").join("RecipeData.st")).unwrap(),
        LINE.replace("LineState", "RecipeData")
    );

    // Listed in the order of their file names.
    ok(&put_type(&data, &p, "LineState", LINE).await);
    let listed = call(&data, &p, "GET", "/types", &[], None).await;
    assert_eq!(
        *ok(&listed),
        json!({ "types": [{ "name": "LineState" }, { "name": "RecipeData" }] })
    );

    let missing = call(&data, &p, "GET", "/types/:type", &[("type", "Nope")], None).await;
    assert_eq!(missing.status, 404);
    assert_eq!(
        missing.body,
        json!({ "error": "the project has no type 'Nope'" })
    );
}

#[tokio::test]
async fn a_type_name_is_an_identifier_and_never_a_windows_device_name() {
    let (data, p) = (Data::default(), project());
    let not_a_name = |n: &str| {
        format!("'{n}' is not a type name: a letter or '_' first, then letters, digits and single underscores, not ending in '_', at most 64")
    };
    let device = |n: &str| {
        format!("'{n}' cannot be a type name: it is a device name on Windows (CON, PRN, AUX, NUL, COM0 to COM9, LPT0 to LPT9)")
    };
    let long = "A".repeat(65);
    let refused: Vec<(&str, String)> = vec![
        ("CON", device("CON")),
        ("con", device("con")),
        ("pRn", device("pRn")),
        ("Aux", device("Aux")),
        ("nul", device("nul")),
        ("Com1", device("Com1")),
        ("cOM0", device("cOM0")),
        ("LPT9", device("LPT9")),
        ("lpt0", device("lpt0")),
        ("a.b", not_a_name("a.b")),
        ("a__b", not_a_name("a__b")),
        ("a_", not_a_name("a_")),
        ("1a", not_a_name("1a")),
        ("_", not_a_name("_")),
        ("", not_a_name("")),
        (long.as_str(), not_a_name(&long)),
        ("é", not_a_name("é")),
    ];
    for (name, message) in &refused {
        let source = format!("TYPE {name} : STRUCT a : INT; END_STRUCT; END_TYPE");
        for (method, body) in [
            ("PUT", Some(json!({ "source": source }))),
            ("GET", None),
            ("DELETE", None),
        ] {
            let a = call(&data, &p, method, "/types/:type", &[("type", name)], body).await;
            assert_eq!(a.status, 400, "{method} {name}: {}", a.body);
            assert_eq!(a.body, json!({ "error": message }), "{method} {name}");
        }
        // The check of a draft says the same of the name it is given.
        let a = call(
            &data,
            &p,
            "POST",
            "/check",
            &[],
            Some(json!({ "type": name, "source": source })),
        )
        .await;
        assert_eq!(a.status, 400, "check {name}: {}", a.body);
        assert_eq!(a.body, json!({ "error": message }), "check {name}");
    }
    assert!(listing(&p.data).is_empty(), "{:?}", listing(&p.data));

    let longest = "A".repeat(64);
    for name in [
        "Recipe",
        "_Tmp",
        "Con1",
        "CONSOLE",
        "Lpt10",
        longest.as_str(),
    ] {
        let source = format!("TYPE {name} : STRUCT a : INT; END_STRUCT; END_TYPE");
        let a = put_type(&data, &p, name, &source).await;
        assert_eq!(a.status, 200, "{name}: {}", a.body);
        assert!(p.data.join("types").join(format!("{name}.st")).is_file());
    }
}

#[tokio::test]
async fn a_type_source_declares_its_type_and_nothing_else() {
    let (data, p) = (Data::default(), project());
    let rule = |said: &str| {
        format!("the source of type 'RecipeData' must declare the type RecipeData and nothing else; it declares {said}")
    };
    let cases = [
        (
            "TYPE RecipeData : STRUCT a : INT; END_STRUCT; Other : STRUCT b : INT; END_STRUCT; END_TYPE",
            rule("type RecipeData, type Other"),
        ),
        (
            "TYPE Other : STRUCT b : INT; END_STRUCT; END_TYPE\nTYPE RecipeData : STRUCT a : INT; END_STRUCT; END_TYPE",
            rule("type Other, type RecipeData"),
        ),
        (
            "TYPE RecipeData : STRUCT a : INT; END_STRUCT; END_TYPE\nFUNCTION_BLOCK Helper VAR x : INT; END_VAR END_FUNCTION_BLOCK",
            rule("type RecipeData, function block Helper"),
        ),
        (
            "FUNCTION Twice : INT VAR_INPUT x : INT; END_VAR Twice := x * 2; END_FUNCTION\nTYPE RecipeData : STRUCT a : INT; END_STRUCT; END_TYPE",
            rule("function Twice, type RecipeData"),
        ),
        (
            "TYPE recipedata : STRUCT a : INT; END_STRUCT; END_TYPE",
            rule("type recipedata"),
        ),
        ("TYPE END_TYPE", rule("nothing")),
    ];
    for (source, message) in &cases {
        let a = put_type(&data, &p, "RecipeData", source).await;
        assert_eq!(a.status, 400, "{source}: {}", a.body);
        assert_eq!(a.body, json!({ "error": message }), "{source}");
        assert_eq!(a.event, None);
    }
    // A PROGRAM is the compiler's own refusal, whose diagnostics come first.
    let a = put_type(
        &data,
        &p,
        "RecipeData",
        "TYPE RecipeData : STRUCT a : INT; END_STRUCT; END_TYPE\nPROGRAM P\nEND_PROGRAM",
    )
    .await;
    assert_eq!(a.status, 400, "{}", a.body);
    assert_eq!(
        a.body["error"],
        "with this source the project's types and data blocks do not check, and nothing was saved"
    );
    assert!(
        messages(&a.body).contains(
            &"P is a PROGRAM in the library source 'data/types/RecipeData.st'; a library holds types, functions and function blocks".to_string()
        ),
        "{}",
        a.body
    );
    assert!(listing(&p.data).is_empty(), "{:?}", listing(&p.data));

    // A name for an array type is a type too.
    let a = put_type(
        &data,
        &p,
        "Levels",
        "TYPE Levels : ARRAY [1..4] OF INT; END_TYPE",
    )
    .await;
    assert_eq!(a.status, 200, "{}", a.body);
}

#[tokio::test]
async fn a_source_that_does_not_check_is_refused_and_nothing_is_written() {
    let (data, p) = (Data::default(), project());
    ok(&put_type(&data, &p, "RecipeData", RECIPE).await);
    let before = listing(&p.data);
    let broken =
        "TYPE LineState : STRUCT\n  running : BOOL;\n  count : NoSuchType;\nEND_STRUCT; END_TYPE\n";
    let a = put_type(&data, &p, "LineState", broken).await;
    assert_eq!(a.status, 400, "{}", a.body);
    assert_eq!(
        a.body["error"],
        "with this source the project's types and data blocks do not check, and nothing was saved"
    );
    let d = &a.body["diagnostics"][0];
    assert_eq!(d["source"], "data/types/LineState.st", "{}", a.body);
    assert_eq!(d["line"], 3, "{}", a.body);
    assert_eq!(a.event, None);
    assert_eq!(listing(&p.data), before);

    // Replacing a type that checks with one that does not leaves the file as it was.
    let a = put_type(
        &data,
        &p,
        "RecipeData",
        "TYPE RecipeData : STRUCT speed : INT END_STRUCT; END_TYPE",
    )
    .await;
    assert_eq!(a.status, 400, "{}", a.body);
    assert_eq!(
        a.body["diagnostics"][0]["source"],
        "data/types/RecipeData.st"
    );
    assert_eq!(
        std::fs::read_to_string(p.data.join("types").join("RecipeData.st")).unwrap(),
        RECIPE
    );
    assert_eq!(listing(&p.data), before);
    assert!(
        !listing(p.dir.path()).iter().any(|f| f.ends_with(".tmp")),
        "a temporary is left"
    );
}

#[tokio::test]
async fn a_name_that_differs_only_in_case_is_one_name() {
    let (data, p) = (Data::default(), project());
    ok(&put_type(&data, &p, "RecipeData", RECIPE).await);
    let a = put_type(
        &data,
        &p,
        "recipedata",
        &RECIPE.replace("RecipeData", "recipedata"),
    )
    .await;
    assert_eq!(a.status, 409, "{}", a.body);
    assert_eq!(
        a.body,
        json!({ "error": "the project already has a type 'RecipeData'; names that differ only in case are one name" })
    );
    assert_eq!(
        listing(&p.data.join("types")),
        BTreeSet::from(["RecipeData.st".to_string()])
    );
    // A read or a delete finds it by any spelling, and answers with the stored one.
    let read = call(
        &data,
        &p,
        "GET",
        "/types/:type",
        &[("type", "RECIPEDATA")],
        None,
    )
    .await;
    assert_eq!(ok(&read)["name"], "RecipeData");

    ok(&put_block(&data, &p, "Recipe", "RecipeData", false).await);
    let a = put_block(&data, &p, "RECIPE", "RecipeData", true).await;
    assert_eq!(a.status, 409, "{}", a.body);
    assert_eq!(
        a.body,
        json!({ "error": "the project already has a data block 'Recipe'; names that differ only in case are one name" })
    );
    // The same spelling replaces, in its place; the type may be spelled any way.
    ok(&put_type(&data, &p, "LineState", LINE).await);
    ok(&put_block(&data, &p, "Line", "LineState", false).await);
    let a = put_block(&data, &p, "Recipe", "recipedata", true).await;
    let body = ok(&a);
    assert_eq!(body["created"], false);
    assert_eq!(body["blocks"][0]["name"], "Recipe");
    assert_eq!(body["blocks"][0]["retain"], true);
    assert_eq!(body["blocks"][1]["name"], "Line");
    // Same type, same layout: nothing changed.
    assert_eq!(body["changed"], json!([]));
}

#[tokio::test]
async fn a_block_is_refused_for_each_thing_the_compiler_refuses() {
    let (data, p) = (Data::default(), project());
    ok(&put_type(&data, &p, "RecipeData", RECIPE).await);
    ok(&put_type(
        &data,
        &p,
        "Levels",
        "TYPE Levels : ARRAY [1..4] OF INT; END_TYPE",
    )
    .await);
    let table = [
        ("1x", "RecipeData", "data block '1x': the name is not an identifier (letters, digits and single underscores, not beginning with a digit)"),
        ("a__b", "RecipeData", "data block 'a__b': the name is not an identifier (letters, digits and single underscores, not beginning with a digit)"),
        ("IF", "RecipeData", "data block 'IF': IF is a keyword of IEC 61131-3 and cannot be used as a name"),
        ("TON", "RecipeData", "data block 'TON': TON is a name of the standard; choose another"),
        ("RecipeData", "RecipeData", "data block 'RecipeData': RecipeData is also the name of a type or POU of the project; a data block needs its own"),
        ("B", "INT", "data block 'B' is a INT; a data block is an instance of a structure type"),
        ("B", "Levels", "data block 'B' is a Levels; a data block is an instance of a structure type"),
        ("B", "TON", "data block 'B' is a TON; a data block is an instance of a structure type (a function block type comes later)"),
        ("B", "Nope", "data block 'B': there is no type Nope"),
    ];
    for (name, ty, message) in table {
        let a = put_block(&data, &p, name, ty, false).await;
        assert_eq!(a.status, 400, "{name} {ty}: {}", a.body);
        assert_eq!(
            a.body["error"],
            format!("data block '{name}' does not check and was not saved"),
            "{name} {ty}"
        );
        assert!(
            messages(&a.body).contains(&message.to_string()),
            "{name} {ty}: {}",
            a.body
        );
        assert_eq!(a.event, None);
        assert!(!p.data.join("blocks.json").exists(), "{name} {ty}");
    }
    let wrong_body = [
        json!({ "type": "RecipeData" }),
        json!({ "retain": true }),
        json!({ "type": "RecipeData", "retain": "yes" }),
        json!({ "type": "RecipeData", "retain": false, "initial": 1 }),
    ];
    for body in wrong_body {
        let a = call(
            &data,
            &p,
            "PUT",
            "/blocks/:block",
            &[("block", "B")],
            Some(body.clone()),
        )
        .await;
        assert_eq!(a.status, 400, "{body}: {}", a.body);
        assert!(
            a.body["error"]
                .as_str()
                .unwrap()
                .starts_with("the body is not what this route takes: "),
            "{}",
            a.body
        );
    }
    assert!(!p.data.join("blocks.json").exists());
}

#[tokio::test]
async fn a_type_a_block_uses_cannot_be_deleted_nor_one_another_type_uses() {
    let (data, p) = (Data::default(), project());
    ok(&put_type(&data, &p, "RecipeData", RECIPE).await);
    ok(&put_block(&data, &p, "Recipe", "RecipeData", false).await);
    ok(&put_block(&data, &p, "Other", "RECIPEDATA", true).await);
    let a = call(
        &data,
        &p,
        "DELETE",
        "/types/:type",
        &[("type", "RecipeData")],
        None,
    )
    .await;
    assert_eq!(a.status, 409, "{}", a.body);
    assert_eq!(
        a.body,
        json!({ "error": "type 'RecipeData' is the type of data block 'Recipe', 'Other': delete those blocks or give them another type first" })
    );
    assert!(p.data.join("types").join("RecipeData.st").is_file());

    ok(&put_type(
        &data,
        &p,
        "Inner",
        "TYPE Inner : STRUCT a : INT; END_STRUCT; END_TYPE",
    )
    .await);
    ok(&put_type(
        &data,
        &p,
        "Outer",
        "TYPE Outer : STRUCT i : Inner; b : BOOL; END_STRUCT; END_TYPE",
    )
    .await);
    let a = call(
        &data,
        &p,
        "DELETE",
        "/types/:type",
        &[("type", "Inner")],
        None,
    )
    .await;
    assert_eq!(a.status, 400, "{}", a.body);
    assert_eq!(
        a.body["error"],
        "without type 'Inner' the project's types and data blocks do not check, and it was not deleted"
    );
    assert_eq!(
        a.body["diagnostics"][0]["source"], "data/types/Outer.st",
        "{}",
        a.body
    );
    assert!(p.data.join("types").join("Inner.st").is_file());

    // Once nothing uses it, it goes: by any spelling, its stored file.
    ok(&call(
        &data,
        &p,
        "DELETE",
        "/types/:type",
        &[("type", "OUTER")],
        None,
    )
    .await);
    let a = call(
        &data,
        &p,
        "DELETE",
        "/types/:type",
        &[("type", "inner")],
        None,
    )
    .await;
    assert_eq!(ok(&a)["deleted"], "Inner");
    assert_eq!(
        a.event,
        Some(json!({ "project": "plant", "what": "type", "name": "Inner", "change": "deleted" }))
    );
    assert_eq!(ok(&a)["blocks"].as_array().unwrap().len(), 2);
    assert!(!p.data.join("types").join("Inner.st").exists());
    let a = call(
        &data,
        &p,
        "DELETE",
        "/types/:type",
        &[("type", "Inner")],
        None,
    )
    .await;
    assert_eq!(a.status, 404);
    assert_eq!(
        a.body,
        json!({ "error": "the project has no type 'Inner'" })
    );

    // Blocks go the same way, and then their type.
    for block in ["recipe", "Other"] {
        let a = call(
            &data,
            &p,
            "DELETE",
            "/blocks/:block",
            &[("block", block)],
            None,
        )
        .await;
        ok(&a);
        assert_eq!(a.event.as_ref().unwrap()["what"], "block");
        assert_eq!(a.event.as_ref().unwrap()["change"], "deleted");
    }
    let a = call(
        &data,
        &p,
        "DELETE",
        "/blocks/:block",
        &[("block", "Other")],
        None,
    )
    .await;
    assert_eq!(a.status, 404);
    assert_eq!(
        a.body,
        json!({ "error": "the project has no data block 'Other'" })
    );
    ok(&call(
        &data,
        &p,
        "DELETE",
        "/types/:type",
        &[("type", "RecipeData")],
        None,
    )
    .await);
}

#[tokio::test]
async fn blocks_json_is_exactly_the_documented_shape() {
    let (data, p) = (Data::default(), project());
    ok(&put_type(&data, &p, "RecipeData", RECIPE).await);
    ok(&put_type(&data, &p, "LineState", LINE).await);
    ok(&put_block(&data, &p, "Recipe", "RecipeData", false).await);
    ok(&put_block(&data, &p, "Line", "LineState", true).await);
    assert_eq!(
        std::fs::read_to_string(p.data.join("blocks.json")).unwrap(),
        r#"{
  "blocks": [
    { "name": "Recipe", "type": "RecipeData", "retain": false },
    { "name": "Line", "type": "LineState", "retain": true }
  ]
}
"#
    );
    // Without a block it is still the shape.
    ok(&call(
        &data,
        &p,
        "DELETE",
        "/blocks/:block",
        &[("block", "Recipe")],
        None,
    )
    .await);
    ok(&call(
        &data,
        &p,
        "DELETE",
        "/blocks/:block",
        &[("block", "Line")],
        None,
    )
    .await);
    assert_eq!(
        std::fs::read_to_string(p.data.join("blocks.json")).unwrap(),
        "{\n  \"blocks\": []\n}\n"
    );
    // A name that JSON must escape is escaped.
    assert_eq!(
        blocks_file_text(&[BlockEntry { name: "a\"b".into(), ty: "T".into(), retain: false }]),
        "{\n  \"blocks\": [\n    { \"name\": \"a\\\"b\", \"type\": \"T\", \"retain\": false }\n  ]\n}\n"
    );

    // A file with a key this module does not know is refused by every route, not cut.
    std::fs::write(
        p.data.join("blocks.json"),
        r#"{"blocks": [{"name": "Recipe", "type": "RecipeData", "retain": false, "initial": {}}]}"#,
    )
    .unwrap();
    let path = p.data.join("blocks.json");
    type Route<'a> = (&'a str, &'a str, Vec<(&'a str, &'a str)>, Option<Value>);
    let routes: Vec<Route> = vec![
        ("GET", "/types", vec![], None),
        ("GET", "/types/:type", vec![("type", "RecipeData")], None),
        (
            "PUT",
            "/types/:type",
            vec![("type", "RecipeData")],
            Some(json!({ "source": RECIPE })),
        ),
        ("DELETE", "/types/:type", vec![("type", "LineState")], None),
        ("GET", "/blocks", vec![], None),
        (
            "PUT",
            "/blocks/:block",
            vec![("block", "B")],
            Some(json!({ "type": "RecipeData", "retain": false })),
        ),
        ("DELETE", "/blocks/:block", vec![("block", "Recipe")], None),
        ("POST", "/check", vec![], None),
    ];
    for (method, route, params, body) in routes {
        let a = call(&data, &p, method, route, &params, body).await;
        assert_eq!(a.status, 500, "{method} {route}: {}", a.body);
        let said = a.body["error"].as_str().unwrap();
        assert!(
            said.starts_with(&format!(
                "{} cannot be read as the project's data blocks: unknown field `initial`",
                path.display()
            )),
            "{method} {route}: {said}"
        );
    }
    // And `retain` has no default.
    std::fs::write(
        p.data.join("blocks.json"),
        r#"{"blocks": [{"name": "Recipe", "type": "RecipeData"}]}"#,
    )
    .unwrap();
    let a = get_blocks(&data, &p).await;
    assert_eq!(a.status, 500, "{}", a.body);
    assert!(a.body["error"]
        .as_str()
        .unwrap()
        .contains("missing field `retain`"));
}

#[tokio::test]
async fn a_saved_type_answers_each_blocks_layout_and_which_changed() {
    let (data, p) = (Data::default(), project());
    ok(&put_type(&data, &p, "RecipeData", RECIPE).await);
    let made = put_block(&data, &p, "Recipe", "RecipeData", false).await;
    let made = ok(&made).clone();
    assert_eq!(made["created"], true);
    assert_eq!(made["changed"], json!(["Recipe"]));
    let block = &made["blocks"][0];
    assert_eq!(
        keys(block),
        BTreeSet::from(["name", "type", "retain", "layout", "size", "members"])
    );
    assert_eq!(block["name"], "Recipe");
    assert_eq!(block["type"], "RecipeData");
    assert_eq!(block["retain"], false);
    let layout = block["layout"].as_str().unwrap().to_string();
    assert_eq!(layout.len(), 16, "{layout}");
    assert!(layout
        .chars()
        .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    let members = block["members"].as_array().unwrap();
    assert_eq!(members.len(), 2);
    assert_eq!(
        keys(&members[0]),
        BTreeSet::from(["name", "type", "offset", "size"])
    );
    assert_eq!(members[0]["name"], "speed");
    assert_eq!(members[1]["name"], "temp");
    // The layout is the compiler's, member for member.
    let laid = check(&read_state(&p.data).unwrap()).unwrap();
    assert_eq!(block["layout"], laid[0].layout);
    assert_eq!(block["size"], laid[0].size);
    for (m, l) in members.iter().zip(&laid[0].members) {
        assert_eq!(m["type"], l.ty);
        assert_eq!(m["offset"], l.offset);
        assert_eq!(m["size"], l.size);
    }

    // Another type and block, which no change of RecipeData touches.
    ok(&put_type(&data, &p, "LineState", LINE).await);
    ok(&put_block(&data, &p, "Line", "LineState", true).await);

    let grown = RECIPE.replace("temp : REAL;", "temp : REAL;\n  batch : DINT;");
    let a = put_type(&data, &p, "RecipeData", &grown).await;
    let body = ok(&a);
    assert_eq!(body["created"], false);
    assert_eq!(body["changed"], json!(["Recipe"]));
    assert_ne!(body["blocks"][0]["layout"], layout.as_str());
    assert_eq!(body["blocks"][0]["members"].as_array().unwrap().len(), 3);

    let a = put_type(&data, &p, "RecipeData", &grown).await;
    assert_eq!(ok(&a)["changed"], json!([]));

    // Broken by hand: what the layouts were is not known.
    std::fs::write(
        p.data.join("types").join("RecipeData.st"),
        "TYPE RecipeData : STRUCT",
    )
    .unwrap();
    let a = put_type(&data, &p, "RecipeData", RECIPE).await;
    let body = ok(&a);
    assert_eq!(body["changed"], Value::Null);
    assert_eq!(body["blocks"][0]["layout"], layout.as_str());
}

#[tokio::test]
async fn blocks_that_do_not_check_on_disk_are_listed_without_a_layout() {
    let (data, p) = (Data::default(), project());
    ok(&put_type(&data, &p, "RecipeData", RECIPE).await);
    ok(&put_block(&data, &p, "Recipe", "RecipeData", false).await);
    let good = get_blocks(&data, &p).await;
    assert_eq!(ok(&good)["ok"], true);
    assert_eq!(ok(&good)["diagnostics"], json!([]));

    // By hand: a block of a type there is not, and one declared twice.
    std::fs::write(
        p.data.join("blocks.json"),
        r#"{"blocks": [
            {"name": "Recipe", "type": "RecipeData", "retain": false},
            {"name": "Gone", "type": "Missing", "retain": true},
            {"name": "RECIPE", "type": "RecipeData", "retain": false}
        ]}"#,
    )
    .unwrap();
    let a = get_blocks(&data, &p).await;
    let body = ok(&a);
    assert_eq!(keys(body), BTreeSet::from(["ok", "diagnostics", "blocks"]));
    assert_eq!(body["ok"], false);
    let said = messages(body);
    assert!(
        said.contains(&"data block 'Gone': there is no type Missing".to_string()),
        "{said:?}"
    );
    assert!(
        said.contains(&"data block 'RECIPE' is declared twice".to_string()),
        "{said:?}"
    );
    let blocks = body["blocks"].as_array().unwrap();
    assert_eq!(blocks.len(), 3);
    for b in blocks {
        assert_eq!(keys(b), BTreeSet::from(["name", "type", "retain"]), "{b}");
    }
    assert_eq!(
        blocks[1],
        json!({ "name": "Gone", "type": "Missing", "retain": true })
    );

    // The check of the stored state says the same.
    let a = call(&data, &p, "POST", "/check", &[], Some(json!({}))).await;
    assert_eq!(ok(&a)["ok"], false);
    for b in ok(&a)["blocks"].as_array().unwrap() {
        assert_eq!(keys(b), BTreeSet::from(["name", "type", "retain"]), "{b}");
    }

    // A write that leaves it not checking is refused; one that mends it is taken, with
    // `changed` unknown.
    let a = put_type(&data, &p, "LineState", LINE).await;
    assert_eq!(a.status, 400, "{}", a.body);
    let a = call(
        &data,
        &p,
        "DELETE",
        "/blocks/:block",
        &[("block", "Gone")],
        None,
    )
    .await;
    assert_eq!(a.status, 400, "{}", a.body);
    assert_eq!(
        a.body["error"],
        "without data block 'Gone' the project's types and data blocks do not check, and it was not deleted"
    );
    std::fs::write(
        p.data.join("blocks.json"),
        r#"{"blocks": [{"name": "Gone", "type": "Missing", "retain": true}]}"#,
    )
    .unwrap();
    let a = put_type(&data, &p, "Missing", &LINE.replace("LineState", "Missing")).await;
    let body = ok(&a);
    assert_eq!(body["changed"], Value::Null);
    assert!(body["blocks"][0]["layout"].is_string());
}

#[tokio::test]
async fn files_outside_the_file_rule_are_ignored() {
    let (data, p) = (Data::default(), project());
    ok(&put_type(&data, &p, "RecipeData", RECIPE).await);
    ok(&put_block(&data, &p, "Recipe", "RecipeData", false).await);
    let types_before = ok(&call(&data, &p, "GET", "/types", &[], None).await).clone();
    let blocks_before = ok(&get_blocks(&data, &p).await).clone();
    let types = p.data.join("types");
    let garbage = "this is not Structured Text (* at all";
    for name in [
        "X.tmp",
        ".blocks.json.9.tmp",
        "notes.txt",
        "Bad-Name.st",
        "Y.ST",
        ".RecipeData.st.77.tmp",
        "A__B.st",
    ] {
        std::fs::write(types.join(name), garbage).unwrap();
    }
    std::fs::write(p.data.join(".blocks.json.9.tmp"), garbage).unwrap();
    // A directory named like a type file is not a file.
    std::fs::create_dir(types.join("Dir.st")).unwrap();
    assert_eq!(
        *ok(&call(&data, &p, "GET", "/types", &[], None).await),
        types_before
    );
    assert_eq!(*ok(&get_blocks(&data, &p).await), blocks_before);
    let a = call(&data, &p, "GET", "/types/:type", &[("type", "Y")], None).await;
    assert_eq!(a.status, 404);
    // And a write checks without them.
    ok(&put_type(&data, &p, "LineState", LINE).await);
}

#[tokio::test]
async fn a_read_writes_nothing() {
    let (data, p) = (Data::default(), project());
    let a = call(&data, &p, "GET", "/types", &[], None).await;
    assert_eq!(*ok(&a), json!({ "types": [] }));
    let a = get_blocks(&data, &p).await;
    assert_eq!(
        *ok(&a),
        json!({ "ok": true, "diagnostics": [], "blocks": [] })
    );
    let a = call(&data, &p, "POST", "/check", &[], None).await;
    assert_eq!(
        *ok(&a),
        json!({ "ok": true, "diagnostics": [], "blocks": [] })
    );
    let a = call(
        &data,
        &p,
        "GET",
        "/types/:type",
        &[("type", "RecipeData")],
        None,
    )
    .await;
    assert_eq!(a.status, 404);
    assert!(listing(&p.data).is_empty(), "{:?}", listing(&p.data));
    assert_eq!(
        listing(p.dir.path()),
        BTreeSet::from(["plant".to_string(), "plant/data".to_string()])
    );
}

#[tokio::test]
async fn a_draft_is_checked_without_being_saved() {
    let (data, p) = (Data::default(), project());
    ok(&put_type(&data, &p, "RecipeData", RECIPE).await);
    ok(&put_block(&data, &p, "Recipe", "RecipeData", false).await);
    let stored = ok(&get_blocks(&data, &p).await).clone();
    let before = listing(&p.data);
    let grown = RECIPE.replace("temp : REAL;", "temp : REAL;\n  batch : DINT;");
    let a = call(
        &data,
        &p,
        "POST",
        "/check",
        &[],
        Some(json!({ "type": "RecipeData", "source": grown })),
    )
    .await;
    let body = ok(&a);
    assert_eq!(body["ok"], true);
    assert_ne!(body["blocks"][0]["layout"], stored["blocks"][0]["layout"]);
    assert_eq!(a.event, None);
    assert_eq!(listing(&p.data), before);
    assert_eq!(
        std::fs::read_to_string(p.data.join("types").join("RecipeData.st")).unwrap(),
        RECIPE
    );
    // A new type in a draft.
    let a = call(
        &data,
        &p,
        "POST",
        "/check",
        &[],
        Some(json!({ "type": "LineState", "source": LINE })),
    )
    .await;
    assert_eq!(ok(&a)["ok"], true);
    // A draft that does not check: the compiler's diagnostics, no layout.
    let a = call(
        &data,
        &p,
        "POST",
        "/check",
        &[],
        Some(json!({ "type": "RecipeData", "source": "TYPE RecipeData : STRUCT" })),
    )
    .await;
    let body = ok(&a);
    assert_eq!(body["ok"], false);
    assert_eq!(body["diagnostics"][0]["source"], "data/types/RecipeData.st");
    assert_eq!(
        keys(&body["blocks"][0]),
        BTreeSet::from(["name", "type", "retain"])
    );
    // A draft that checks but declares more than its type: the save's refusal, as a diagnostic.
    let a = call(&data, &p, "POST", "/check", &[], Some(json!({ "type": "LineState", "source": format!("{LINE}{}", "TYPE Extra : STRUCT a : INT; END_STRUCT; END_TYPE") }))).await;
    let body = ok(&a);
    assert_eq!(body["ok"], false);
    assert_eq!(
        messages(body),
        ["the source of type 'LineState' must declare the type LineState and nothing else; it declares type LineState, type Extra"]
    );
    assert_eq!(
        keys(&body["blocks"][0]),
        BTreeSet::from(["name", "type", "retain"])
    );
    // Both or neither.
    for given in [json!({ "type": "RecipeData" }), json!({ "source": RECIPE })] {
        let a = call(&data, &p, "POST", "/check", &[], Some(given)).await;
        assert_eq!(a.status, 400);
        assert_eq!(
            a.body,
            json!({ "error": "give both type and source, or neither" })
        );
    }
    let a = call(
        &data,
        &p,
        "POST",
        "/check",
        &[],
        Some(json!({ "type": "recipedata", "source": RECIPE })),
    )
    .await;
    assert_eq!(a.status, 409, "{}", a.body);
    let a = call(&data, &p, "POST", "/check", &[], Some(json!("text"))).await;
    assert_eq!(a.status, 400);
    assert_eq!(listing(&p.data), before);
}

#[tokio::test]
async fn a_call_without_a_data_directory_or_a_known_route_is_refused() {
    let data = Data::default();
    let mut call = HttpCall {
        method: "GET".into(),
        route: "/types".into(),
        params: Default::default(),
        query: Default::default(),
        body: None,
        identity: None,
        project: Some("plant".into()),
        data_dir: None,
        tags: None,
    };
    let (reply, event) = data.serve(&call).await;
    assert_eq!(reply.status, 500);
    assert_eq!(
        reply.body,
        json!({ "error": "the server gave the data module no data directory" })
    );
    assert_eq!(event, None);
    let p = project();
    call.data_dir = Some(p.data.display().to_string());
    call.route = "/nothing".into();
    assert_eq!(data.serve(&call).await.0.status, 404);
    // The body of a type is its source.
    let a = Answer::from(
        data.serve(&HttpCall {
            method: "PUT".into(),
            route: "/types/:type".into(),
            params: [("type".to_string(), "RecipeData".to_string())].into(),
            body: Some(json!({ "text": RECIPE })),
            ..call.clone()
        })
        .await,
    );
    assert_eq!(a.status, 400);
    assert!(a.body["error"]
        .as_str()
        .unwrap()
        .starts_with("the body is not what this route takes: "));
    assert!(listing(&p.data).is_empty());
}

impl From<(HttpReply, Option<Value>)> for Answer {
    fn from((reply, event): (HttpReply, Option<Value>)) -> Self {
        Answer {
            status: reply.status,
            body: reply.body,
            event,
        }
    }
}

#[test]
fn the_manifest_declares_eight_routes_each_a_tool_of_its_own() {
    let m = manifest();
    assert_eq!(m.id, MODULE_ID);
    assert_eq!(m.provides, PROVIDES);
    assert_eq!(m.version, "0.1.0");
    assert_eq!(m.kind, ModuleKind::Service);
    assert!(!m.always_on);
    let tools: Vec<(&str, &str, &str, Option<&str>)> = m
        .routes
        .iter()
        .map(|r| {
            (
                r.method.as_str(),
                r.path.as_str(),
                r.tool.as_str(),
                r.role.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        tools,
        [
            ("GET", "/types", "list_data_types", Some("viewer")),
            ("GET", "/types/:type", "get_data_type", Some("viewer")),
            ("PUT", "/types/:type", "save_data_type", Some("engineer")),
            (
                "DELETE",
                "/types/:type",
                "delete_data_type",
                Some("engineer")
            ),
            ("GET", "/blocks", "list_data_blocks", Some("viewer")),
            ("PUT", "/blocks/:block", "save_data_block", Some("engineer")),
            (
                "DELETE",
                "/blocks/:block",
                "delete_data_block",
                Some("engineer")
            ),
            ("POST", "/check", "check_data", Some("viewer")),
        ]
    );
    for r in &m.routes {
        assert_eq!(r.scope, RouteScope::Project);
        assert!(r.needs.is_empty());
        assert!(!r.tool.contains("logic"), "{}", r.tool);
        assert!(
            r.summary.contains("nothing is written, built or loaded")
                || r.summary.contains("Nothing is built or loaded")
                || r.summary.contains("Nothing is written, built or loaded"),
            "{}: {}",
            r.tool,
            r.summary
        );
    }
}
