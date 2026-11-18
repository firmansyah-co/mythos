//! The logic VM backend (ADR-013 §2.1): lays an IR unit's variables out in memory and emits
//! bytecode for `mythos-logic-vm`.
//!
//! It depends on the IR and on the VM's format, and on no front-end: it compiles whatever
//! language was lowered to the IR. Another target kind (C for small microcontrollers) is
//! another crate like this one.
//!
//! **Everything has one fixed place.** A unit has no recursion, so nothing needs a frame that
//! comes and goes: the program's variables, every function block instance inside them, and
//! every function's parameters and locals are laid out once, here. A function's locals are put
//! back to their initial values by code at each call.
//!
//! **A function block's body is emitted once per type**, addressing its variables relative to
//! the instance it is called for. A function's body and the program's use absolute addresses.
//! Bodies are emitted callees first, the program last, which is the order the VM's loader
//! requires and what makes the stack and call depths known when a call is emitted.

use mythos_logic_ir::*;
use mythos_logic_vm::fb::FbKind;
use mythos_logic_vm::format::{
    crc32, op, rfn, string_size, ElemType, MAGIC, PLACE_INDEXED, PLACE_RELATIVE,
};

pub use mythos_logic_vm::FORMAT_VERSION;

/// The VM's block for a standard function block of the IR.
///
/// The IR names the blocks and their members; the VM owns how an instance is stored and run.
/// They are matched by name, and `the_vm_has_every_standard_block_of_the_ir` holds them equal.
fn vm_fb(fb: StdFb) -> FbKind {
    FbKind::ALL
        .into_iter()
        .find(|k| k.name() == fb.name())
        .unwrap_or_else(|| panic!("the VM has no function block {}", fb.name()))
}

/// Where member `m` of `fb` (an index into `fb.members()`) lies in an instance.
fn member_offset(fb: StdFb, m: usize) -> u32 {
    let name = fb.members()[m].name;
    vm_fb(fb)
        .members()
        .iter()
        .find(|x| x.name == name)
        .unwrap_or_else(|| panic!("the VM's {} has no member {name}", fb.name()))
        .offset
}

/// Where a variable (and each member of a structure or an instance) was put, for the host:
/// binding a variable to a tag, monitoring and forcing all go by these.
///
/// An array is one symbol, however many elements it has: `array` gives its element type and
/// size and its bounds, and element `[i, j]` is at `offset` plus the element's position
/// (rightmost subscript varying fastest) times `element_size`. The members of an element that
/// is a structure or a function block instance are in [`Output::types`].
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Symbol {
    /// `name`, `instance.member`, `structure.member.member`.
    pub path: String,
    #[serde(rename = "type")]
    pub ty: String,
    pub section: Section,
    pub offset: u32,
    pub size: u32,
    /// The host may set it (a program input, or part of one).
    pub writable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub array: Option<ArrayInfo>,
    /// Whether the variable keeps its value across a warm restart (IEC 61131-3 §6.5.6): declared
    /// in a RETAIN section, or a member of an instance or structure that is, as the rules say.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub retain: bool,
}

/// What an array symbol or member holds.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ArrayInfo {
    pub element_type: String,
    pub element_size: u32,
    /// Each subscript's range, inclusive.
    pub bounds: Vec<(i64, i64)>,
}

/// How a structure or a function block type is laid out, for reaching into an array's elements.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TypeLayout {
    pub name: String,
    pub size: u32,
    pub members: Vec<MemberLayout>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MemberLayout {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
    /// From the start of the structure or instance.
    pub offset: u32,
    pub size: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub array: Option<ArrayInfo>,
}

pub struct Output {
    pub bytes: Vec<u8>,
    pub symbols: Vec<Symbol>,
    pub types: Vec<TypeLayout>,
    pub memory_size: u32,
    pub code_size: u32,
    pub stack_depth: u16,
    /// How deep calls nest at most; the VM is given that many call frames.
    pub call_depth: u16,
}

/// How a value of a type sits on the VM stack.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Class {
    Bool,
    I32,
    I64,
    F32,
    F64,
}

fn class(t: Ty) -> Class {
    match t {
        Ty::Bool => Class::Bool,
        Ty::Sint | Ty::Int | Ty::Dint => Class::I32,
        Ty::Lint | Ty::Time => Class::I64,
        Ty::Real => Class::F32,
        Ty::Lreal => Class::F64,
        Ty::String(_) | Ty::Fb(_) | Ty::Struct(_) | Ty::Array(_) | Ty::Instance(_) => {
            unreachable!("only an elementary value is on the stack")
        }
    }
}

pub fn elem(t: Ty) -> ElemType {
    match t {
        Ty::Bool => ElemType::Bool,
        Ty::Sint => ElemType::Sint,
        Ty::Int => ElemType::Int,
        Ty::Dint => ElemType::Dint,
        Ty::Lint => ElemType::Lint,
        Ty::Real => ElemType::Real,
        Ty::Lreal => ElemType::Lreal,
        Ty::Time => ElemType::Time,
        Ty::String(_) | Ty::Fb(_) | Ty::Struct(_) | Ty::Array(_) | Ty::Instance(_) => {
            unreachable!("not an element")
        }
    }
}

/// The size of something and where each of its parts starts within it.
#[derive(Debug, Clone, Default)]
struct Layout {
    size: u64,
    offsets: Vec<u64>,
}

/// Sizes and layouts of every type of a unit. Sizes are kept in 64 bits and checked against
/// the format's 32 once, in `emit`, so no sum here can wrap.
struct Layouts<'u> {
    u: &'u Unit,
    structs: Vec<Layout>,
    /// For a function block type, an instance; for a function, its parameters and locals
    /// (inputs first); for the program, its variables.
    pous: Vec<Layout>,
    /// For a function: where in its layout the variables that are not inputs start. They are
    /// what a call starts afresh.
    locals: Vec<u64>,
}

impl<'u> Layouts<'u> {
    /// `order` is the unit's POUs, each after those it uses.
    fn new(u: &'u Unit, order: &[usize]) -> Layouts<'u> {
        let mut l = Layouts {
            u,
            structs: vec![Layout::default(); u.structs.len()],
            pous: vec![Layout::default(); u.pous.len()],
            locals: vec![0; u.pous.len()],
        };
        // A structure may hold one declared after it; `struct_cycle` was ruled out by the
        // caller, so going round until nothing changes ends.
        let mut done = vec![false; u.structs.len()];
        while done.iter().any(|d| !d) {
            for i in 0..u.structs.len() {
                let ready = u.structs[i]
                    .fields
                    .iter()
                    .all(|f| l.structs_in(f.ty).iter().all(|s| done[*s]));
                if done[i] || !ready {
                    continue;
                }
                let mut lay = Layout::default();
                for f in &u.structs[i].fields {
                    lay.offsets.push(lay.size);
                    lay.size = lay.size.saturating_add(l.size(f.ty));
                }
                l.structs[i] = lay;
                done[i] = true;
            }
        }
        for &p in order {
            let pou = &u.pous[p];
            let mut lay = Layout {
                size: 0,
                offsets: vec![0; pou.vars.len()],
            };
            let is_function = matches!(pou.kind, PouKind::Function { .. });
            // A function's inputs first, so the rest is one run of bytes to start afresh.
            for inputs in [true, false] {
                if !inputs {
                    l.locals[p] = lay.size;
                }
                for (i, v) in pou.vars.iter().enumerate() {
                    let first = is_function && v.section == Section::Input;
                    if first == inputs {
                        lay.offsets[i] = lay.size;
                        lay.size = lay.size.saturating_add(l.size(v.ty));
                    }
                }
            }
            l.pous[p] = lay;
        }
        l
    }

    /// The structures a value of `ty` holds directly (through arrays, not through structures).
    fn structs_in(&self, ty: Ty) -> Vec<usize> {
        match ty {
            Ty::Struct(i) => vec![i],
            Ty::Array(a) => self.structs_in(self.u.arrays[a].elem),
            _ => Vec::new(),
        }
    }

    fn size(&self, t: Ty) -> u64 {
        match t {
            Ty::Bool | Ty::Sint => 1,
            Ty::Int => 2,
            Ty::Dint | Ty::Real => 4,
            Ty::Lint | Ty::Lreal | Ty::Time => 8,
            Ty::String(n) => string_size(n) as u64,
            Ty::Fb(k) => vm_fb(k).size() as u64,
            Ty::Struct(i) => self.structs[i].size,
            Ty::Instance(p) => self.pous[p].size,
            Ty::Array(a) => {
                let a = &self.u.arrays[a];
                a.len().saturating_mul(self.size(a.elem))
            }
        }
    }

    /// Where member `m` of a value of type `of` starts within it.
    fn member(&self, of: Ty, m: usize) -> u64 {
        match of {
            Ty::Struct(i) => self.structs[i].offsets[m],
            Ty::Instance(p) => self.pous[p].offsets[m],
            Ty::Fb(k) => member_offset(k, m) as u64,
            _ => unreachable!("a member of what has none"),
        }
    }

    fn array_info(&self, ty: Ty) -> Option<ArrayInfo> {
        let Ty::Array(a) = ty else { return None };
        let a = &self.u.arrays[a];
        Some(ArrayInfo {
            element_type: self.u.ty_name(a.elem),
            element_size: self.size(a.elem) as u32,
            bounds: a.dims.clone(),
        })
    }
}

/// Where code reaches a value: an address, absolute or from the base, and, when an offset
/// computed at run time is added (it is on the stack), the end of the region it must stay in.
#[derive(Clone, Copy)]
struct Loc {
    rel: bool,
    addr: u32,
    indexed: Option<u32>,
}

impl Loc {
    fn absolute(addr: u32) -> Loc {
        Loc {
            rel: false,
            addr,
            indexed: None,
        }
    }
}

/// An argument of a function call worked out and not yet put in its place: the input it
/// sets, its type, and, for a structure, an array or a string, where it is (a string with its
/// length); a value is on the stack.
type Waiting = (usize, Ty, Option<(u16, Loc)>);

/// A function block instance being called. Its inputs, the call and its outputs each reach
/// it again, so an offset computed at run time is kept in a scratch variable (`saved`).
#[derive(Clone, Copy)]
struct Inst {
    rel: bool,
    addr: u32,
    /// (the end of the region, where the offset was saved)
    indexed: Option<(u32, u32)>,
}

struct Emitter<'u> {
    u: &'u Unit,
    lay: Layouts<'u>,
    code: Vec<u8>,
    /// The next free byte of memory: scratch variables are taken from here.
    memory: u64,
    /// For each POU of the unit: its index in the bytecode's table of bodies.
    body: Vec<u16>,
    /// For each function of the unit: where its parameters and locals are.
    frame: Vec<u64>,
    /// For each POU already emitted: the stack slots and the call frames its body needs.
    need_stack: Vec<i32>,
    need_calls: Vec<u32>,
    /// The POU being emitted.
    cur: usize,
    depth: i32,
    max_depth: i32,
    calls: u32,
    /// For each loop being emitted, the jumps its EXITs must be patched to.
    exits: Vec<Vec<usize>>,
    /// Every string literal and where its string is: put in the memory image, never written.
    literals: Vec<(Vec<u8>, u32)>,
}

fn too_big(what: impl std::fmt::Display) -> Diagnostic {
    Diagnostic::whole(format!("{what} is more than the bytecode format can hold"))
}

/// Where everything of a unit goes, before any code is emitted.
struct Plan<'u> {
    /// The POUs, each after those it uses, the program last.
    order: Vec<usize>,
    lay: Layouts<'u>,
    /// For each function of the unit: where its parameters and locals are.
    frame: Vec<u64>,
    /// Bytes of memory the variables take: the program's, then each function's.
    memory: u64,
}

/// `Err` when the unit was made for another IR version, is recursive (a front-end refuses
/// that with the place; this is the backstop for a unit that came from elsewhere), or is
/// larger than the format can describe.
fn plan(u: &Unit) -> Result<Plan<'_>, Diagnostic> {
    if u.ir_version != IR_VERSION {
        return Err(Diagnostic::whole(format!(
            "the unit is IR version {}; this backend reads version {IR_VERSION}",
            u.ir_version
        )));
    }
    if u.struct_cycle().is_some() {
        return Err(Diagnostic::whole("a structure contains itself"));
    }
    let mut order = u.order().map_err(|cycle| {
        let names: Vec<&str> = cycle.iter().map(|p| u.pous[*p].name.as_str()).collect();
        Diagnostic::whole(format!(
            "recursion is not allowed: {} reach themselves",
            names.join(", ")
        ))
    })?;
    // An array is one symbol, retained or not as a whole. A front-end refuses one that would
    // be retained in part with the place; this is the backstop.
    if let Some((path, _, _)) = u.arrays_retained_in_part().into_iter().next() {
        return Err(Diagnostic::whole(format!(
            "{path} would be retained in part; an array is retained or not as a whole"
        )));
    }
    // The program is last: a scan starts at the last body.
    order.retain(|p| *p != u.entry);
    order.push(u.entry);
    if order.len() > u16::MAX as usize {
        return Err(too_big(format!("{} POUs", order.len())));
    }

    let lay = Layouts::new(u, &order);
    // Memory: the program's variables from 0 (so their addresses do not move when a function
    // is added), then each function's parameters and locals, then scratch.
    let mut memory = lay.pous[u.entry].size;
    let mut frame = vec![0u64; u.pous.len()];
    for &p in &order {
        if matches!(u.pous[p].kind, PouKind::Function { .. }) {
            frame[p] = memory;
            memory = memory.saturating_add(lay.pous[p].size);
        }
    }
    let largest = lay
        .pous
        .iter()
        .chain(lay.structs.iter())
        .map(|l| l.size)
        .max()
        .unwrap_or(0)
        .max(memory);
    // Scratch is a few bytes per CASE and per call of an instance in an array; leaving room
    // for it here means no address computed below can pass 32 bits.
    if largest > (u32::MAX / 2) as u64 {
        return Err(too_big(format!("{largest} bytes of variables")));
    }
    Ok(Plan {
        order,
        lay,
        frame,
        memory,
    })
}

/// Bytes of memory the unit's variables need, without emitting anything: for refusing a unit
/// too large for its target before its memory image is built. The scratch `emit` adds (a few
/// bytes per CASE) is not in it; `Output::memory_size` is the whole.
pub fn variable_memory(u: &Unit) -> Result<u64, Diagnostic> {
    plan(u).map(|p| p.memory)
}

/// Lay `u` out and emit its bytecode. `Err` as [`variable_memory`].
pub fn emit(u: &Unit) -> Result<Output, Diagnostic> {
    let Plan {
        order,
        lay,
        frame,
        memory,
    } = plan(u)?;

    let mut body = vec![0u16; u.pous.len()];
    for (k, &p) in order.iter().enumerate() {
        body[p] = k as u16;
    }
    let mut e = Emitter {
        u,
        lay,
        code: Vec::new(),
        memory,
        body,
        frame,
        need_stack: vec![0; u.pous.len()],
        need_calls: vec![0; u.pous.len()],
        cur: u.entry,
        depth: 0,
        max_depth: 0,
        calls: 0,
        exits: Vec::new(),
        literals: Vec::new(),
    };
    let mut starts = Vec::new();
    for &p in &order {
        starts.push(e.code.len() as u64);
        e.pou(p);
    }
    let stack_depth = e.need_stack[u.entry].max(1);
    let call_depth = e.need_calls[u.entry];
    if e.memory > u32::MAX as u64 || e.code.len() as u64 > u32::MAX as u64 {
        return Err(too_big("the program"));
    }
    if stack_depth > u16::MAX as i32 || call_depth > u16::MAX as u32 {
        return Err(too_big("the depth of its expressions or calls"));
    }
    let memory_size = e.memory as u32;

    // Initial memory image: the program's variables, with every instance inside them, and
    // each function's variables, which `INIT` copies back at a call.
    let mut init = vec![0u8; memory_size as usize];
    for &p in &order {
        let at = match u.pous[p].kind {
            PouKind::Program => 0,
            PouKind::Function { .. } => e.frame[p],
            PouKind::FunctionBlock => continue,
        };
        for (i, v) in u.pous[p].vars.iter().enumerate() {
            e.lay.init(
                &mut init,
                at + e.lay.pous[p].offsets[i],
                v.ty,
                v.init.as_ref(),
            );
        }
    }

    let mut symbols = Vec::new();
    let program = u.program();
    for (i, v) in program.vars.iter().enumerate() {
        e.lay.symbols(
            &mut symbols,
            &v.name,
            v.ty,
            v.section,
            v.retention,
            e.lay.pous[u.entry].offsets[i],
            v.section == Section::Input,
        );
    }
    let types = e.lay.types();
    for (text, at) in &e.literals {
        let at = *at as usize;
        init[at..at + 2].copy_from_slice(&(text.len() as u16).to_le_bytes());
        init[at + 2..at + 2 + text.len()].copy_from_slice(text);
    }

    let mut bytes = Vec::with_capacity(20 + 8 * order.len() + init.len() + e.code.len() + 4);
    bytes.extend_from_slice(&MAGIC);
    bytes.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    bytes.extend_from_slice(&(stack_depth as u16).to_le_bytes());
    bytes.extend_from_slice(&memory_size.to_le_bytes());
    bytes.extend_from_slice(&(e.code.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&(call_depth as u16).to_le_bytes());
    bytes.extend_from_slice(&(order.len() as u16).to_le_bytes());
    for (k, &p) in order.iter().enumerate() {
        let instance = match u.pous[p].kind {
            PouKind::FunctionBlock => e.lay.pous[p].size as u32,
            PouKind::Program | PouKind::Function { .. } => 0,
        };
        bytes.extend_from_slice(&(starts[k] as u32).to_le_bytes());
        bytes.extend_from_slice(&instance.to_le_bytes());
    }
    bytes.extend_from_slice(&init);
    bytes.extend_from_slice(&e.code);
    let crc = crc32(&bytes);
    bytes.extend_from_slice(&crc.to_le_bytes());
    Ok(Output {
        memory_size,
        code_size: e.code.len() as u32,
        stack_depth: stack_depth as u16,
        call_depth: call_depth as u16,
        bytes,
        symbols,
        types,
    })
}

/// The length of a STRING type.
fn string_cap(ty: Ty) -> u16 {
    match ty {
        Ty::String(n) => n,
        other => unreachable!("{other:?} is not a STRING"),
    }
}

/// What `CMP_*` and `SCMP` add for each comparison.
fn cmp_code(c: Cmp) -> u8 {
    match c {
        Cmp::Eq => 0,
        Cmp::Ne => 1,
        Cmp::Lt => 2,
        Cmp::Le => 3,
        Cmp::Gt => 4,
        Cmp::Ge => 5,
    }
}

fn const_bytes(ty: Ty, k: Const) -> Vec<u8> {
    match (ty, k) {
        (Ty::Bool, Const::Bool(b)) => vec![b as u8],
        (Ty::Sint, Const::Int(x)) => (x as i8).to_le_bytes().to_vec(),
        (Ty::Int, Const::Int(x)) => (x as i16).to_le_bytes().to_vec(),
        (Ty::Dint, Const::Int(x)) => (x as i32).to_le_bytes().to_vec(),
        (Ty::Lint, Const::Int(x)) | (Ty::Time, Const::Time(x)) => x.to_le_bytes().to_vec(),
        (Ty::Real, Const::Real(x)) => (x as f32).to_le_bytes().to_vec(),
        (Ty::Real, Const::Int(x)) => (x as f32).to_le_bytes().to_vec(),
        (Ty::Lreal, Const::Real(x)) => x.to_le_bytes().to_vec(),
        (Ty::Lreal, Const::Int(x)) => (x as f64).to_le_bytes().to_vec(),
        (ty, k) => unreachable!("the front-end let {k:?} initialise a {ty:?}"),
    }
}

impl Layouts<'_> {
    /// Write what a value of `ty` at `at` starts with: `init` if the declaration gave one,
    /// otherwise what its type says (a structure's members and a function block's variables
    /// have their own), otherwise zero.
    fn init(&self, image: &mut [u8], at: u64, ty: Ty, init: Option<&Init>) {
        fn put(image: &mut [u8], at: u64, bytes: &[u8]) {
            let at = at as usize;
            image[at..at + bytes.len()].copy_from_slice(bytes);
        }
        match (ty, init) {
            (Ty::Struct(i), _) => {
                for (k, f) in self.u.structs[i].fields.iter().enumerate() {
                    self.init(
                        image,
                        at + self.structs[i].offsets[k],
                        f.ty,
                        f.init.as_ref(),
                    );
                }
            }
            (Ty::Instance(p), _) => {
                for (k, v) in self.u.pous[p].vars.iter().enumerate() {
                    self.init(image, at + self.pous[p].offsets[k], v.ty, v.init.as_ref());
                }
            }
            (Ty::Array(a), init) => {
                let array = &self.u.arrays[a];
                let size = self.size(array.elem);
                if let Some(Init::Texts(texts)) = init {
                    for (k, text) in texts.iter().enumerate() {
                        let text = Init::Str(text.clone());
                        self.init(image, at + k as u64 * size, array.elem, Some(&text));
                    }
                    return;
                }
                let given: &[Const] = match init {
                    Some(Init::Elements(values)) => values,
                    _ => &[],
                };
                // An element past the list starts at its type's default: zero for an
                // elementary type, which the image already is.
                let filled = if array.elem.is_elementary() {
                    (given.len() as u64).min(array.len())
                } else {
                    array.len()
                };
                for k in 0..filled {
                    match given.get(k as usize) {
                        Some(c) => put(image, at + k * size, &const_bytes(array.elem, *c)),
                        None => self.init(image, at + k * size, array.elem, None),
                    }
                }
            }
            (Ty::Fb(_), _) => {}
            (Ty::String(_), Some(Init::Str(text))) => {
                put(image, at, &(text.len() as u16).to_le_bytes());
                put(image, at + 2, text);
            }
            (ty, Some(Init::Value(c))) => put(image, at, &const_bytes(ty, *c)),
            (_, _) => {}
        }
    }

    /// The symbols of a variable of the program at `offset`: itself, or each of its parts.
    /// `retention` is the variable's own, or for a part, what the part takes from the rules
    /// of IEC 61131-3 ed. 3 §6.5.6.2 ([`Unit::member_retention`]).
    #[allow(clippy::too_many_arguments)]
    fn symbols(
        &self,
        out: &mut Vec<Symbol>,
        path: &str,
        ty: Ty,
        section: Section,
        retention: Retention,
        offset: u64,
        writable: bool,
    ) {
        match ty {
            // Every element of a structure, nested ones too, takes its qualifier.
            Ty::Struct(i) => {
                for (k, f) in self.u.structs[i].fields.iter().enumerate() {
                    let at = offset + self.structs[i].offsets[k];
                    let path = format!("{path}.{}", f.name);
                    self.symbols(out, &path, f.ty, section, retention, at, writable);
                }
            }
            // The members of an instance are the block's to set: the host sets none of them.
            Ty::Instance(p) => {
                for (k, v) in self.u.pous[p].vars.iter().enumerate() {
                    let at = offset + self.pous[p].offsets[k];
                    let path = format!("{path}.{}", v.name);
                    let retention = self.u.member_retention(retention, v);
                    self.symbols(out, &path, v.ty, section, retention, at, false);
                }
            }
            // A standard function block declares no retention of its own: its members take
            // the instance's.
            Ty::Fb(k) => {
                for (i, m) in k.members().iter().enumerate() {
                    let at = offset + member_offset(k, i) as u64;
                    let path = format!("{path}.{}", m.name);
                    self.symbols(out, &path, m.ty, section, retention, at, false);
                }
            }
            ty => out.push(Symbol {
                path: path.to_string(),
                ty: self.u.ty_name(ty),
                section,
                offset: offset as u32,
                size: self.size(ty) as u32,
                writable,
                array: self.array_info(ty),
                // An array is retained or not as a whole: `plan` refused one that would be
                // retained in part, so all of it is one or the other.
                retain: self
                    .u
                    .retained(ty, retention)
                    .expect("an array retained in part is refused before it is laid out"),
            }),
        }
    }

    /// The layout of every structure and function block type.
    fn types(&self) -> Vec<TypeLayout> {
        let member = |name: &str, ty: Ty, offset: u64| MemberLayout {
            name: name.to_string(),
            ty: self.u.ty_name(ty),
            offset: offset as u32,
            size: self.size(ty) as u32,
            array: self.array_info(ty),
        };
        let mut out = Vec::new();
        for (i, s) in self.u.structs.iter().enumerate() {
            out.push(TypeLayout {
                name: s.name.clone(),
                size: self.structs[i].size as u32,
                members: s
                    .fields
                    .iter()
                    .enumerate()
                    .map(|(k, f)| member(&f.name, f.ty, self.structs[i].offsets[k]))
                    .collect(),
            });
        }
        for (p, pou) in self.u.pous.iter().enumerate() {
            if pou.kind != PouKind::FunctionBlock {
                continue;
            }
            out.push(TypeLayout {
                name: pou.name.clone(),
                size: self.pous[p].size as u32,
                members: pou
                    .vars
                    .iter()
                    .enumerate()
                    .map(|(k, v)| member(&v.name, v.ty, self.pous[p].offsets[k]))
                    .collect(),
            });
        }
        let used = self.u.types_used();
        for k in StdFb::ALL {
            if used.contains(&Ty::Fb(k)) {
                out.push(TypeLayout {
                    name: k.name().to_string(),
                    size: vm_fb(k).size(),
                    members: k
                        .members()
                        .iter()
                        .enumerate()
                        .map(|(i, m)| member(m.name, m.ty, member_offset(k, i) as u64))
                        .collect(),
                });
            }
        }
        out
    }
}

impl Emitter<'_> {
    /// Emit the body of POU `p`, and record what running it needs.
    fn pou(&mut self, p: usize) {
        self.cur = p;
        self.depth = 0;
        self.max_depth = 0;
        self.calls = 0;
        let u = self.u;
        let pou = &u.pous[p];
        if let PouKind::Function { .. } = pou.kind {
            // IEC 61131-3 ed. 3 §6.6.2.1: a function does not store its result or its
            // internal variables from one call to the next. Its inputs are the caller's to
            // set; everything else starts afresh here.
            let start = self.lay.locals[p];
            let len = self.lay.pous[p].size - start;
            if len > 0 {
                self.init((self.frame[p] + start) as u32, len as u32);
            }
        }
        self.stmts(&pou.body);
        self.leave();
        self.need_stack[p] = self.max_depth;
        self.need_calls[p] = self.calls;
    }

    /// Leave the body being emitted: the scan ends in the program, a call returns elsewhere.
    fn leave(&mut self) {
        let code = match self.u.pous[self.cur].kind {
            PouKind::Program => op::END,
            PouKind::FunctionBlock | PouKind::Function { .. } => op::RET,
        };
        self.op(code, 0);
    }

    fn op(&mut self, code: u8, delta: i32) {
        self.code.push(code);
        self.depth += delta;
        self.max_depth = self.max_depth.max(self.depth);
    }
    fn op_u32(&mut self, code: u8, x: u32, delta: i32) {
        self.op(code, delta);
        self.code.extend_from_slice(&x.to_le_bytes());
    }
    fn here(&self) -> usize {
        self.code.len()
    }
    /// Emit a jump to be patched; returns where its operand is.
    fn jump(&mut self, code: u8) -> usize {
        self.op_u32(code, 0, if code == op::JZ { -1 } else { 0 });
        self.code.len() - 4
    }
    fn patch(&mut self, at: usize, target: usize) {
        self.code[at..at + 4].copy_from_slice(&(target as u32).to_le_bytes());
    }

    /// Bytes of scratch memory, for this place in the code alone. Fixed, like everything
    /// else: no body runs twice at once.
    fn scratch(&mut self, size: u64) -> Loc {
        let at = self.memory;
        self.memory += size;
        Loc::absolute(at as u32)
    }

    fn size(&self, ty: Ty) -> u32 {
        self.lay.size(ty) as u32
    }

    /// Where variable `var` of the POU being emitted is.
    fn var(&self, var: usize) -> Loc {
        let offset = self.lay.pous[self.cur].offsets[var];
        match self.u.pous[self.cur].kind {
            PouKind::Program => Loc::absolute(offset as u32),
            PouKind::Function { .. } => Loc::absolute((self.frame[self.cur] + offset) as u32),
            PouKind::FunctionBlock => Loc {
                rel: true,
                addr: offset as u32,
                indexed: None,
            },
        }
    }

    /// Emit what reaching `p` needs and say where it is. A subscript that is not a constant
    /// is checked by `IDX` at run time and leaves an offset on the stack; `indexed` then
    /// holds the end of the array it was first applied to, the region the offset stays in.
    fn place(&mut self, p: &Place) -> Loc {
        let u = self.u;
        let mut loc = self.var(p.var);
        let mut ty = u.pous[self.cur].vars[p.var].ty;
        for sel in &p.path {
            match sel {
                Sel::Member(m) => {
                    loc.addr += self.lay.member(ty, *m) as u32;
                    ty = u.member_ty(ty, *m).expect("the front-end resolved it");
                }
                Sel::Index(subscripts) => {
                    let Ty::Array(a) = ty else {
                        unreachable!("a subscript on what is not an array")
                    };
                    let array = &u.arrays[a];
                    let whole = self.size(ty);
                    let mut stride = whole as u64;
                    for (e, (lo, hi)) in subscripts.iter().zip(&array.dims) {
                        let count = (hi - lo + 1) as u64;
                        stride /= count;
                        // A constant subscript inside its range needs no code: it is part
                        // of the address. Anything else is checked when it is known.
                        if let ExprKind::Const(Const::Int(v)) = e.kind {
                            if v >= *lo && v <= *hi {
                                loc.addr += ((v - lo) as u64 * stride) as u32;
                                continue;
                            }
                        }
                        self.expr(e);
                        let code = match class(e.ty) {
                            Class::I32 => op::IDX,
                            Class::I64 => op::IDX64,
                            _ => unreachable!("a subscript is an integer"),
                        };
                        self.op(code, 0);
                        self.code.extend_from_slice(&(*lo as i32).to_le_bytes());
                        self.code.extend_from_slice(&(count as u32).to_le_bytes());
                        self.code.extend_from_slice(&(stride as u32).to_le_bytes());
                        match loc.indexed {
                            // A second offset is added to the first.
                            Some(_) => self.op(op::ADD_I32, -1),
                            None => loc.indexed = Some(loc.addr + whole),
                        }
                    }
                    ty = array.elem;
                }
            }
        }
        loc
    }

    /// A place operand: `mode addr span`, for `len` bytes at `loc`.
    fn place_operand(&mut self, loc: Loc, len: u32) {
        let mut mode = 0;
        if loc.rel {
            mode |= PLACE_RELATIVE;
        }
        let span = match loc.indexed {
            Some(end) => {
                mode |= PLACE_INDEXED;
                end - loc.addr
            }
            None => len,
        };
        self.code.push(mode);
        self.code.extend_from_slice(&loc.addr.to_le_bytes());
        self.code.extend_from_slice(&span.to_le_bytes());
    }

    fn load(&mut self, ty: Ty, loc: Loc) {
        // (absolute, relative, through a place)
        let (abs, rel, place) = match ty {
            Ty::Bool => (op::LD_BOOL, op::LDR_BOOL, op::LDP_BOOL),
            Ty::Sint => (op::LD_I8, op::LDR_I8, op::LDP_I8),
            Ty::Int => (op::LD_I16, op::LDR_I16, op::LDP_I16),
            Ty::Dint => (op::LD_I32, op::LDR_32, op::LDP_32),
            Ty::Real => (op::LD_F32, op::LDR_32, op::LDP_32),
            Ty::Lint | Ty::Time => (op::LD_I64, op::LDR_64, op::LDP_64),
            Ty::Lreal => (op::LD_F64, op::LDR_64, op::LDP_64),
            _ => unreachable!("only an elementary value is loaded"),
        };
        match (loc.indexed, loc.rel) {
            (Some(_), _) => {
                // The offset is popped and the value pushed.
                self.op(place, 0);
                self.place_operand(loc, self.size(ty));
            }
            (None, true) => self.op_u32(rel, loc.addr, 1),
            (None, false) => self.op_u32(abs, loc.addr, 1),
        }
    }
    fn store(&mut self, ty: Ty, loc: Loc) {
        let (abs, rel, place) = match ty {
            Ty::Bool => (op::ST_BOOL, op::STR_BOOL, op::STP_BOOL),
            Ty::Sint => (op::ST_8, op::STR_8, op::STP_8),
            Ty::Int => (op::ST_16, op::STR_16, op::STP_16),
            Ty::Dint | Ty::Real => (op::ST_32, op::STR_32, op::STP_32),
            Ty::Lint | Ty::Lreal | Ty::Time => (op::ST_64, op::STR_64, op::STP_64),
            _ => unreachable!("only an elementary value is stored"),
        };
        match (loc.indexed, loc.rel) {
            (Some(_), _) => {
                // The value is popped, then the offset under it.
                self.op(place, -2);
                self.place_operand(loc, self.size(ty));
            }
            (None, true) => self.op_u32(rel, loc.addr, -1),
            (None, false) => self.op_u32(abs, loc.addr, -1),
        }
    }
    /// Copy `len` bytes from `src` to `dst`. When both take an offset, `dst`'s was pushed
    /// first.
    fn copy(&mut self, len: u32, dst: Loc, src: Loc) {
        let popped = dst.indexed.is_some() as i32 + src.indexed.is_some() as i32;
        self.op_u32(op::MOVE, len, -popped);
        self.place_operand(dst, len);
        self.place_operand(src, len);
    }
    fn init(&mut self, addr: u32, len: u32) {
        self.op_u32(op::INIT, addr, 0);
        self.code.extend_from_slice(&len.to_le_bytes());
    }

    // ---- Strings ----------------------------------------------------------------------
    //
    // A string never goes on the stack: an instruction names each string it reads or writes
    // (`cap place`). What an expression of a STRING type gives is a string somewhere in
    // memory: the variable it loads, a literal's string in the memory image, or scratch the
    // value was written to. A string read from an array element is named where it is, its
    // offset on the stack: an instruction pops its integers, then the offsets of its operands
    // from the last to the first, which is the reverse of the order they are emitted in
    // here. What a function gives is always written to scratch, never to an element.

    /// A string operand: `cap` and the place of `2 + cap` bytes at `loc`.
    fn string_operand(&mut self, cap: u16, loc: Loc) {
        self.code.extend_from_slice(&cap.to_le_bytes());
        self.place_operand(loc, string_size(cap));
    }

    /// The string of a literal: one place for each distinct text, never written.
    fn literal(&mut self, text: &[u8]) -> Loc {
        if let Some((_, at)) = self.literals.iter().find(|(t, _)| t == text) {
            return Loc::absolute(*at);
        }
        let loc = self.scratch(string_size(text.len() as u16) as u64);
        self.literals.push((text.to_vec(), loc.addr));
        loc
    }

    /// `dst := src`, cut to `dst`'s length. When both take an offset, `dst`'s was pushed
    /// first.
    fn string_move(&mut self, dst: (u16, Loc), src: (u16, Loc)) {
        let popped = dst.1.indexed.is_some() as i32 + src.1.indexed.is_some() as i32;
        self.op(op::SMOVE, -popped);
        self.string_operand(dst.0, dst.1);
        self.string_operand(src.0, src.1);
    }

    /// Where the string `e` gives is, and its length. When it is an element of an array, its
    /// offset is now on the stack.
    fn string_value(&mut self, e: &Expr) -> (u16, Loc) {
        match &e.kind {
            ExprKind::Str(text) => (text.len() as u16, self.literal(text)),
            ExprKind::Load(p) => {
                let cap = string_cap(self.u.place_ty(self.cur, p));
                (cap, self.place(p))
            }
            _ => {
                let cap = string_cap(e.ty);
                let tmp = self.scratch(string_size(cap) as u64);
                self.string_into(e, (cap, tmp));
                (cap, tmp)
            }
        }
    }

    /// How many of `strings` have an offset on the stack, which the instruction pops.
    fn offsets(strings: &[(u16, Loc)]) -> i32 {
        strings.iter().filter(|(_, l)| l.indexed.is_some()).count() as i32
    }

    /// An integer argument, as the i64 the instructions on strings pop.
    fn int64(&mut self, e: &Expr) {
        self.expr(e);
        if class(e.ty) == Class::I32 {
            self.op(op::I32_TO_I64, 0);
        }
    }

    /// Write the string `e` gives to `dst`, a string of `e`'s own type at a place without an
    /// offset that none of `e`'s operands is.
    fn string_into(&mut self, e: &Expr, dst: (u16, Loc)) {
        match &e.kind {
            ExprKind::Std(f, args) => self.string_function(*f, args, dst),
            ExprKind::Convert(a) => {
                self.int64(a);
                self.op(op::ITOS, -1);
                self.string_operand(dst.0, dst.1);
            }
            ExprKind::Call { pou, args } => {
                let result = self.call_body(*pou, args);
                self.string_move(dst, result);
            }
            ExprKind::Str(_) | ExprKind::Load(_) => {
                let src = self.string_value(e);
                self.string_move(dst, src);
            }
            _ => unreachable!("{:?} does not give a STRING", e.kind),
        }
    }

    /// A function of Table 34 that gives a string, written to `dst`.
    fn string_function(&mut self, f: StdFn, args: &[Expr], dst: (u16, Loc)) {
        let mut strings = Vec::new();
        let (code, ints): (u8, &[Expr]) = match f {
            StdFn::Concat => {
                // Two at a time, into `dst`: the first two, then `dst` and each next one.
                let a = self.string_value(&args[0]);
                let b = self.string_value(&args[1]);
                self.op(op::SCAT, -Self::offsets(&[a, b]));
                self.string_operand(dst.0, dst.1);
                self.string_operand(a.0, a.1);
                self.string_operand(b.0, b.1);
                for next in &args[2..] {
                    let b = self.string_value(next);
                    self.op(op::SCAT, -Self::offsets(&[b]));
                    self.string_operand(dst.0, dst.1);
                    self.string_operand(dst.0, dst.1);
                    self.string_operand(b.0, b.1);
                }
                return;
            }
            StdFn::Left => (op::SLEFT, &args[1..2]),
            StdFn::Right => (op::SRIGHT, &args[1..2]),
            StdFn::Mid => (op::SMID, &args[1..3]),
            StdFn::Delete => (op::SDELETE, &args[1..3]),
            StdFn::Insert => {
                strings.push(1);
                (op::SINSERT, &args[2..3])
            }
            StdFn::Replace => {
                strings.push(1);
                (op::SREPLACE, &args[2..4])
            }
            _ => unreachable!("{} does not give a STRING", f.name()),
        };
        let mut operands = vec![self.string_value(&args[0])];
        for k in strings {
            operands.push(self.string_value(&args[k]));
        }
        for i in ints {
            self.int64(i);
        }
        self.op(code, -(ints.len() as i32) - Self::offsets(&operands));
        self.string_operand(dst.0, dst.1);
        for (cap, loc) in operands {
            self.string_operand(cap, loc);
        }
    }

    /// A function of the standard library that gives a value of type `ty` on the stack.
    fn std_value(&mut self, f: StdFn, args: &[Expr], ty: Ty) {
        match f {
            StdFn::Len => {
                let a = self.string_value(&args[0]);
                self.op(op::SLEN, 1 - Self::offsets(&[a]));
                self.string_operand(a.0, a.1);
            }
            StdFn::Find => {
                let a = self.string_value(&args[0]);
                let b = self.string_value(&args[1]);
                self.op(op::SFIND, 1 - Self::offsets(&[a, b]));
                self.string_operand(a.0, a.1);
                self.string_operand(b.0, b.1);
            }
            f if f.is_string_function() => unreachable!("{} gives a STRING", f.name()),
            _ => self.numeric(f, args, ty),
        }
    }

    /// A numeric or selection function (Table 28, EXPT, TRUNC, Table 32) giving a `ty`.
    fn numeric(&mut self, f: StdFn, args: &[Expr], ty: Ty) {
        let real = |t: Ty| if t == Ty::Real { op::RFN32 } else { op::RFN };
        let which = match f {
            StdFn::Sqrt => Some(rfn::SQRT),
            StdFn::Ln => Some(rfn::LN),
            StdFn::Log => Some(rfn::LOG),
            StdFn::Exp => Some(rfn::EXP),
            StdFn::Sin => Some(rfn::SIN),
            StdFn::Cos => Some(rfn::COS),
            StdFn::Tan => Some(rfn::TAN),
            StdFn::Asin => Some(rfn::ASIN),
            StdFn::Acos => Some(rfn::ACOS),
            StdFn::Atan => Some(rfn::ATAN),
            _ => None,
        };
        if let Some(which) = which {
            self.expr(&args[0]);
            self.op(real(ty), 0);
            self.code.push(which);
            return;
        }
        match f {
            StdFn::Abs => {
                self.expr(&args[0]);
                let code = match class(ty) {
                    Class::I32 => op::ABS_I32,
                    Class::I64 => op::ABS_I64,
                    Class::F32 => op::ABS_F32,
                    Class::F64 => op::ABS_F64,
                    Class::Bool => unreachable!("ABS of a BOOL"),
                };
                self.op(code, 0);
                // ABS of SINT's or INT's most negative value is outside its type: it wraps,
                // as every result outside its type does here.
                self.wrap(ty);
            }
            StdFn::Trunc => {
                let from = args[0].ty;
                self.expr(&args[0]);
                self.op(real(from), 0);
                self.code.push(rfn::TRUNC);
                self.convert(from, ty);
            }
            StdFn::Atan2 => {
                self.expr(&args[0]);
                self.expr(&args[1]);
                let code = if ty == Ty::Real {
                    op::ATAN2_F32
                } else {
                    op::ATAN2_F64
                };
                self.op(code, -1);
            }
            StdFn::Expt => {
                let (base, exponent) = (&args[0], &args[1]);
                if exponent.ty.is_int() {
                    self.expr(base);
                    self.expr(exponent);
                    let code = if ty == Ty::Real {
                        op::EXPTI_F32
                    } else {
                        op::EXPTI_F64
                    };
                    self.op(code, -1);
                } else if exponent.ty == ty {
                    self.expr(base);
                    self.expr(exponent);
                    let code = if ty == Ty::Real {
                        op::EXPT_F32
                    } else {
                        op::EXPT_F64
                    };
                    self.op(code, -1);
                } else {
                    // A REAL to an LREAL power: in LREAL, the result a REAL again.
                    self.expr(base);
                    self.convert(ty, Ty::Lreal);
                    self.expr(exponent);
                    self.convert(exponent.ty, Ty::Lreal);
                    self.op(op::EXPT_F64, -1);
                    self.convert(Ty::Lreal, ty);
                }
            }
            StdFn::Sel => {
                // IN0 when G is FALSE, IN1 when it is TRUE; only the one chosen is worked out.
                self.expr(&args[0]);
                let to_in0 = self.jump(op::JZ);
                let depth = self.depth;
                self.expr(&args[2]);
                let to_end = self.jump(op::JMP);
                let h = self.here();
                self.patch(to_in0, h);
                self.depth = depth;
                self.expr(&args[1]);
                let h = self.here();
                self.patch(to_end, h);
            }
            StdFn::Max | StdFn::Min => {
                let more = if f == StdFn::Max { Cmp::Gt } else { Cmp::Lt };
                let acc = self.scratch(self.lay.size(ty));
                self.expr(&args[0]);
                self.store(ty, acc);
                for a in &args[1..] {
                    self.keep_if(ty, acc, a, more);
                }
                self.load(ty, acc);
            }
            StdFn::Limit => {
                // MIN(MAX(IN, MN), MX).
                let acc = self.scratch(self.lay.size(ty));
                self.expr(&args[1]);
                self.store(ty, acc);
                self.keep_if(ty, acc, &args[0], Cmp::Gt);
                self.keep_if(ty, acc, &args[2], Cmp::Lt);
                self.load(ty, acc);
            }
            StdFn::Mux => {
                // K is checked against the inputs (Table 32 footnote e), then compared with
                // each; only the input chosen is worked out.
                let k = self.scratch(8);
                self.expr(&args[0]);
                self.store(Ty::Lint, k);
                self.load(Ty::Lint, k);
                let inputs = &args[1..];
                self.op_u32(op::CHKSEL, inputs.len() as u32, -1);
                let depth = self.depth;
                let mut ends = Vec::new();
                for (i, input) in inputs.iter().enumerate() {
                    self.depth = depth;
                    if i + 1 == inputs.len() {
                        self.expr(input);
                        break;
                    }
                    self.load(Ty::Lint, k);
                    self.push_const(Ty::Lint, Const::Int(i as i64));
                    self.op(op::CMP_I64 + cmp_code(Cmp::Eq), -1);
                    let next = self.jump(op::JZ);
                    self.expr(input);
                    ends.push(self.jump(op::JMP));
                    let h = self.here();
                    self.patch(next, h);
                }
                let h = self.here();
                for j in ends {
                    self.patch(j, h);
                }
            }
            _ => unreachable!("{} is not numeric", f.name()),
        }
    }

    /// `acc := a` when `a` compares to `acc` as `more` says (MAX, MIN, LIMIT).
    fn keep_if(&mut self, ty: Ty, acc: Loc, a: &Expr, more: Cmp) {
        let tmp = self.scratch(self.lay.size(ty));
        self.expr(a);
        self.store(ty, tmp);
        self.load(ty, tmp);
        self.load(ty, acc);
        let base = match class(ty) {
            Class::Bool | Class::I32 => op::CMP_I32,
            Class::I64 => op::CMP_I64,
            Class::F32 => op::CMP_F32,
            Class::F64 => op::CMP_F64,
        };
        self.op(base + cmp_code(more), -1);
        let skip = self.jump(op::JZ);
        self.load(ty, tmp);
        self.store(ty, acc);
        let h = self.here();
        self.patch(skip, h);
    }

    /// Note that the body being emitted calls POU `q`, with the stack as deep as it is now.
    fn calling(&mut self, q: usize) {
        self.calls = self.calls.max(1 + self.need_calls[q]);
        self.max_depth = self.max_depth.max(self.depth + self.need_stack[q]);
    }

    /// The instance at `loc`, ready to be reached several times.
    fn instance(&mut self, loc: Loc) -> Inst {
        let indexed = loc.indexed.map(|end| {
            let saved = self.scratch(4);
            self.store(Ty::Dint, saved);
            (end, saved.addr)
        });
        Inst {
            rel: loc.rel,
            addr: loc.addr,
            indexed,
        }
    }
    /// What is `offset` bytes into `inst`; its saved offset, if it has one, is pushed.
    fn within(&mut self, inst: Inst, offset: u32) -> Loc {
        let indexed = inst.indexed.map(|(end, saved)| {
            self.load(Ty::Dint, Loc::absolute(saved));
            end
        });
        Loc {
            rel: inst.rel,
            addr: inst.addr + offset,
            indexed,
        }
    }

    fn push_const(&mut self, ty: Ty, k: Const) {
        match (class(ty), k) {
            (Class::Bool, Const::Bool(b)) => self.op_u32(op::PUSH32, b as u32, 1),
            (Class::I32, Const::Int(v)) => self.op_u32(op::PUSH32, v as i32 as u32, 1),
            (Class::I64, Const::Int(v) | Const::Time(v)) => {
                self.op(op::PUSH64, 1);
                self.code.extend_from_slice(&(v as u64).to_le_bytes());
            }
            (Class::F32, Const::Real(v)) => self.op_u32(op::PUSH32, (v as f32).to_bits(), 1),
            (Class::F32, Const::Int(v)) => self.op_u32(op::PUSH32, (v as f32).to_bits(), 1),
            (Class::F64, Const::Real(v)) => {
                self.op(op::PUSH64, 1);
                self.code.extend_from_slice(&v.to_bits().to_le_bytes());
            }
            (Class::F64, Const::Int(v)) => {
                self.op(op::PUSH64, 1);
                self.code
                    .extend_from_slice(&(v as f64).to_bits().to_le_bytes());
            }
            (_, k) => unreachable!("constant {k:?} typed {ty:?}"),
        }
    }
    /// Bring an i32 result back into the range of SINT or INT.
    fn wrap(&mut self, ty: Ty) {
        match ty {
            Ty::Int => self.op(op::WRAP_I16, 0),
            Ty::Sint => self.op(op::WRAP_I8, 0),
            _ => {}
        }
    }

    fn expr(&mut self, e: &Expr) {
        match &e.kind {
            ExprKind::Const(k) => self.push_const(e.ty, *k),
            ExprKind::Str(_) => unreachable!("a string is not a value on the stack"),
            ExprKind::Std(f, args) => self.std_value(*f, args, e.ty),
            ExprKind::Load(p) => {
                let a = self.place(p);
                self.load(e.ty, a);
            }
            ExprKind::Call { pou, args } => self.call(*pou, args),
            ExprKind::Not(a) => {
                self.expr(a);
                self.op(op::NOT, 0);
            }
            ExprKind::Neg(a) => {
                self.expr(a);
                let code = match class(e.ty) {
                    Class::I32 => op::NEG_I32,
                    Class::I64 => op::NEG_I64,
                    Class::F32 => op::NEG_F32,
                    Class::F64 => op::NEG_F64,
                    Class::Bool => unreachable!(),
                };
                self.op(code, 0);
                self.wrap(e.ty);
            }
            ExprKind::Logic(l, a, b) => {
                self.expr(a);
                self.expr(b);
                let code = match l {
                    Logic::And => op::AND,
                    Logic::Or => op::OR,
                    Logic::Xor => op::XOR,
                };
                self.op(code, -1);
            }
            ExprKind::Cmp(c, a, b) if a.ty.is_string() => {
                let a = self.string_value(a);
                let b = self.string_value(b);
                self.op(op::SCMP, 1 - Self::offsets(&[a, b]));
                self.code.push(cmp_code(*c));
                self.string_operand(a.0, a.1);
                self.string_operand(b.0, b.1);
            }
            ExprKind::Cmp(c, a, b) => {
                self.expr(a);
                self.expr(b);
                let base = match class(a.ty) {
                    Class::Bool | Class::I32 => op::CMP_I32,
                    Class::I64 => op::CMP_I64,
                    Class::F32 => op::CMP_F32,
                    Class::F64 => op::CMP_F64,
                };
                self.op(base + cmp_code(*c), -1);
            }
            ExprKind::Arith(a_op, a, b) => {
                self.expr(a);
                self.expr(b);
                let base = match class(e.ty) {
                    Class::I32 => op::ADD_I32,
                    Class::I64 => op::ADD_I64,
                    Class::F32 => op::ADD_F32,
                    Class::F64 => op::ADD_F64,
                    Class::Bool => unreachable!(),
                };
                let which = match a_op {
                    Arith::Add => 0,
                    Arith::Sub => 1,
                    Arith::Mul => 2,
                    Arith::Div => 3,
                    Arith::Mod => 4,
                };
                self.op(base + which, -1);
                self.wrap(e.ty);
            }
            ExprKind::Convert(a) if a.ty.is_string() => {
                let a = self.string_value(a);
                self.op(op::STOI, 1 - Self::offsets(&[a]));
                self.string_operand(a.0, a.1);
                self.convert(Ty::Lint, e.ty);
            }
            ExprKind::Convert(a) => {
                self.expr(a);
                self.convert(a.ty, e.ty);
            }
        }
    }

    /// A call of function `f` in an expression whose value goes on the stack.
    fn call(&mut self, f: usize, args: &[(usize, Expr)]) {
        let (_, result) = self.call_body(f, args);
        let PouKind::Function { result: r } = self.u.pous[f].kind else {
            unreachable!("only a function is called in an expression")
        };
        self.load(self.u.pous[f].vars[r].ty, result);
    }

    /// Emit a call of function `f`, and say where its result is (and, for a STRING, how long
    /// it is). Every argument is worked out before any is put in its place: an argument may
    /// itself call `f`, whose parameters are the same bytes.
    fn call_body(&mut self, f: usize, args: &[(usize, Expr)]) -> (u16, Loc) {
        let u = self.u;
        let frame = self.frame[f];
        let at =
            |e: &Emitter, var: usize| Loc::absolute((frame + e.lay.pous[f].offsets[var]) as u32);
        // A value goes on the stack; a structure, an array or a string stays where it is, and
        // only an offset computed for it goes on the stack.
        let mut waiting: Vec<Waiting> = Vec::new();
        for (var, a) in args {
            if a.ty.is_aggregate() {
                let ExprKind::Load(src) = &a.kind else {
                    unreachable!("a structure or an array is always loaded from a place")
                };
                let src = self.place(src);
                waiting.push((*var, a.ty, Some((0, src))));
            } else if a.ty.is_string() {
                let src = self.string_value(a);
                waiting.push((*var, a.ty, Some(src)));
            } else {
                self.expr(a);
                waiting.push((*var, a.ty, None));
            }
        }
        for (var, ty, src) in waiting.into_iter().rev() {
            let dst = at(self, var);
            match src {
                Some(src) if ty.is_string() => {
                    let cap = string_cap(u.pous[f].vars[var].ty);
                    self.string_move((cap, dst), src);
                }
                Some((_, src)) => self.copy(self.size(ty), dst, src),
                None => self.store(ty, dst),
            }
        }
        // IEC 61131-3 ed. 3 §6.6.1.4.2: an input given no value has its initial value.
        for (var, v) in u.pous[f].vars.iter().enumerate() {
            let given = args.iter().any(|(a, _)| *a == var);
            if v.section == Section::Input && !given && self.size(v.ty) > 0 {
                self.init(at(self, var).addr, self.size(v.ty));
            }
        }
        self.calling(f);
        self.op(op::CALL, 0);
        self.code.extend_from_slice(&self.body[f].to_le_bytes());
        let PouKind::Function { result } = u.pous[f].kind else {
            unreachable!("only a function is called in an expression")
        };
        let cap = match u.pous[f].vars[result].ty {
            Ty::String(n) => n,
            _ => 0,
        };
        (cap, at(self, result))
    }

    fn convert(&mut self, from: Ty, to: Ty) {
        use Class::*;
        let codes: &[u8] = match (class(from), class(to)) {
            (x, y) if x == y => &[],
            (Bool, I32) | (Bool, I64) => &[],
            (Bool, F32) => &[op::I32_TO_F32],
            (Bool, F64) => &[op::I32_TO_F64],
            (I32, Bool) | (I64, Bool) => &[op::I64_TO_BOOL],
            (I32, I64) => &[op::I32_TO_I64],
            (I64, I32) => &[op::I64_TO_I32],
            (I32, F32) => &[op::I32_TO_F32],
            (I32, F64) => &[op::I32_TO_F64],
            (I64, F32) => &[op::I64_TO_F32],
            (I64, F64) => &[op::I64_TO_F64],
            (F32, F64) => &[op::F32_TO_F64],
            (F64, F32) => &[op::F64_TO_F32],
            (F32, I32) => &[op::F32_TO_I32],
            (F64, I32) => &[op::F64_TO_I32],
            (F32, I64) => &[op::F32_TO_I64],
            (F64, I64) => &[op::F64_TO_I64],
            (F32 | F64, Bool) => unreachable!("refused by the checker"),
            _ => unreachable!(),
        };
        for c in codes {
            self.op(*c, 0);
        }
        // Narrowing within the i32 class (DINT -> INT), and any conversion arriving in it.
        if class(to) == I32 && from != to {
            self.wrap(to);
        }
    }

    fn stmts(&mut self, s: &[Stmt]) {
        for x in s {
            self.stmt(x);
        }
    }

    fn stmt(&mut self, s: &Stmt) {
        match s {
            Stmt::Assign(p, v) => {
                let ty = self.u.place_ty(self.cur, p);
                let dst = self.place(p);
                if let Ty::String(cap) = ty {
                    let src = self.string_value(v);
                    self.string_move((cap, dst), src);
                } else if v.ty.is_aggregate() {
                    let ExprKind::Load(src) = &v.kind else {
                        unreachable!("a structure or an array is always loaded from a place")
                    };
                    let src = self.place(src);
                    self.copy(self.size(v.ty), dst, src);
                } else {
                    self.expr(v);
                    self.store(v.ty, dst);
                }
            }
            Stmt::Call {
                instance,
                inputs,
                outputs,
            } => {
                let u = self.u;
                let block = u.place_ty(self.cur, instance);
                let member = |e: &Emitter, m: usize| {
                    (
                        u.member_ty(block, m).expect("the front-end resolved it"),
                        e.lay.member(block, m) as u32,
                    )
                };
                let loc = self.place(instance);
                let inst = self.instance(loc);
                for (m, v) in inputs {
                    let (mty, offset) = member(self, *m);
                    let dst = self.within(inst, offset);
                    if let Ty::String(cap) = mty {
                        let src = self.string_value(v);
                        self.string_move((cap, dst), src);
                    } else if mty.is_aggregate() {
                        let ExprKind::Load(src) = &v.kind else {
                            unreachable!("a structure or an array is always loaded from a place")
                        };
                        let src = self.place(src);
                        self.copy(self.size(mty), dst, src);
                    } else {
                        self.expr(v);
                        self.store(mty, dst);
                    }
                }
                let at = self.within(inst, 0);
                let popped = -(at.indexed.is_some() as i32);
                match block {
                    Ty::Fb(k) if !at.rel && at.indexed.is_none() => {
                        self.op(op::FB, 0);
                        self.code.push(vm_fb(k).code());
                        self.code.extend_from_slice(&at.addr.to_le_bytes());
                    }
                    Ty::Fb(k) => {
                        self.op(op::FBP, popped);
                        self.code.push(vm_fb(k).code());
                        self.place_operand(at, vm_fb(k).size());
                    }
                    Ty::Instance(q) => {
                        self.op(op::CALLFB, popped);
                        self.code.extend_from_slice(&self.body[q].to_le_bytes());
                        self.place_operand(at, self.size(block));
                        self.calling(q);
                    }
                    _ => unreachable!("only a function block instance is called"),
                }
                for (m, target) in outputs {
                    let (mty, offset) = member(self, *m);
                    let tty = u.place_ty(self.cur, target);
                    let dst = self.place(target);
                    let src = self.within(inst, offset);
                    if let (Ty::String(to), Ty::String(from)) = (tty, mty) {
                        self.string_move((to, dst), (from, src));
                    } else if mty.is_aggregate() {
                        self.copy(self.size(mty), dst, src);
                    } else {
                        self.load(mty, src);
                        self.convert(mty, tty);
                        self.store(tty, dst);
                    }
                }
            }
            Stmt::If(branches, otherwise) => {
                let mut ends = Vec::new();
                for (c, body) in branches {
                    self.expr(c);
                    let next = self.jump(op::JZ);
                    self.stmts(body);
                    ends.push(self.jump(op::JMP));
                    let h = self.here();
                    self.patch(next, h);
                }
                self.stmts(otherwise);
                let h = self.here();
                for j in ends {
                    self.patch(j, h);
                }
            }
            Stmt::Case(sel, arms, otherwise) => {
                let tmp = self.scratch(self.size(sel.ty) as u64);
                self.expr(sel);
                self.store(sel.ty, tmp);
                let mut ends = Vec::new();
                for (ranges, body) in arms {
                    if ranges.is_empty() {
                        continue;
                    }
                    for (i, (lo, hi)) in ranges.iter().enumerate() {
                        let base = if class(sel.ty) == Class::I64 {
                            op::CMP_I64
                        } else {
                            op::CMP_I32
                        };
                        if lo == hi {
                            self.load(sel.ty, tmp);
                            self.push_const(sel.ty, Const::Int(*lo));
                            self.op(base, -1);
                        } else {
                            self.load(sel.ty, tmp);
                            self.push_const(sel.ty, Const::Int(*lo));
                            self.op(base + 5, -1);
                            self.load(sel.ty, tmp);
                            self.push_const(sel.ty, Const::Int(*hi));
                            self.op(base + 3, -1);
                            self.op(op::AND, -1);
                        }
                        if i > 0 {
                            self.op(op::OR, -1);
                        }
                    }
                    let next = self.jump(op::JZ);
                    self.stmts(body);
                    ends.push(self.jump(op::JMP));
                    let h = self.here();
                    self.patch(next, h);
                }
                self.stmts(otherwise);
                let h = self.here();
                for j in ends {
                    self.patch(j, h);
                }
            }
            Stmt::For {
                var,
                from,
                to,
                by,
                body,
            } => {
                let ty = self.u.place_ty(self.cur, var);
                let a = self.place(var);
                assert!(
                    a.indexed.is_none(),
                    "a FOR variable is not an array element"
                );
                self.expr(from);
                self.store(ty, a);
                let top = self.here();
                self.load(ty, a);
                self.expr(to);
                let base = if class(ty) == Class::I64 {
                    op::CMP_I64
                } else {
                    op::CMP_I32
                };
                self.op(base + if *by > 0 { 3 } else { 5 }, -1);
                let end = self.jump(op::JZ);
                self.exits.push(Vec::new());
                self.stmts(body);
                self.load(ty, a);
                self.push_const(ty, Const::Int(*by));
                self.op(
                    if class(ty) == Class::I64 {
                        op::ADD_I64
                    } else {
                        op::ADD_I32
                    },
                    -1,
                );
                self.wrap(ty);
                self.store(ty, a);
                self.op_u32(op::JMP, top as u32, 0);
                let h = self.here();
                self.patch(end, h);
                for j in self.exits.pop().unwrap_or_default() {
                    self.patch(j, h);
                }
            }
            Stmt::While(c, body) => {
                let top = self.here();
                self.expr(c);
                let end = self.jump(op::JZ);
                self.exits.push(Vec::new());
                self.stmts(body);
                self.op_u32(op::JMP, top as u32, 0);
                let h = self.here();
                self.patch(end, h);
                for j in self.exits.pop().unwrap_or_default() {
                    self.patch(j, h);
                }
            }
            Stmt::Repeat(body, until) => {
                let top = self.here();
                self.exits.push(Vec::new());
                self.stmts(body);
                self.expr(until);
                self.op_u32(op::JZ, top as u32, -1);
                let h = self.here();
                for j in self.exits.pop().unwrap_or_default() {
                    self.patch(j, h);
                }
            }
            Stmt::Exit => {
                let j = self.jump(op::JMP);
                if let Some(l) = self.exits.last_mut() {
                    l.push(j);
                }
            }
            Stmt::Return => self.leave(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mythos_logic_vm::fb::Direction as VmDirection;

    /// The IR's catalog and the VM's blocks are written twice (the IR must not depend on a
    /// VM); this is what keeps them one thing.
    #[test]
    fn the_vm_has_every_standard_block_of_the_ir_with_the_same_members() {
        assert_eq!(StdFb::ALL.len(), FbKind::ALL.len());
        for fb in StdFb::ALL {
            let kind = vm_fb(fb);
            assert_eq!(fb.members().len(), kind.members().len(), "{}", fb.name());
            for (i, m) in fb.members().iter().enumerate() {
                let v = kind
                    .members()
                    .iter()
                    .find(|x| x.name == m.name)
                    .unwrap_or_else(|| panic!("{}.{} missing in the VM", fb.name(), m.name));
                assert_eq!(elem(m.ty), v.ty, "{}.{}", fb.name(), m.name);
                let same = matches!(
                    (m.direction, v.direction),
                    (Direction::Input, VmDirection::Input)
                        | (Direction::Output, VmDirection::Output)
                );
                assert!(same, "{}.{} direction", fb.name(), m.name);
                assert_eq!(member_offset(fb, i), v.offset);
            }
        }
    }

    #[test]
    fn a_unit_of_another_ir_version_or_with_recursion_is_refused_not_compiled() {
        let function = |name: &str, calls: usize| Pou {
            name: name.into(),
            kind: PouKind::Function { result: 0 },
            vars: vec![Var {
                name: name.into(),
                ty: Ty::Int,
                section: Section::Var,
                retention: Retention::Unqualified,
                init: None,
            }],
            body: vec![Stmt::Assign(
                Place::var(0),
                Expr {
                    ty: Ty::Int,
                    kind: ExprKind::Call {
                        pou: calls,
                        args: Vec::new(),
                    },
                },
            )],
        };
        let program = Pou {
            name: "main".into(),
            kind: PouKind::Program,
            vars: Vec::new(),
            body: Vec::new(),
        };
        let mut u = Unit::new();
        u.pous = vec![program.clone(), function("a", 2), function("b", 1)];
        let message = emit(&u).err().expect("recursion is refused").message;
        assert!(
            message.contains("recursion is not allowed") && message.contains("a, b"),
            "{message}"
        );

        let mut u = Unit::new();
        u.pous = vec![program];
        assert!(emit(&u).is_ok());
        u.ir_version = IR_VERSION + 1;
        let message = emit(&u).err().expect("another version is refused").message;
        assert!(message.contains("IR version"), "{message}");
    }

    /// A unit that did not come through the ST checker (which refuses it with the place) is
    /// refused here too: an array symbol cannot say "retained in part".
    #[test]
    fn a_unit_with_an_array_retained_in_part_is_refused_not_laid_out() {
        let var = |name: &str, ty: Ty, retention: Retention| Var {
            name: name.into(),
            ty,
            section: Section::Var,
            retention,
            init: None,
        };
        let block = Pou {
            name: "cell".into(),
            kind: PouKind::FunctionBlock,
            vars: vec![
                var("kept", Ty::Int, Retention::Retain),
                var("plain", Ty::Int, Retention::Unqualified),
            ],
            body: Vec::new(),
        };
        let mut u = Unit::new();
        let cells = u.array(vec![(1, 2)], Ty::Instance(1));
        let program = |retention| Pou {
            name: "main".into(),
            kind: PouKind::Program,
            vars: vec![var("cells", cells, retention)],
            body: Vec::new(),
        };
        u.pous = vec![program(Retention::Unqualified), block.clone()];
        let message = emit(&u).err().expect("retained in part is refused").message;
        assert_eq!(
            message,
            "cells would be retained in part; an array is retained or not as a whole"
        );
        // NON_RETAIN on the array does not override `kept`, which its type declares RETAIN.
        u.pous = vec![program(Retention::NonRetain), block.clone()];
        assert!(emit(&u).is_err());
        // RETAIN makes every member retained: the array is, as a whole.
        u.pous = vec![program(Retention::Retain), block];
        let out = emit(&u).expect("retained as a whole");
        assert_eq!(out.symbols.len(), 1);
        assert!(out.symbols[0].retain);
    }
}
