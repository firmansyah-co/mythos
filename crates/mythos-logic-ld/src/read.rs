//! A TC6 document to the syntax trees of its POUs' interfaces.
//!
//! The part of PLCopen TC6 XML v2.01 taken (the schema's names; everything else is refused
//! by name where it is):
//!
//! - `project`, in the namespace [`TC6_NAMESPACE`]; its `fileHeader` and `contentHeader` are
//!   not read. `types/dataTypes` and `instances/configurations` must be empty: a data type
//!   is declared in the Structured Text source, a task in the logic module.
//! - `types/pous/pou`: `name`, `pouType` (`program`, `functionBlock`, `function`), and
//!   - `interface`: `returnType` (a function's), `inputVars`, `outputVars`, `localVars`, each
//!     a `varList` with `retain` or `nonretain` where ST takes RETAIN and NON_RETAIN; each
//!     `variable` with `name`, `type` and `initialValue`. `tempVars`, `inOutVars`,
//!     `externalVars`, `globalVars`, `accessVars`, `constant`, `persistent` and `address` are
//!     refused, as ST refuses VAR_TEMP, VAR_IN_OUT, VAR_EXTERNAL, VAR_GLOBAL, CONSTANT and AT.
//!   - `type`: the elementary types by name (`BOOL` … `TIME`; the others reach the ST checker,
//!     which refuses them as it does in ST), `string` with its `length`, `array` with its
//!     `dimension`s and `baseType`, `derived` (a structure, a named array, a function block
//!     type). `enum`, `struct`, the subranges, `pointer` and the generic types are refused.
//!   - `initialValue`: `simpleValue` (its `value` read as ST reads an initial value) and
//!     `arrayValue` (with `repetitionValue`); `structValue` is refused.
//!   - one `body` holding `LD` (lowered by `lower`); `actions` and `transitions` must be
//!     empty, and a body in IL, ST, FBD or SFC is refused.

use crate::xml::Node;
use crate::{expression, Place, Places, TC6_NAMESPACE};
use mythos_logic_ir::{is_keyword, Diagnostic, Retention, Section};
use mythos_logic_st::ast::{self, ArrayInit, Init, PouKind, TypeSpec, TypeSpecKind, VarDecl};
use mythos_logic_st::lexer::{self, Tok};

/// An LD POU whose interface has been read, and the LD body it has.
#[derive(Debug, Clone)]
pub(crate) struct LdPou {
    pub doc: usize,
    /// Its name, kind, result type and variables; no statements.
    pub tree: ast::Pou,
    /// Its `LD` element, when it has one that can be lowered.
    pub body: Option<Node>,
    /// The keywords its interface declared as names.
    pub keywords_as_names: Vec<String>,
}

/// What a body may call, by name in upper case: the functions (with their result type, when
/// a declaration states it) and the function block types of every source and of the library.
pub(crate) struct Known {
    functions: Vec<(String, Option<TypeSpec>)>,
    blocks: Vec<String>,
    /// The BOOL outputs of the function block types of the sources, by type and formal
    /// parameter in upper case.
    bool_outputs: Vec<(String, String)>,
    /// Those of the standard library's, read from it when a body first asks.
    library_bool_outputs: std::cell::OnceCell<Vec<(String, String)>>,
}

impl Known {
    pub fn of<'a>(st: &ast::Source, ld: impl Iterator<Item = &'a LdPou>) -> Known {
        let mut k = Known {
            functions: Vec::new(),
            blocks: Vec::new(),
            bool_outputs: Vec::new(),
            library_bool_outputs: std::cell::OnceCell::new(),
        };
        for p in st.pous.iter().chain(ld.map(|p| &p.tree)) {
            let upper = p.name.to_ascii_uppercase();
            match p.kind {
                PouKind::Function => k.functions.push((upper, p.result.clone())),
                PouKind::FunctionBlock => {
                    k.bool_outputs.extend(bool_outputs(p));
                    k.blocks.push(upper);
                }
                PouKind::Program => {}
            }
        }
        k
    }

    /// Whether the output `formal` (upper case) of the function block type `upper` is
    /// declared BOOL: the project's type, or else the standard library's.
    pub fn output_is_bool(&self, upper: &str, formal: &str) -> bool {
        let wanted = |(t, f): &(String, String)| t == upper && f == formal;
        if self.blocks.iter().any(|b| b == upper) {
            return self.bool_outputs.iter().any(wanted);
        }
        self.library_bool_outputs
            .get_or_init(|| {
                (0..mythos_logic_st::library::FILES.len())
                    .flat_map(|file| mythos_logic_st::library::read(file).0.pous)
                    .filter(|p| p.kind == PouKind::FunctionBlock)
                    .flat_map(|p| bool_outputs(&p))
                    .collect()
            })
            .iter()
            .any(wanted)
    }

    /// A function block type: the project's or the standard library's.
    pub fn is_block(&self, upper: &str) -> bool {
        self.blocks.iter().any(|b| b == upper)
            || matches!(
                mythos_logic_st::library::find(upper),
                Some((PouKind::FunctionBlock, _))
            )
    }

    /// The result type of a function of the project.
    pub fn result_of(&self, upper: &str) -> Option<&TypeSpec> {
        self.functions
            .iter()
            .find(|(n, _)| n == upper)
            .and_then(|(_, r)| r.as_ref())
    }
}

/// The BOOL outputs of function block `p`: its name and each output's, in upper case.
fn bool_outputs(p: &ast::Pou) -> Vec<(String, String)> {
    p.vars
        .iter()
        .filter(|v| v.section == mythos_logic_ir::Section::Output)
        .filter(
            |v| matches!(&v.ty.kind, ast::TypeSpecKind::Named(n) if n.eq_ignore_ascii_case("BOOL")),
        )
        .map(|v| (p.name.to_ascii_uppercase(), v.name.to_ascii_uppercase()))
        .collect()
}

/// Whether `text` is a name a program may declare: one identifier (IEC 61131-3 ed. 3
/// §6.1.2, as the ST lexer reads it), not a keyword. What is wrong is said at `line`.
pub(crate) fn name(
    places: &mut Places,
    at: u32,
    field: &str,
    text: &str,
    diags: &mut Vec<Diagnostic>,
) -> bool {
    let line = places.text(at, field, text);
    let lexed = lexer::tokens_at(text, line);
    if !lexed.diags.is_empty() {
        diags.extend(lexed.diags);
        return false;
    }
    match lexed.toks.as_slice() {
        [w, end] if matches!(end.tok, Tok::Eof) => match &w.tok {
            Tok::Word(upper, _) if is_keyword(upper) => {
                places.keywords_as_names.push(upper.clone());
                diags.push(places.diag(
                    line,
                    format!("{text} is a keyword of IEC 61131-3 and cannot be used as a name"),
                ));
                false
            }
            Tok::Word(..) => true,
            _ => {
                diags.push(places.diag(line, format!("{text} is not a name")));
                false
            }
        },
        _ => {
            diags.push(places.diag(line, format!("'{text}' is not a name")));
            false
        }
    }
}

/// A `true` or `false` attribute (xsd:boolean: also `1` and `0`), `false` when absent.
pub(crate) fn flag(
    node: &Node,
    attr: &str,
    places: &Places,
    line: u32,
    diags: &mut Vec<Diagnostic>,
) -> bool {
    match node.attr(attr) {
        None | Some("false") | Some("0") => false,
        Some("true") | Some("1") => true,
        Some(other) => {
            diags.push(places.diag(line, format!("{attr}=\"{other}\" is not true or false")));
            false
        }
    }
}

/// The POUs of a TC6 document whose root is `root`, with what is wrong with it; `None` when
/// it is not a TC6 project, so what it declares is not known.
pub(crate) fn document(
    root: &Node,
    namespace: Option<&str>,
    doc: usize,
    places: &mut Places,
    diags: &mut Vec<Diagnostic>,
) -> Option<Vec<LdPou>> {
    let at = places.add(Place {
        doc,
        pou: String::new(),
        local_id: None,
        position: None,
        variable: None,
        field: None,
        len: 0,
        tag: root.span,
    });
    let whole = |places: &Places, message: String| {
        let mut d = places.diag(at, message);
        // About the document, not an element of a POU.
        d.end_col = d.col;
        d
    };
    if root.name != "project" || namespace != Some(TC6_NAMESPACE) {
        diags.push(whole(
            places,
            format!(
                "the document is not a PLCopen TC6 XML v2.01 project: its root is {} in the namespace {}; one is <project xmlns=\"{TC6_NAMESPACE}\">",
                root.name,
                namespace.unwrap_or("of none")
            ),
        ));
        return None;
    }
    let Some(types) = root.child("types") else {
        diags.push(whole(
            places,
            "the project has no types element, so no POUs".into(),
        ));
        return None;
    };
    if types
        .child("dataTypes")
        .is_some_and(|d| !d.children.is_empty())
    {
        diags.push(whole(
            places,
            "data types declared in a TC6 document are not read yet: declare them in the Structured Text source".into(),
        ));
    }
    if root
        .child("instances")
        .and_then(|i| i.child("configurations"))
        .is_some_and(|c| !c.children.is_empty())
    {
        diags.push(whole(
            places,
            "configurations are not read: a program's task is set in the logic module".into(),
        ));
    }
    let mut out = Vec::new();
    for pou in types
        .child("pous")
        .into_iter()
        .flat_map(|p| p.children_named("pou"))
    {
        if let Some(p) = read_pou(pou, doc, places, diags) {
            out.push(p);
        }
    }
    Some(out)
}

fn read_pou(
    node: &Node,
    doc: usize,
    places: &mut Places,
    diags: &mut Vec<Diagnostic>,
) -> Option<LdPou> {
    let pou_name = node.attr("name").unwrap_or("").to_string();
    places.keywords_as_names.clear();
    let line = places.add(Place {
        doc,
        pou: pou_name.clone(),
        local_id: None,
        position: None,
        variable: None,
        field: None,
        len: 0,
        tag: node.span,
    });
    if !name(places, line, "name", &pou_name, diags) {
        return None;
    }
    let kind = match node.attr("pouType") {
        Some("program") => PouKind::Program,
        Some("functionBlock") => PouKind::FunctionBlock,
        Some("function") => PouKind::Function,
        other => {
            diags.push(places.diag(
                line,
                format!(
                    "{pou_name} is a POU of the type {}; one is a program, a functionBlock or a function",
                    other.unwrap_or("not given")
                ),
            ));
            return None;
        }
    };
    let mut tree = ast::Pou {
        kind,
        name: pou_name.clone(),
        span: places.span(line),
        result: None,
        vars: Vec::new(),
        body: Vec::new(),
        partial_vars: false,
        partial_body: false,
        library: false,
    };
    let before = diags.len();
    if let Some(interface) = node.child("interface") {
        interface_of(interface, &mut tree, doc, places, diags);
    }
    if kind == PouKind::Function && tree.result.is_none() && diags.len() == before {
        diags.push(places.diag(
            line,
            format!("the function {pou_name} has no returnType in its interface"),
        ));
    }
    for what in ["actions", "transitions"] {
        if node.child(what).is_some_and(|n| !n.children.is_empty()) {
            diags.push(places.diag(line, format!("{what} of a POU are not read yet (SFC)")));
        }
    }
    tree.partial_vars = diags.len() != before;
    let bodies: Vec<&Node> = node.children_named("body").collect();
    let mut body = None;
    match bodies.as_slice() {
        [] => diags.push(places.diag(line, format!("{pou_name} has no body"))),
        [b] => {
            let lang = b
                .children
                .iter()
                .find(|c| matches!(c.name.as_str(), "IL" | "ST" | "FBD" | "LD" | "SFC"));
            match lang {
                Some(l) if l.name == "LD" => body = Some(l.clone()),
                Some(l) => diags.push(places.diag(
                    line,
                    format!(
                        "{pou_name} has a body in {}; this front-end reads LD (write ST in a .st source)",
                        l.name
                    ),
                )),
                None => diags.push(places.diag(line, format!("{pou_name} has an empty body"))),
            }
        }
        _ => diags.push(places.diag(
            line,
            format!("{pou_name} has {} bodies; a POU has one", bodies.len()),
        )),
    }
    if body.is_none() {
        tree.partial_body = true;
    }
    Some(LdPou {
        doc,
        tree,
        body,
        keywords_as_names: std::mem::take(&mut places.keywords_as_names),
    })
}

fn interface_of(
    interface: &Node,
    pou: &mut ast::Pou,
    doc: usize,
    places: &mut Places,
    diags: &mut Vec<Diagnostic>,
) {
    for section in &interface.children {
        let at = places.add(Place {
            doc,
            pou: pou.name.clone(),
            local_id: None,
            position: None,
            variable: None,
            field: None,
            len: 0,
            tag: section.span,
        });
        let kind = match section.name.as_str() {
            "returnType" => {
                if pou.kind != PouKind::Function {
                    diags.push(places.diag(
                        at,
                        format!("{} is not a function, so it has no returnType", pou.name),
                    ));
                    continue;
                }
                let field = places.text(at, "returnType", "");
                pou.result = type_spec(section, field, places, diags);
                continue;
            }
            "inputVars" => Section::Input,
            "outputVars" => Section::Output,
            "localVars" => Section::Var,
            "tempVars" => {
                diags.push(places.diag(at, "tempVars (VAR_TEMP) is not supported yet"));
                continue;
            }
            "inOutVars" => {
                diags.push(places.diag(at, "inOutVars (VAR_IN_OUT) is not supported yet"));
                continue;
            }
            "externalVars" | "globalVars" => {
                diags.push(places.diag(
                    at,
                    format!(
                        "{} (VAR_EXTERNAL, VAR_GLOBAL) is not supported yet",
                        section.name
                    ),
                ));
                continue;
            }
            "accessVars" => {
                diags.push(places.diag(at, "accessVars (VAR_ACCESS) is not supported yet"));
                continue;
            }
            "addData" | "documentation" => continue,
            other => {
                diags.push(places.diag(at, format!("{other} is not a section of an interface")));
                continue;
            }
        };
        let retain = flag(section, "retain", places, at, diags);
        let nonretain = flag(section, "nonretain", places, at, diags);
        if flag(section, "constant", places, at, diags) {
            if retain || nonretain {
                diags.push(places.diag(
                    at,
                    "CONSTANT does not combine with RETAIN or NON_RETAIN: a section takes one of the three (IEC 61131-3 ed. 3 Annex A)",
                ));
            } else {
                diags.push(places.diag(at, "CONSTANT is not supported yet"));
            }
            continue;
        }
        for attr in ["persistent", "nonpersistent"] {
            if flag(section, attr, places, at, diags) {
                diags.push(places.diag(at, format!("{attr} (PERSISTENT) is not supported yet")));
            }
        }
        let retention = match (retain, nonretain) {
            (true, true) => {
                diags.push(places.diag(at, "a section is RETAIN or NON_RETAIN, not both"));
                continue;
            }
            (true, false) => Retention::Retain,
            (false, true) => Retention::NonRetain,
            (false, false) => Retention::Unqualified,
        };
        if retention != Retention::Unqualified && pou.kind == PouKind::Function {
            diags.push(places.diag(
                at,
                "RETAIN and NON_RETAIN are not allowed in a FUNCTION: a function keeps nothing from one call to the next, so there is nothing to retain (IEC 61131-3 ed. 3 §6.5.6.1, §6.6.2.1)",
            ));
            continue;
        }
        for v in section.children_named("variable") {
            if let Some(d) = variable(v, kind, retention, pou, doc, places, diags) {
                pou.vars.push(d);
            }
        }
    }
}

fn variable(
    v: &Node,
    section: Section,
    retention: Retention,
    pou: &ast::Pou,
    doc: usize,
    places: &mut Places,
    diags: &mut Vec<Diagnostic>,
) -> Option<VarDecl> {
    let var_name = v.attr("name").unwrap_or("").to_string();
    let at = places.add(Place {
        doc,
        pou: pou.name.clone(),
        local_id: None,
        position: None,
        variable: Some(var_name.clone()),
        field: None,
        len: 0,
        tag: v.span,
    });
    if !name(places, at, "name", &var_name, diags) {
        return None;
    }
    if v.attr("address").is_some() {
        diags.push(places.diag(
            at,
            "AT %I/%Q addresses are not used: bind a variable to a tag in the variable table instead",
        ));
        return None;
    }
    let Some(ty) = v.child("type") else {
        diags.push(places.diag(at, format!("{var_name} has no type")));
        return None;
    };
    let ty_line = places.text(at, "type", "");
    let ty = type_spec(ty, ty_line, places, diags)?;
    let init = match v.child("initialValue") {
        Some(init) => Some(initial(init, at, places, diags)?),
        None => None,
    };
    let name_line = places.text(at, "name", &var_name);
    Some(VarDecl {
        name: var_name,
        span: places.span(name_line),
        section,
        retention,
        ty,
        init,
        hidden: false,
    })
}

/// The type a TC6 `dataType` element states. Its children are the type: one of them.
pub(crate) fn type_spec(
    node: &Node,
    at: u32,
    places: &mut Places,
    diags: &mut Vec<Diagnostic>,
) -> Option<TypeSpec> {
    let span = places.span(at);
    let kinds: Vec<&Node> = node
        .children
        .iter()
        .filter(|c| !matches!(c.name.as_str(), "addData" | "documentation"))
        .collect();
    let [t] = kinds.as_slice() else {
        diags.push(places.diag(at, "a type names one type"));
        return None;
    };
    let kind = match t.name.as_str() {
        "string" => {
            let length = match t.attr("length") {
                None => None,
                Some(n) => match n.trim().parse::<i128>() {
                    Ok(n) => Some((n, span)),
                    Err(_) => {
                        diags.push(
                            places
                                .diag(at, format!("the length of a string is a number, not '{n}'")),
                        );
                        return None;
                    }
                },
            };
            TypeSpecKind::String(length)
        }
        "wstring" => TypeSpecKind::Named("WSTRING".into()),
        "array" => {
            let mut dims = Vec::new();
            for d in t.children_named("dimension") {
                let bound = |which: &str, diags: &mut Vec<Diagnostic>| -> Option<i128> {
                    let text = d.attr(which).unwrap_or("");
                    match text.trim().parse::<i128>() {
                        Ok(n) => Some(n),
                        Err(_) => {
                            diags.push(places.diag(
                                at,
                                format!(
                                    "the {which} bound of an array is a whole number, not '{text}'"
                                ),
                            ));
                            None
                        }
                    }
                };
                dims.push((bound("lower", diags)?, bound("upper", diags)?));
            }
            if dims.is_empty() {
                diags.push(places.diag(at, "an array has at least one dimension"));
                return None;
            }
            let Some(base) = t.child("baseType") else {
                diags.push(places.diag(at, "an array has a baseType"));
                return None;
            };
            TypeSpecKind::Array(dims, Box::new(type_spec(base, at, places, diags)?))
        }
        "derived" => {
            let n = t.attr("name").unwrap_or("");
            if !name(places, at, "type", n, diags) {
                return None;
            }
            TypeSpecKind::Named(n.to_string())
        }
        "enum" | "struct" | "subrangeSigned" | "subrangeUnsigned" | "pointer" => {
            diags.push(places.diag(
                at,
                format!("a type declared in place ({}) is not supported; declare it in the Structured Text source and name it", t.name),
            ));
            return None;
        }
        n if n.starts_with("ANY") => {
            diags.push(places.diag(
                at,
                format!("{n} is a generic type, which a variable cannot have"),
            ));
            return None;
        }
        elementary => TypeSpecKind::Named(elementary.to_string()),
    };
    Some(TypeSpec { span, kind })
}

/// An initial value: `simpleValue` as ST reads `:= value`, `arrayValue` as `[…]`.
fn initial(node: &Node, at: u32, places: &mut Places, diags: &mut Vec<Diagnostic>) -> Option<Init> {
    let Some(value) = node.children.iter().find(|c| c.name.ends_with("Value")) else {
        diags.push(places.diag(at, "an initialValue holds a simpleValue or an arrayValue"));
        return None;
    };
    match value.name.as_str() {
        "simpleValue" => {
            let text = value.attr("value").unwrap_or("");
            Some(Init::Value(expression(
                places,
                at,
                "initialValue",
                text,
                diags,
            )?))
        }
        "arrayValue" => {
            let items = array_items(value, at, places, diags)?;
            Some(Init::Array(items, places.span(at)))
        }
        other => {
            diags.push(places.diag(
                at,
                format!("an initial value as a {other} is not supported"),
            ));
            None
        }
    }
}

fn array_items(
    node: &Node,
    at: u32,
    places: &mut Places,
    diags: &mut Vec<Diagnostic>,
) -> Option<Vec<ArrayInit>> {
    let mut items = Vec::new();
    for v in node.children_named("value") {
        // A nested arrayValue is the elements of an inner dimension, the rightmost varying
        // fastest, as ST writes them in one list (IEC 61131-3 ed. 3 §6.4.4.5.2).
        let inner = match v.children.iter().find(|c| c.name.ends_with("Value")) {
            Some(s) if s.name == "simpleValue" => {
                let text = s.attr("value").unwrap_or("");
                vec![ArrayInit::Value(expression(
                    places,
                    at,
                    "initialValue",
                    text,
                    diags,
                )?)]
            }
            Some(a) if a.name == "arrayValue" => array_items(a, at, places, diags)?,
            _ => {
                diags.push(places.diag(
                    at,
                    "an element of an arrayValue is a simpleValue or an arrayValue",
                ));
                return None;
            }
        };
        match v.attr("repetitionValue").map(str::trim) {
            None | Some("1") => items.extend(inner),
            Some(n) => match n.parse::<i128>() {
                Ok(n) => items.push(ArrayInit::Repeat(n, inner, places.span(at))),
                Err(_) => {
                    diags.push(places.diag(
                        at,
                        format!("a repetitionValue is a whole number, not '{n}'"),
                    ));
                    return None;
                }
            },
        }
    }
    Some(items)
}
