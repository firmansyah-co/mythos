//! The VM backend: lays the IR's variables out in memory and emits bytecode for it.

use crate::ir::*;
use mythos_logic_vm::format::{crc32, op, ElemType, FORMAT_VERSION, MAGIC};

/// Where a variable (and each member of an instance) was put, for the host: binding a variable
/// to a tag, monitoring and forcing all go by these.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Symbol {
    /// `name` or `instance.member`.
    pub path: String,
    #[serde(rename = "type")]
    pub ty: &'static str,
    pub section: crate::ast::Section,
    pub offset: u32,
    pub size: u32,
    /// The host may set it (a program input, or an FB input it is not given here).
    pub writable: bool,
}

pub struct Output {
    pub bytes: Vec<u8>,
    pub symbols: Vec<Symbol>,
    pub memory_size: u32,
    pub code_size: u32,
    pub stack_depth: u16,
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
        Ty::Fb(_) => unreachable!("an instance is never a value"),
    }
}

fn size(t: Ty) -> u32 {
    match t {
        Ty::Bool | Ty::Sint => 1,
        Ty::Int => 2,
        Ty::Dint | Ty::Real => 4,
        Ty::Lint | Ty::Lreal | Ty::Time => 8,
        Ty::Fb(k) => k.size(),
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
        Ty::Fb(_) => unreachable!("an instance is not an element"),
    }
}

struct Emitter<'p> {
    p: &'p Program,
    offsets: Vec<u32>,
    code: Vec<u8>,
    depth: i32,
    max_depth: i32,
    /// For each loop being emitted, the jumps its EXITs must be patched to.
    exits: Vec<Vec<usize>>,
    /// Scratch variables (CASE selectors), allocated after the program's own.
    memory_size: u32,
}

pub fn emit(p: &Program) -> Output {
    let mut offsets = Vec::new();
    let mut at = 0u32;
    for v in &p.vars {
        offsets.push(at);
        at += size(v.ty);
    }
    let mut e = Emitter {
        p,
        offsets,
        code: Vec::new(),
        depth: 0,
        max_depth: 0,
        exits: Vec::new(),
        memory_size: at,
    };
    e.stmts(&p.body);
    e.op(op::END, 0);

    // Initial memory image.
    let mut init = vec![0u8; e.memory_size as usize];
    for (i, v) in p.vars.iter().enumerate() {
        let Some(k) = v.init else { continue };
        let o = e.offsets[i] as usize;
        let bytes: Vec<u8> = match (v.ty, k) {
            (Ty::Bool, Const::Bool(b)) => vec![b as u8],
            (Ty::Sint, Const::Int(x)) => (x as i8).to_le_bytes().to_vec(),
            (Ty::Int, Const::Int(x)) => (x as i16).to_le_bytes().to_vec(),
            (Ty::Dint, Const::Int(x)) => (x as i32).to_le_bytes().to_vec(),
            (Ty::Lint, Const::Int(x)) | (Ty::Time, Const::Time(x)) => x.to_le_bytes().to_vec(),
            (Ty::Real, Const::Real(x)) => (x as f32).to_le_bytes().to_vec(),
            (Ty::Real, Const::Int(x)) => (x as f32).to_le_bytes().to_vec(),
            (Ty::Lreal, Const::Real(x)) => x.to_le_bytes().to_vec(),
            (Ty::Lreal, Const::Int(x)) => (x as f64).to_le_bytes().to_vec(),
            (ty, k) => unreachable!("the checker let {k:?} initialise a {}", ty.name()),
        };
        init[o..o + bytes.len()].copy_from_slice(&bytes);
    }

    let mut symbols = Vec::new();
    for (i, v) in p.vars.iter().enumerate() {
        let o = e.offsets[i];
        match v.ty {
            Ty::Fb(k) => {
                for m in k.members() {
                    symbols.push(Symbol {
                        path: format!("{}.{}", v.name, m.name),
                        ty: elem_to_ty(m.ty).name(),
                        section: v.section,
                        offset: o + m.offset,
                        size: m.ty.size(),
                        writable: false,
                    });
                }
            }
            ty => symbols.push(Symbol {
                path: v.name.clone(),
                ty: ty.name(),
                section: v.section,
                offset: o,
                size: size(ty),
                writable: v.section == crate::ast::Section::Input,
            }),
        }
    }

    let mut bytes = Vec::with_capacity(16 + init.len() + e.code.len() + 4);
    bytes.extend_from_slice(&MAGIC);
    bytes.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    let stack_depth = e.max_depth.max(1) as u16;
    bytes.extend_from_slice(&stack_depth.to_le_bytes());
    bytes.extend_from_slice(&e.memory_size.to_le_bytes());
    bytes.extend_from_slice(&(e.code.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&init);
    bytes.extend_from_slice(&e.code);
    let crc = crc32(&bytes);
    bytes.extend_from_slice(&crc.to_le_bytes());
    Output {
        memory_size: e.memory_size,
        code_size: e.code.len() as u32,
        stack_depth,
        bytes,
        symbols,
    }
}

impl Emitter<'_> {
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

    fn addr(&self, p: Place) -> u32 {
        let base = self.offsets[p.var];
        match (self.p.vars[p.var].ty, p.member) {
            (Ty::Fb(k), Some(m)) => base + k.members()[m].offset,
            _ => base,
        }
    }

    fn load(&mut self, ty: Ty, addr: u32) {
        let code = match ty {
            Ty::Bool => op::LD_BOOL,
            Ty::Sint => op::LD_I8,
            Ty::Int => op::LD_I16,
            Ty::Dint => op::LD_I32,
            Ty::Lint | Ty::Time => op::LD_I64,
            Ty::Real => op::LD_F32,
            Ty::Lreal => op::LD_F64,
            Ty::Fb(_) => unreachable!(),
        };
        self.op_u32(code, addr, 1);
    }
    fn store(&mut self, ty: Ty, addr: u32) {
        let code = match ty {
            Ty::Bool => op::ST_BOOL,
            Ty::Sint => op::ST_8,
            Ty::Int => op::ST_16,
            Ty::Dint | Ty::Real => op::ST_32,
            Ty::Lint | Ty::Lreal | Ty::Time => op::ST_64,
            Ty::Fb(_) => unreachable!(),
        };
        self.op_u32(code, addr, -1);
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
            (_, k) => unreachable!("constant {k:?} typed {}", ty.name()),
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
            ExprKind::Load(p) => {
                let a = self.addr(*p);
                self.load(e.ty, a);
            }
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
            ExprKind::Cmp(c, a, b) => {
                self.expr(a);
                self.expr(b);
                let base = match class(a.ty) {
                    Class::Bool | Class::I32 => op::CMP_I32,
                    Class::I64 => op::CMP_I64,
                    Class::F32 => op::CMP_F32,
                    Class::F64 => op::CMP_F64,
                };
                let which = match c {
                    Cmp::Eq => 0,
                    Cmp::Ne => 1,
                    Cmp::Lt => 2,
                    Cmp::Le => 3,
                    Cmp::Gt => 4,
                    Cmp::Ge => 5,
                };
                self.op(base + which, -1);
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
            ExprKind::Convert(a) => {
                self.expr(a);
                self.convert(a.ty, e.ty);
            }
        }
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
                self.expr(v);
                let (ty, a) = (self.p.place_ty(*p), self.addr(*p));
                self.store(ty, a);
            }
            Stmt::Call {
                var,
                inputs,
                outputs,
            } => {
                let Ty::Fb(k) = self.p.vars[*var].ty else {
                    unreachable!()
                };
                let base = self.offsets[*var];
                for (m, v) in inputs {
                    self.expr(v);
                    let mem = k.members()[*m];
                    self.store(elem_to_ty(mem.ty), base + mem.offset);
                }
                self.op(op::FB, 0);
                self.code.push(k.code());
                self.code.extend_from_slice(&base.to_le_bytes());
                for (m, target) in outputs {
                    let mem = k.members()[*m];
                    let mty = elem_to_ty(mem.ty);
                    self.load(mty, base + mem.offset);
                    let tty = self.p.place_ty(*target);
                    self.convert(mty, tty);
                    let a = self.addr(*target);
                    self.store(tty, a);
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
                let tmp = self.memory_size;
                self.memory_size += size(sel.ty);
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
                let ty = self.p.place_ty(*var);
                let a = self.addr(*var);
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
            Stmt::Return => self.op(op::END, 0),
        }
    }
}
