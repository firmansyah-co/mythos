//! What the end-to-end tests share: compile Structured Text, load it into the VM the way a
//! runner does, scan, and read variables by their path.
#![allow(dead_code)]

use mythos_logic_compiler::{compile, Compiled, TargetProfile};
use mythos_logic_vm::{
    cold_start, read, read_string, scan, write, ElemType, Fault, Frame, Program, Value,
};

pub const MS: i64 = 1_000_000;
pub const BUDGET: u32 = 1_000_000;

pub struct Run {
    pub c: Compiled,
    pub memory: Vec<u8>,
    stack: Vec<u64>,
    calls: Vec<Frame>,
    pub now: i64,
}

impl Run {
    pub fn new(src: &str) -> Run {
        let c = compile(src, &TargetProfile::pc()).unwrap_or_else(|d| {
            panic!(
                "{}",
                d.iter()
                    .map(|d| d.to_string())
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        });
        let p = Program::load(&c.bytecode).expect("the VM loads what the compiler made");
        let mut memory = vec![0; p.memory_size()];
        cold_start(&p, &mut memory).unwrap();
        // Exactly what the program declares, as a microcontroller's firmware would give.
        let stack = vec![0; p.stack_depth()];
        let calls = vec![Frame::default(); p.call_depth()];
        Run {
            c,
            memory,
            stack,
            calls,
            now: 0,
        }
    }
    pub fn scan(&mut self) -> Result<(), Fault> {
        let p = Program::load(&self.c.bytecode).unwrap();
        scan(
            &p,
            &mut self.memory,
            &mut self.stack,
            &mut self.calls,
            self.now,
            BUDGET,
        )
        .map(|_| ())
    }
    pub fn scan_at(&mut self, now_ms: i64) {
        self.now = now_ms * MS;
        self.scan().unwrap();
    }
    /// The value of the variable at `path`; an array element as `name[i]` or `name[i, j]`.
    pub fn get(&self, path: &str) -> Value {
        let (offset, ty) = self.locate(path);
        read(&self.memory, offset, elem(&ty)).unwrap()
    }
    /// The characters of the STRING at `path`, each byte the character of that code.
    pub fn text(&self, path: &str) -> String {
        let (offset, ty) = self.locate(path);
        let n: u16 = ty
            .strip_prefix("STRING[")
            .and_then(|t| t.strip_suffix(']'))
            .and_then(|t| t.parse().ok())
            .unwrap_or_else(|| panic!("{path} is a {ty}, not a STRING"));
        let bytes = read_string(&self.memory, offset, n)
            .unwrap_or_else(|| panic!("{path} holds a length past {n}"));
        bytes.iter().map(|b| *b as char).collect()
    }
    pub fn set(&mut self, path: &str, v: Value) {
        let (offset, _) = self.locate(path);
        assert!(write(&mut self.memory, offset, v));
    }
    pub fn int(&self, path: &str) -> i64 {
        match self.get(path) {
            Value::Sint(v) => v as i64,
            Value::Int(v) => v as i64,
            Value::Dint(v) => v as i64,
            Value::Lint(v) | Value::Time(v) => v,
            other => panic!("{path} is {other:?}"),
        }
    }
    pub fn bool(&self, path: &str) -> bool {
        match self.get(path) {
            Value::Bool(v) => v,
            other => panic!("{path} is {other:?}"),
        }
    }
    pub fn real(&self, path: &str) -> f64 {
        match self.get(path) {
            Value::Real(v) => v as f64,
            Value::Lreal(v) => v,
            other => panic!("{path} is {other:?}"),
        }
    }

    /// Where `path` is and its elementary type, from what the compiler gives a host alone,
    /// as a monitor would find it: a symbol; or an array symbol, then elements (`[i, j]`)
    /// and members (`.name`) of what it holds, by the layouts of the types.
    fn locate(&self, path: &str) -> (u32, String) {
        if let Some(s) = self.c.symbol(path) {
            assert!(s.array.is_none(), "{path} is a whole array");
            return (s.offset, s.ty.clone());
        }
        let s = self
            .c
            .symbols
            .iter()
            .filter(|s| {
                let bytes = path.as_bytes();
                s.array.is_some()
                    && bytes.len() > s.path.len()
                    && path[..s.path.len()].eq_ignore_ascii_case(&s.path)
                    && bytes[s.path.len()] == b'['
            })
            .max_by_key(|s| s.path.len())
            .unwrap_or_else(|| panic!("no symbol {path}"));
        let mut rest = &path[s.path.len()..];
        let (mut offset, mut ty, mut array) = (s.offset, s.ty.clone(), s.array.clone());
        while !rest.is_empty() {
            if let Some(inner) = rest.strip_prefix('[') {
                let close = inner.find(']').expect("a closing bracket");
                let info = array
                    .take()
                    .unwrap_or_else(|| panic!("{path}: a {ty} is not an array"));
                let subscripts: Vec<i64> = inner[..close]
                    .split(',')
                    .map(|x| x.trim().parse().unwrap())
                    .collect();
                assert_eq!(subscripts.len(), info.bounds.len(), "{path}");
                let mut position = 0i64;
                for (i, (lo, hi)) in subscripts.iter().zip(&info.bounds) {
                    assert!(i >= lo && i <= hi, "{path}: {i} outside {lo}..{hi}");
                    position = position * (hi - lo + 1) + (i - lo);
                }
                offset += position as u32 * info.element_size;
                ty = info.element_type;
                rest = &inner[close + 1..];
            } else {
                let inner = rest.strip_prefix('.').expect("a member");
                let end = inner.find(['.', '[']).unwrap_or(inner.len());
                let layout = self
                    .c
                    .types
                    .iter()
                    .find(|t| t.name.eq_ignore_ascii_case(&ty))
                    .unwrap_or_else(|| panic!("{path}: no layout for {ty}"));
                let m = layout
                    .members
                    .iter()
                    .find(|m| m.name.eq_ignore_ascii_case(&inner[..end]))
                    .unwrap_or_else(|| panic!("{path}: {ty} has no member {}", &inner[..end]));
                offset += m.offset;
                ty = m.ty.clone();
                array = m.array.clone();
                rest = &inner[end..];
            }
        }
        assert!(array.is_none(), "{path} is a whole array");
        (offset, ty)
    }
}

pub fn elem(ty: &str) -> ElemType {
    match ty {
        "BOOL" => ElemType::Bool,
        "SINT" => ElemType::Sint,
        "INT" => ElemType::Int,
        "DINT" => ElemType::Dint,
        "LINT" => ElemType::Lint,
        "REAL" => ElemType::Real,
        "LREAL" => ElemType::Lreal,
        "TIME" => ElemType::Time,
        other => panic!("{other} is not an elementary type"),
    }
}

/// The diagnostics of a source that must not compile, as `line:col: message`.
pub fn diagnostics(src: &str) -> Vec<String> {
    match compile(src, &TargetProfile::pc()) {
        Ok(_) => panic!("compiled, but should not have:\n{src}"),
        Err(d) => d.iter().map(|d| d.to_string()).collect(),
    }
}

/// The one diagnostic of a source that must not compile.
pub fn refusal(src: &str) -> String {
    let d = diagnostics(src);
    assert_eq!(d.len(), 1, "one diagnostic expected: {d:?}");
    d.into_iter().next().unwrap()
}
