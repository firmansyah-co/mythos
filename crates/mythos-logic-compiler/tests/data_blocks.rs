//! Library sources and data blocks (ADR-014, slice D1): a program compiled with the project's
//! types, functions and function block types, and with its own copy of each data block it
//! declares in `VAR_EXTERNAL`; what `Compiled.data` says of each, and the layout hash.

mod support;
use support::*;

use mythos_logic_compiler::{
    check_project, compile, compile_sources, Compiled, DataBlock, DataUse, Diagnostic, Sources,
    TargetProfile,
};
use mythos_logic_ir::{Section, Ty};

/// A structure, a function and a function block, in two library sources.
const TYPES: (&str, &str) = (
    "types.st",
    "TYPE Point : STRUCT x : INT; y : INT; END_STRUCT; END_TYPE
FUNCTION Twice : INT VAR_INPUT a : INT; END_VAR Twice := a * 2; END_FUNCTION",
);
const BLOCKS: (&str, &str) = (
    "blocks.st",
    "FUNCTION_BLOCK Acc
  VAR_INPUT x : INT; y : INT; END_VAR
  VAR_OUTPUT sum : INT; END_VAR
  sum := sum + Twice(x) + y;
END_FUNCTION_BLOCK",
);
const USES_THE_LIBRARY: &str = "PROGRAM Main
  VAR pt : Point; acc : Acc; out : INT; END_VAR
  pt.x := 3; pt.y := 4;
  acc(x := pt.x, y := Twice(pt.y));
  out := acc.sum;
END_PROGRAM";

fn with(
    st: &str,
    library: &[(&str, &str)],
    data: &[DataBlock],
) -> Result<Compiled, Vec<Diagnostic>> {
    compile_sources(
        &Sources {
            st: Some(("main.st", st)),
            library,
            data,
            ..Default::default()
        },
        &TargetProfile::pc(),
    )
}

fn compiled(st: &str, library: &[(&str, &str)], data: &[DataBlock]) -> Compiled {
    with(st, library, data).unwrap_or_else(|d| {
        panic!(
            "{}",
            d.iter()
                .map(|d| d.to_string())
                .collect::<Vec<_>>()
                .join("\n")
        )
    })
}

fn refused(st: &str, library: &[(&str, &str)], data: &[DataBlock]) -> Vec<Diagnostic> {
    match with(st, library, data) {
        Ok(_) => panic!("compiled:\n{st}"),
        Err(d) => d,
    }
}

fn messages(d: &[Diagnostic]) -> Vec<String> {
    d.iter().map(|d| d.message.clone()).collect()
}

/// The written members of a data block, by name.
fn written(d: &DataUse) -> Vec<&str> {
    d.members
        .iter()
        .filter(|m| m.written)
        .map(|m| m.name.as_str())
        .collect()
}

#[test]
fn a_program_using_two_library_sources_computes_what_it_does_with_everything_in_one_source() {
    let mut from_library = Run::of(with(USES_THE_LIBRARY, &[TYPES, BLOCKS], &[]));
    let one = format!("{}\n{}\n{USES_THE_LIBRARY}", TYPES.1, BLOCKS.1);
    let mut one_source = Run::new(&one);
    for _ in 0..3 {
        from_library.scan().unwrap();
        one_source.scan().unwrap();
        assert_eq!(from_library.int("out"), one_source.int("out"));
    }
    // 3 scans of sum := sum + 2 * 3 + 2 * 4.
    assert_eq!(from_library.int("out"), 42);
}

#[test]
fn a_library_source_the_program_does_not_use_changes_nothing_compiled() {
    let program =
        "PROGRAM Main VAR a : INT; b : DINT; END_VAR a := a + 1; b := INT_TO_DINT(a); END_PROGRAM";
    let alone = compile(program, &TargetProfile::pc()).unwrap();
    let unused = (
        "unused.st",
        "TYPE Big : STRUCT a : ARRAY [0..99] OF LREAL; END_STRUCT; END_TYPE
         FUNCTION Unused : INT VAR_INPUT v : INT; END_VAR Unused := v; END_FUNCTION
         FUNCTION_BLOCK Never VAR b : Big; t : TON; END_VAR t(IN := TRUE); END_FUNCTION_BLOCK",
    );
    for library in [&[unused][..], &[TYPES, BLOCKS, unused][..]] {
        let with_it = compiled(program, library, &[]);
        assert_eq!(with_it.bytecode, alone.bytecode);
        assert_eq!(with_it.symbols, alone.symbols);
        assert_eq!(with_it.types, alone.types);
        assert_eq!(with_it.memory_size, alone.memory_size);
        assert!(with_it.data.is_empty());
    }
}

#[test]
fn a_mistake_in_a_part_of_the_library_no_program_uses_is_told_with_its_source() {
    let broken = (
        "broken.st",
        "FUNCTION Fine : INT Fine := 1; END_FUNCTION\nFUNCTION Broken : INT\n  Broken := nowhere;\nEND_FUNCTION",
    );
    let d = refused(
        "PROGRAM Main VAR a : INT; END_VAR a := 1; END_PROGRAM",
        &[TYPES, broken],
        &[],
    );
    assert_eq!(messages(&d), ["nowhere is not declared"]);
    assert_eq!(d[0].source.as_deref(), Some("broken.st"));
    assert_eq!((d[0].line, d[0].col), (3, 13));

    // A PROGRAM in a library source.
    let program = ("prog.st", "PROGRAM Other END_PROGRAM");
    let d = refused("PROGRAM Main END_PROGRAM", &[program], &[]);
    assert_eq!(
        messages(&d),
        ["Other is a PROGRAM in the library source 'prog.st'; a library holds types, functions and function blocks"]
    );
    assert_eq!(d[0].source.as_deref(), Some("prog.st"));

    // A name declared in two sources names both; twice in one, as for one source.
    let again = ("again.st", "FUNCTION Twice : INT Twice := 2; END_FUNCTION");
    let d = refused("PROGRAM Main END_PROGRAM", &[TYPES, again], &[]);
    assert_eq!(
        messages(&d),
        ["Twice is declared twice: in the library source 'types.st' and in the library source 'again.st'"]
    );
    assert_eq!(d[0].source.as_deref(), Some("again.st"));
    let d = refused(
        "PROGRAM Main END_PROGRAM FUNCTION Twice : INT Twice := 2; END_FUNCTION",
        &[TYPES],
        &[],
    );
    assert_eq!(
        messages(&d),
        ["Twice is declared twice: in the program and in the library source 'types.st'"]
    );
    let twice = (
        "twice.st",
        "FUNCTION F : INT F := 1; END_FUNCTION FUNCTION F : INT F := 2; END_FUNCTION",
    );
    let d = refused("PROGRAM Main END_PROGRAM", &[twice], &[]);
    assert_eq!(messages(&d), ["F is declared twice"]);
    assert_eq!(d[0].source.as_deref(), Some("twice.st"));

    // The program's own mistakes name its source, now that there are several.
    let d = refused("PROGRAM Main\n  x := 1;\nEND_PROGRAM", &[TYPES], &[]);
    assert_eq!(messages(&d), ["x is not declared"]);
    assert_eq!((d[0].source.as_deref(), d[0].line), (Some("main.st"), 2));
}

#[test]
fn with_no_library_and_no_data_var_global_and_var_external_are_refused() {
    let pc = TargetProfile::pc();
    let first = |src: &str| {
        let d = compile(src, &pc).err().expect("refused");
        format!("{}:{}: {}", d[0].line, d[0].col, d[0].message)
    };
    assert_eq!(
        first("VAR_GLOBAL g : INT; END_VAR PROGRAM Main END_PROGRAM"),
        "1:1: VAR_GLOBAL is not supported yet"
    );
    assert_eq!(
        first("PROGRAM Main VAR_GLOBAL g : INT; END_VAR END_PROGRAM"),
        "1:14: VAR_GLOBAL is not supported yet"
    );
    assert_eq!(
        first("PROGRAM Main VAR_EXTERNAL g : INT; END_VAR END_PROGRAM"),
        "1:27: there is no data block g in the project"
    );
    // RETAIN on it, as it was.
    assert_eq!(
        first("PROGRAM Main VAR_EXTERNAL RETAIN x : INT; END_VAR END_PROGRAM"),
        "1:27: RETAIN is not allowed on VAR_EXTERNAL: the variable is retained or not where VAR_GLOBAL declares it (IEC 61131-3 ed. 3 Annex A, External_Var_Decls)"
    );
}

const RECIPES: (&str, &str) = (
    "recipes.st",
    "TYPE
  RecipeData : STRUCT speed : INT; temp : REAL; END_STRUCT;
  Pair : ARRAY [0..1] OF INT;
END_TYPE
FUNCTION_BLOCK Valve VAR_INPUT open : BOOL; END_VAR END_FUNCTION_BLOCK",
);
const RECIPE: DataBlock = DataBlock {
    name: "Recipe",
    ty: "RecipeData",
};

#[test]
fn each_refusal_of_a_data_block_has_its_message() {
    let program = "PROGRAM Main END_PROGRAM";
    // The project's declarations.
    for (block, message) in [
        (
            DataBlock { name: "1st", ty: "RecipeData" },
            "data block '1st': the name is not an identifier (letters, digits and single underscores, not beginning with a digit)",
        ),
        (
            DataBlock { name: "WHILE", ty: "RecipeData" },
            "data block 'WHILE': WHILE is a keyword of IEC 61131-3 and cannot be used as a name",
        ),
        (
            DataBlock { name: "Valve", ty: "RecipeData" },
            "data block 'Valve': Valve is also the name of a type or POU of the project; a data block needs its own",
        ),
        (
            DataBlock { name: "TON", ty: "RecipeData" },
            "data block 'TON': TON is a name of the standard; choose another",
        ),
        (
            DataBlock { name: "B", ty: "INT" },
            "data block 'B' is a INT; a data block is an instance of a structure type",
        ),
        (
            DataBlock { name: "B", ty: "Pair" },
            "data block 'B' is a Pair; a data block is an instance of a structure type",
        ),
        (
            DataBlock { name: "B", ty: "Valve" },
            "data block 'B' is a Valve; a data block is an instance of a structure type (a function block type comes later)",
        ),
        (
            DataBlock { name: "B", ty: "TON" },
            "data block 'B' is a TON; a data block is an instance of a structure type (a function block type comes later)",
        ),
        (
            DataBlock { name: "B", ty: "Nope" },
            "data block 'B': there is no type Nope",
        ),
    ] {
        let d = refused(program, &[RECIPES], &[block]);
        assert_eq!(messages(&d), [message], "{block:?}");
    }
    let d = refused(
        program,
        &[RECIPES],
        &[
            RECIPE,
            DataBlock {
                name: "RECIPE",
                ty: "RecipeData",
            },
        ],
    );
    assert_eq!(messages(&d), ["data block 'RECIPE' is declared twice"]);
    // A structure of the program is not one of the library.
    let d = refused(
        "TYPE Local : STRUCT a : INT; END_STRUCT; END_TYPE PROGRAM Main END_PROGRAM",
        &[RECIPES],
        &[DataBlock {
            name: "B",
            ty: "Local",
        }],
    );
    assert_eq!(
        messages(&d),
        ["data block 'B' is a Local, a structure of the program; a data block is an instance of a structure type of the library"]
    );

    // A program's declaration.
    for (src, message) in [
        (
            "FUNCTION_BLOCK F VAR_EXTERNAL Recipe : RecipeData; END_VAR END_FUNCTION_BLOCK PROGRAM Main END_PROGRAM",
            "VAR_EXTERNAL in a function block or function is not supported yet: give the block the member as a parameter",
        ),
        (
            "FUNCTION F : INT VAR_EXTERNAL Recipe : RecipeData; END_VAR F := 1; END_FUNCTION PROGRAM Main END_PROGRAM",
            "VAR_EXTERNAL in a function block or function is not supported yet: give the block the member as a parameter",
        ),
        (
            "PROGRAM Main VAR_EXTERNAL Recipes : RecipeData; END_VAR END_PROGRAM",
            "there is no data block Recipes in the project",
        ),
        (
            "TYPE Point : STRUCT x : INT; END_STRUCT; END_TYPE PROGRAM Main VAR_EXTERNAL Recipe : Point; END_VAR END_PROGRAM",
            "data block Recipe is a RecipeData, not a Point",
        ),
        (
            "PROGRAM Main VAR_EXTERNAL Recipe : RecipeData := 1; END_VAR END_PROGRAM",
            "a VAR_EXTERNAL takes no initial value: the data block has it",
        ),
        (
            "PROGRAM Main VAR_EXTERNAL RETAIN Recipe : RecipeData; END_VAR END_PROGRAM",
            "RETAIN is not allowed on VAR_EXTERNAL: the variable is retained or not where VAR_GLOBAL declares it (IEC 61131-3 ed. 3 Annex A, External_Var_Decls)",
        ),
        (
            "VAR_GLOBAL Recipe : RecipeData; END_VAR PROGRAM Main END_PROGRAM",
            "VAR_GLOBAL is not supported yet",
        ),
    ] {
        let d = refused(src, &[RECIPES], &[RECIPE]);
        assert_eq!(messages(&d), [message], "{src}");
    }
}

#[test]
fn a_program_reads_and_writes_a_block_and_compiled_data_says_where_it_is() {
    let c = compiled(
        "PROGRAM Main
           VAR before : LINT; END_VAR
           VAR_EXTERNAL RECIPE : RecipeData; END_VAR
           VAR out : REAL; END_VAR
           recipe.speed := recipe.speed + 1;
           out := recipe.temp + 0.5;
         END_PROGRAM",
        &[RECIPES],
        &[RECIPE],
    );
    assert_eq!(c.data.len(), 1);
    let d = &c.data[0];
    // The project's spelling, not the program's.
    assert_eq!((d.name.as_str(), d.ty.as_str()), ("Recipe", "RecipeData"));
    assert!(!d.constant);
    assert_eq!(d.size, 6);
    let names: Vec<&str> = d.members.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(names, ["speed", "temp"]);
    for m in &d.members {
        let s = c
            .symbol(&format!("Recipe.{}", m.name))
            .unwrap_or_else(|| panic!("a symbol of {}", m.name));
        assert_eq!(
            (s.offset, s.size, s.ty.as_str()),
            (d.offset + m.offset, m.size, m.ty.as_str())
        );
        assert_eq!(s.section, Section::External);
        assert!(!s.retain && !s.writable);
        assert_eq!(s.path, format!("Recipe.{}", m.name));
    }
    assert_eq!(written(d), ["speed"]);

    let mut r = Run::of(Ok(c));
    r.set("Recipe.temp", mythos_logic_vm::Value::Real(2.0));
    r.scan().unwrap();
    r.scan().unwrap();
    assert_eq!(r.int("Recipe.speed"), 2);
    assert_eq!(r.real("out"), 2.5);
}

#[test]
fn the_symbol_section_of_a_block_is_serialised_external() {
    assert_eq!(
        serde_json::to_value(Section::External).unwrap(),
        serde_json::json!("external")
    );
}

/// A block of every kind of member the `written` analysis tells apart.
const MIXED: (&str, &str) = (
    "mixed.st",
    "TYPE
  Inner : STRUCT a : INT; b : INT; END_STRUCT;
  Mixed : STRUCT
    speed : INT;
    inner : Inner;
    arr : ARRAY [0..3] OF INT;
    count : INT;
    q : BOOL;
    r1 : INT;
    r2 : INT;
    r3 : INT;
    r4 : INT;
  END_STRUCT;
END_TYPE",
);
const M: DataBlock = DataBlock {
    name: "M",
    ty: "Mixed",
};

#[test]
fn written_is_true_for_every_kind_of_store_and_false_for_reads() {
    let c = compiled(
        "PROGRAM Main
           VAR_EXTERNAL M : Mixed; END_VAR
           VAR i : INT; x : INT; f : BOOL; t : R_TRIG; acc : Acc; END_VAR
           M.speed := 5;
           M.inner.a := 1;
           i := i + 1;
           M.arr[i MOD 4] := 7;
           FOR i := 1 TO 3 DO x := x + M.r1; END_FOR;
           t(CLK := TRUE, Q => M.q);
           acc(x := M.r2, y := Twice(M.r4));
           f := M.r3 > 0 OR M.r1 = M.r2;
         END_PROGRAM",
        &[MIXED, TYPES, BLOCKS],
        &[M],
    );
    assert_eq!(c.data.len(), 1);
    // A FOR loop's control variable is a variable named alone today (a member of a block is
    // refused as one), so that store is proved on the IR (`Unit::stores`).
    assert_eq!(written(&c.data[0]), ["speed", "inner", "arr", "q"]);

    // The whole block assigned: every member.
    let c = compiled(
        "PROGRAM Main VAR_EXTERNAL M : Mixed; END_VAR VAR other : Mixed; END_VAR
           M := other;
         END_PROGRAM",
        &[MIXED],
        &[M],
    );
    assert!(c.data[0].members.iter().all(|m| m.written));

    // Only read, compared, passed: nothing.
    let c = compiled(
        "PROGRAM Main VAR_EXTERNAL M : Mixed; END_VAR VAR other : Mixed; x : INT; f : BOOL; END_VAR
           other := M; x := M.arr[1] + M.inner.b; f := M.q;
           x := Twice(M.speed);
         END_PROGRAM",
        &[MIXED, TYPES],
        &[M],
    );
    assert!(written(&c.data[0]).is_empty());
    assert!(!c.data[0].constant);
}

#[test]
fn var_external_constant_reads_compile_and_every_store_is_refused() {
    let c = compiled(
        "PROGRAM Main VAR_EXTERNAL CONSTANT M : Mixed; END_VAR VAR x : INT; f : BOOL; END_VAR
           x := M.speed + M.inner.a + M.arr[x MOD 4]; f := M.q;
         END_PROGRAM",
        &[MIXED],
        &[M],
    );
    assert!(c.data[0].constant);
    assert!(written(&c.data[0]).is_empty());

    let message = "M is declared VAR_EXTERNAL CONSTANT: the program reads it and does not write it";
    for store in [
        "M.speed := 1;",
        "M.inner.a := 1;",
        "M.arr[x MOD 4] := 1;",
        "M := other;",
        "t(CLK := TRUE, Q => M.q);",
    ] {
        let src = format!(
            "PROGRAM Main VAR_EXTERNAL CONSTANT M : Mixed; END_VAR
               VAR x : INT; other : Mixed; t : R_TRIG; END_VAR
               {store}
             END_PROGRAM"
        );
        let d = refused(&src, &[MIXED], &[M]);
        assert_eq!(messages(&d), [message], "{store}");
    }
    // A member as a FOR loop's control variable is refused for every variable, as before.
    let d = refused(
        "PROGRAM Main VAR_EXTERNAL CONSTANT M : Mixed; END_VAR VAR x : INT; END_VAR
           FOR M.count := 1 TO 3 DO x := x + 1; END_FOR;
         END_PROGRAM",
        &[MIXED],
        &[M],
    );
    assert_eq!(
        messages(&d),
        ["a FOR variable that is a member or an element (name it alone) is not supported yet"]
    );
}

const SHARED: (&str, &str) = (
    "shared.st",
    "TYPE
  Pos : STRUCT x : DINT; y : DINT; END_STRUCT;
  Shared : STRUCT speed : INT; pos : Pos; back : INT; END_STRUCT;
END_TYPE",
);
const S: DataBlock = DataBlock {
    name: "S",
    ty: "Shared",
};
const WRITER: &str = "PROGRAM A
  VAR a1 : INT := 11; seen : INT; END_VAR
  VAR_EXTERNAL S : Shared; END_VAR
  S.speed := 40 + a1;
  S.pos.x := 7;
  S.pos.y := 8;
  seen := S.back * 2;
END_PROGRAM";
const READER: &str = "PROGRAM B
  VAR b0 : LREAL; b1 : BOOL; END_VAR
  VAR_EXTERNAL S : Shared; END_VAR
  VAR out : DINT; END_VAR
  out := INT_TO_DINT(S.speed) + S.pos.x * 100 + S.pos.y * 1000;
  S.back := 21;
END_PROGRAM";

/// Copy from `from`'s memory into `to`'s exactly the bytes of the members of block `name`
/// that `from` writes, as `Compiled.data` of each says: nothing else is used to find them.
fn hand_over(from: &Run, to: &mut Run, name: &str) {
    let src = from.c.data.iter().find(|d| d.name == name).unwrap();
    let dst = to.c.data.iter().find(|d| d.name == name).unwrap();
    assert_eq!(src.layout, dst.layout);
    for (m, n) in src.members.iter().zip(&dst.members) {
        if !m.written {
            continue;
        }
        let a = (src.offset + m.offset) as usize;
        let b = (dst.offset + n.offset) as usize;
        let len = m.size as usize;
        to.memory[b..b + len].copy_from_slice(&from.memory[a..a + len]);
    }
}

#[test]
fn a_member_written_by_one_program_reaches_another_by_what_compiled_data_says() {
    let mut a = Run::of(with(WRITER, &[SHARED], &[S]));
    let mut b = Run::of(with(READER, &[SHARED], &[S]));
    assert_eq!(written(&a.c.data[0]), ["speed", "pos"]);
    assert_eq!(written(&b.c.data[0]), ["back"]);

    a.scan().unwrap();
    hand_over(&a, &mut b, "S");
    b.scan().unwrap();
    assert_eq!(b.int("out"), 51 + 700 + 8000);

    // The other way: what B writes, and only that, goes back to A.
    hand_over(&b, &mut a, "S");
    assert_eq!(a.int("S.speed"), 51, "A's own member is not overwritten");
    a.scan().unwrap();
    assert_eq!(a.int("seen"), 42);
}

#[test]
fn the_same_block_in_two_programs_has_one_layout_and_its_own_place_in_each() {
    let a = compiled(WRITER, &[SHARED], &[S]);
    let b = compiled(READER, &[SHARED], &[S]);
    let (da, db) = (&a.data[0], &b.data[0]);
    assert_eq!(da.layout, db.layout);
    assert_eq!(da.size, db.size);
    let place = |d: &DataUse| -> Vec<(String, u32, u32)> {
        d.members
            .iter()
            .map(|m| (m.name.clone(), m.offset, m.size))
            .collect()
    };
    assert_eq!(place(da), place(db));
    assert_ne!(da.offset, db.offset);
}

/// The layout hash of a block of type `ty`, declared by `types`.
fn hash(types: &str, ty: &str) -> String {
    let layouts = check_project(&Sources {
        library: &[("types.st", types)],
        data: &[DataBlock { name: "B", ty }],
        ..Default::default()
    })
    .unwrap_or_else(|d| panic!("{d:?}"));
    layouts[0].layout.clone()
}

#[test]
fn the_layout_hash_is_pinned_and_follows_the_layout_not_the_names_of_types() {
    let small = "TYPE Small : STRUCT a : INT; s : STRING[4]; END_STRUCT; END_TYPE";
    // FNV-1a 64 of "D1;8;STRUCT(A:INT@0+2;S:STRING[4]@2+6;)".
    assert_eq!(hash(small, "Small"), "d41e45e02f9128e0");

    let base = hash(
        "TYPE In : STRUCT v : INT; END_STRUCT; T : STRUCT a : INT; b : ARRAY [1..3] OF In; s : STRING[8]; END_STRUCT; END_TYPE",
        "T",
    );
    for (what, types, ty) in [
        (
            "a member renamed",
            "TYPE In : STRUCT v : INT; END_STRUCT; T : STRUCT c : INT; b : ARRAY [1..3] OF In; s : STRING[8]; END_STRUCT; END_TYPE",
            "T",
        ),
        (
            "a member of a member renamed",
            "TYPE In : STRUCT w : INT; END_STRUCT; T : STRUCT a : INT; b : ARRAY [1..3] OF In; s : STRING[8]; END_STRUCT; END_TYPE",
            "T",
        ),
        (
            "a member retyped",
            "TYPE In : STRUCT v : INT; END_STRUCT; T : STRUCT a : DINT; b : ARRAY [1..3] OF In; s : STRING[8]; END_STRUCT; END_TYPE",
            "T",
        ),
        (
            "members reordered",
            "TYPE In : STRUCT v : INT; END_STRUCT; T : STRUCT b : ARRAY [1..3] OF In; a : INT; s : STRING[8]; END_STRUCT; END_TYPE",
            "T",
        ),
        (
            "an array bound changed",
            "TYPE In : STRUCT v : INT; END_STRUCT; T : STRUCT a : INT; b : ARRAY [0..2] OF In; s : STRING[8]; END_STRUCT; END_TYPE",
            "T",
        ),
        (
            "a STRING length changed",
            "TYPE In : STRUCT v : INT; END_STRUCT; T : STRUCT a : INT; b : ARRAY [1..3] OF In; s : STRING[9]; END_STRUCT; END_TYPE",
            "T",
        ),
    ] {
        assert_ne!(hash(types, ty), base, "{what}");
    }
    // A member retyped to one of the same size: INT, and a structure of one INT.
    assert_ne!(
        hash(
            "TYPE In : STRUCT v : INT; END_STRUCT; T : STRUCT a : In; END_STRUCT; END_TYPE",
            "T"
        ),
        hash("TYPE T : STRUCT a : INT; END_STRUCT; END_TYPE", "T"),
    );
    // The types renamed, and the block: the same.
    assert_eq!(
        hash(
            "TYPE Other : STRUCT v : INT; END_STRUCT; Renamed : STRUCT a : INT; b : ARRAY [1..3] OF Other; s : STRING[8]; END_STRUCT; END_TYPE",
            "Renamed",
        ),
        base
    );
    let renamed_block = check_project(&Sources {
        library: &[(
            "types.st",
            "TYPE In : STRUCT v : INT; END_STRUCT; T : STRUCT a : INT; b : ARRAY [1..3] OF In; s : STRING[8]; END_STRUCT; END_TYPE",
        )],
        data: &[DataBlock { name: "Elsewhere", ty: "T" }],
        ..Default::default()
    })
    .unwrap();
    assert_eq!(renamed_block[0].layout, base);
}

#[test]
fn check_project_lays_blocks_out_as_programs_do_and_needs_no_program() {
    let data = [
        S,
        DataBlock {
            name: "Other",
            ty: "Pos",
        },
    ];
    let layouts = check_project(&Sources {
        // Not looked at.
        st: Some(("main.st", "this is not Structured Text")),
        library: &[SHARED],
        data: &data,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(layouts.len(), 2);
    assert_eq!(
        (layouts[0].name.as_str(), layouts[0].ty.as_str()),
        ("S", "Shared")
    );
    assert_eq!(
        (layouts[1].name.as_str(), layouts[1].ty.as_str()),
        ("Other", "Pos")
    );
    let program = compiled(WRITER, &[SHARED], &data);
    // The program declares S only.
    assert_eq!(program.data.len(), 1);
    let d = &program.data[0];
    assert_eq!((&layouts[0].layout, layouts[0].size), (&d.layout, d.size));
    for (l, m) in layouts[0].members.iter().zip(&d.members) {
        assert_eq!(
            (&l.name, &l.ty, l.offset, l.size),
            (&m.name, &m.ty, m.offset, m.size)
        );
        assert!(!l.written);
    }
    assert_eq!(layouts[0].members.len(), d.members.len());

    // Its refusals: of the library and of the blocks.
    let d = check_project(&Sources {
        library: &[
            SHARED,
            ("bad.st", "FUNCTION F : INT F := nothing; END_FUNCTION"),
        ],
        data: &[
            S,
            DataBlock {
                name: "X",
                ty: "Nope",
            },
            DataBlock {
                name: "S",
                ty: "Pos",
            },
        ],
        ..Default::default()
    })
    .expect_err("refused");
    let mut m = messages(&d);
    m.sort();
    assert_eq!(
        m,
        [
            "data block 'S' is declared twice",
            "data block 'X': there is no type Nope",
            "nothing is not declared",
        ]
    );
    let told = d
        .iter()
        .find(|d| d.message == "nothing is not declared")
        .unwrap();
    assert_eq!(told.source.as_deref(), Some("bad.st"));

    // No library and no block: nothing to lay out.
    assert!(check_project(&Sources::default()).unwrap().is_empty());
}

#[test]
fn a_target_that_refuses_string_refuses_a_block_with_one_and_not_an_unused_type() {
    let no_text = TargetProfile {
        name: "no-text".into(),
        refuses: vec![Ty::String(254)],
        ..TargetProfile::pc()
    };
    let library = [(
        "texts.st",
        "TYPE Label : STRUCT text : STRING[8]; n : INT; END_STRUCT; Count : STRUCT n : INT; END_STRUCT; END_TYPE",
    )];
    let data = [
        DataBlock {
            name: "L",
            ty: "Label",
        },
        DataBlock {
            name: "C",
            ty: "Count",
        },
    ];
    let compile_for = |st: &str| {
        compile_sources(
            &Sources {
                st: Some(("main.st", st)),
                library: &library,
                data: &data,
                ..Default::default()
            },
            &no_text,
        )
    };
    let d = compile_for("PROGRAM Main VAR_EXTERNAL L : Label; END_VAR END_PROGRAM")
        .err()
        .expect("refused");
    assert_eq!(
        messages(&d),
        ["STRING is not supported on the target no-text"]
    );
    let c = compile_for("PROGRAM Main VAR_EXTERNAL C : Count; END_VAR C.n := C.n + 1; END_PROGRAM")
        .unwrap_or_else(|d| panic!("{d:?}"));
    assert_eq!(c.data.len(), 1);
    assert_eq!(c.data[0].name, "C");
}
