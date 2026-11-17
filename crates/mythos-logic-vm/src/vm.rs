//! Running a scan.

use crate::fb::{self, FbKind};
use crate::format::rfn;
use crate::format::{
    access, op, place_at, string_operand_at, string_shape, string_size, u32_at, Cell, ElemType,
    Place, Program, Reach, PLACE_INDEXED, PLACE_LEN, PLACE_RELATIVE, SOP_LEN,
};
use crate::math;
use crate::strings::{self, Str};

/// Why a scan stopped before its end. The host then treats the program as stopped: its outputs
/// go to their safe values (ADR-013).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fault {
    /// The memory, the stack or the call frames given are smaller than the program declares.
    WrongSizes,
    /// More instructions than the budget: most likely a loop that never ends.
    BudgetExceeded,
    /// Integer division by zero, at this code offset. (`MOD` by zero is 0.)
    DivideByZero(u32),
    /// A push beyond the stack depth the program declares, at this code offset.
    StackOverflow(u32),
    StackUnderflow(u32),
    /// Execution reached a byte that is not an instruction, or ran off the end of the code, or
    /// an instruction reached outside the memory or the region it names: the program was not
    /// made by a correct compiler.
    BadCode(u32),
    /// An array subscript outside the bounds the array was declared with, at this code offset
    /// (IEC 61131-3 ed. 3 §6.4.4.5.1: an error, found at run time for a computed subscript).
    IndexOutOfRange(u32),
    /// Calls nested deeper than the program declares, at this code offset.
    CallDepthExceeded(u32),
    /// A string function would give a string longer than the one it is written to holds, at
    /// this code offset (IEC 61131-3 ed. 3 §6.6.2.5.11: producing a string longer than the
    /// maximum is an error).
    StringTooLong(u32),
    /// A string function was given a length or position below zero, or a position its string
    /// does not have, at this code offset (§6.6.2.5.11: both are errors).
    StringArgument(u32),
    /// A string whose stored length is greater than its declared length, at this code
    /// offset: its bytes were not written by the program's own instructions.
    BadString(u32),
    /// A string converted to an integer does not write one in the form of an integer literal,
    /// or writes one beyond 64 bits, at this code offset.
    BadNumber(u32),
    /// MUX was given a K outside the inputs it has, at this code offset (IEC 61131-3 ed. 3
    /// Table 32 footnote e: an error).
    SelectorOutOfRange(u32),
}

/// One call in progress: where to go back to, and the base to give back. The host supplies
/// as many as the program declares ([`Program::call_depth`]); what is in them does not matter.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Frame {
    ret: u32,
    base: u32,
}

/// What a scan did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScanReport {
    pub instructions: u32,
}

/// A value read from or written to memory by the host.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Value {
    Bool(bool),
    Sint(i8),
    Int(i16),
    Dint(i32),
    Lint(i64),
    Real(f32),
    Lreal(f64),
    /// Nanoseconds.
    Time(i64),
}

impl Value {
    pub fn elem_type(self) -> ElemType {
        match self {
            Value::Bool(_) => ElemType::Bool,
            Value::Sint(_) => ElemType::Sint,
            Value::Int(_) => ElemType::Int,
            Value::Dint(_) => ElemType::Dint,
            Value::Lint(_) => ElemType::Lint,
            Value::Real(_) => ElemType::Real,
            Value::Lreal(_) => ElemType::Lreal,
            Value::Time(_) => ElemType::Time,
        }
    }
}

/// Put the program's initial values into `memory` (a cold start).
pub fn cold_start(program: &Program, memory: &mut [u8]) -> Result<(), Fault> {
    if memory.len() != program.memory_size() {
        return Err(Fault::WrongSizes);
    }
    memory.copy_from_slice(program.init);
    Ok(())
}

/// Read the variable of type `ty` at `addr`; `None` when it lies outside `memory`.
pub fn read(memory: &[u8], addr: u32, ty: ElemType) -> Option<Value> {
    let a = addr as usize;
    let bytes = memory.get(a..a.checked_add(ty.size() as usize)?)?;
    let mut x = [0u8; 8];
    x[..bytes.len()].copy_from_slice(bytes);
    Some(match ty {
        ElemType::Bool => Value::Bool(x[0] != 0),
        ElemType::Sint => Value::Sint(x[0] as i8),
        ElemType::Int => Value::Int(i16::from_le_bytes([x[0], x[1]])),
        ElemType::Dint => Value::Dint(i32::from_le_bytes([x[0], x[1], x[2], x[3]])),
        ElemType::Real => Value::Real(f32::from_le_bytes([x[0], x[1], x[2], x[3]])),
        ElemType::Lint => Value::Lint(i64::from_le_bytes(x)),
        ElemType::Lreal => Value::Lreal(f64::from_le_bytes(x)),
        ElemType::Time => Value::Time(i64::from_le_bytes(x)),
    })
}

/// The characters of the `STRING[cap]` at `addr`; `None` when it lies outside `memory` or
/// holds a length greater than `cap`, which the program's own instructions never write.
pub fn read_string(memory: &[u8], addr: u32, cap: u16) -> Option<&[u8]> {
    let a = addr as usize;
    let head = memory.get(a..a.checked_add(2)?)?;
    let n = u16::from_le_bytes([head[0], head[1]]);
    if n > cap {
        return None;
    }
    memory.get(a + 2..a + 2 + n as usize)
}

/// Write `value` at `addr`. Returns `false`, writing nothing, when it lies outside `memory`.
/// The host calls this between scans only (inputs before, forced values), never during one.
pub fn write(memory: &mut [u8], addr: u32, value: Value) -> bool {
    let a = addr as usize;
    let n = value.elem_type().size() as usize;
    let Some(end) = a.checked_add(n) else {
        return false;
    };
    let Some(dst) = memory.get_mut(a..end) else {
        return false;
    };
    match value {
        Value::Bool(v) => dst[0] = v as u8,
        Value::Sint(v) => dst[0] = v as u8,
        Value::Int(v) => dst.copy_from_slice(&v.to_le_bytes()),
        Value::Dint(v) => dst.copy_from_slice(&v.to_le_bytes()),
        Value::Real(v) => dst.copy_from_slice(&v.to_le_bytes()),
        Value::Lint(v) | Value::Time(v) => dst.copy_from_slice(&v.to_le_bytes()),
        Value::Lreal(v) => dst.copy_from_slice(&v.to_le_bytes()),
    }
    true
}

/// Round to the nearest integer, a half to the nearest even one, as IEC 61131-3 ed. 3 Table 22
/// footnote b requires of REAL and LREAL to integer conversions (the convention of IEC 60559):
/// 1.5 and 2.5 both give 2. Without `std` (`core` has no `f64::round_ties_even`).
fn round_f64(x: f64) -> f64 {
    let t = x as i64 as f64; // truncates toward zero; saturates outside i64
    let frac = x - t;
    let odd = (t as i64) % 2 != 0;
    if frac > 0.5 || (frac == 0.5 && odd) {
        t + 1.0
    } else if frac < -0.5 || (frac == -0.5 && odd) {
        t - 1.0
    } else {
        t
    }
}

struct Stack<'s> {
    slots: &'s mut [u64],
    sp: usize,
    pc: u32,
}

impl Stack<'_> {
    fn push(&mut self, v: u64) -> Result<(), Fault> {
        let slot = self
            .slots
            .get_mut(self.sp)
            .ok_or(Fault::StackOverflow(self.pc))?;
        *slot = v;
        self.sp += 1;
        Ok(())
    }
    fn pop(&mut self) -> Result<u64, Fault> {
        if self.sp == 0 {
            return Err(Fault::StackUnderflow(self.pc));
        }
        self.sp -= 1;
        Ok(self.slots[self.sp])
    }
    fn pop2(&mut self) -> Result<(u64, u64), Fault> {
        let b = self.pop()?;
        let a = self.pop()?;
        Ok((a, b))
    }
}

fn i32_of(v: u64) -> i32 {
    v as u32 as i32
}
fn of_i32(v: i32) -> u64 {
    v as u32 as u64
}
fn f32_of(v: u64) -> f32 {
    f32::from_bits(v as u32)
}
fn of_f32(v: f32) -> u64 {
    v.to_bits() as u64
}
fn cmp<T: PartialOrd>(which: u8, a: T, b: T) -> u64 {
    (match which {
        0 => a == b,
        1 => a != b,
        2 => a < b,
        3 => a <= b,
        4 => a > b,
        _ => a >= b,
    }) as u64
}

/// `len` bytes at `start`, as a range of a memory of `size` bytes; `None` when they do not all
/// lie inside it. Every sum is checked, so nothing wraps, on a 32-bit target either.
fn range(start: u32, len: usize, size: usize) -> Option<core::ops::Range<usize>> {
    let start = start as usize;
    let end = start.checked_add(len)?;
    if end > size {
        return None;
    }
    Some(start..end)
}

/// Where a place of `len` bytes starts: its address, from the base if it says so, plus an
/// offset popped from the stack if it takes one. The offset may not take the access outside
/// the place's region. The loader checked the region; `range` checks the result again.
fn resolve(p: Place, len: usize, base: u32, s: &mut Stack) -> Result<u32, Fault> {
    let bad = Fault::BadCode(s.pc);
    if p.mode > (PLACE_RELATIVE | PLACE_INDEXED) {
        return Err(bad);
    }
    let mut start = p.addr;
    if p.mode & PLACE_RELATIVE != 0 {
        start = start.checked_add(base).ok_or(bad)?;
    }
    let mut offset = 0u32;
    if p.mode & PLACE_INDEXED != 0 {
        offset = s.pop()? as u32;
    }
    let end = u32::try_from(len)
        .ok()
        .and_then(|len| offset.checked_add(len))
        .ok_or(bad)?;
    if end > p.span {
        return Err(bad);
    }
    start.checked_add(offset).ok_or(bad)
}

/// Copy the bytes at `src` to `dst`, two ranges of one length inside `memory`, which may
/// overlap. `None`, copying nothing, when they are not. (`copy_within` would do it, at the
/// price of `memmove` in a microcontroller's flash: 1.3 KB on a Cortex-M0.)
pub(crate) fn copy(
    memory: &mut [u8],
    src: core::ops::Range<usize>,
    dst: core::ops::Range<usize>,
) -> Option<()> {
    if src.end <= dst.start {
        let (low, high) = memory.split_at_mut_checked(dst.start)?;
        high.get_mut(..src.len())?.copy_from_slice(low.get(src)?);
    } else if dst.end <= src.start {
        let (low, high) = memory.split_at_mut_checked(src.start)?;
        low.get_mut(dst.clone())?
            .copy_from_slice(high.get(..dst.len())?);
    } else if dst.start <= src.start {
        // Overlapping, the destination first: from the front, so nothing is read after it
        // was written.
        for k in 0..src.len() {
            let byte = *memory.get(src.start + k)?;
            *memory.get_mut(dst.start + k)? = byte;
        }
    } else {
        for k in (0..src.len()).rev() {
            let byte = *memory.get(src.start + k)?;
            *memory.get_mut(dst.start + k)? = byte;
        }
    }
    Some(())
}

/// The value a load pushes for the bytes of a cell: `b` is exactly the cell's bytes.
fn fetch(cell: Cell, b: &[u8]) -> u64 {
    let mut x = [0u8; 8];
    if let Some(low) = x.get_mut(..b.len()) {
        low.copy_from_slice(b);
    }
    let raw = u64::from_le_bytes(x);
    match cell {
        Cell::Bool => (raw != 0) as u64,
        Cell::I8 => of_i32(raw as u8 as i8 as i32),
        Cell::I16 => of_i32(raw as u16 as i16 as i32),
        Cell::W32 | Cell::W64 => raw,
    }
}

/// Run one scan of `program` over `memory` at time `now` (nanoseconds of a monotonic clock),
/// executing at most `budget` instructions. `stack` and `calls` are working space the host
/// owns: at least [`Program::stack_depth`] slots and [`Program::call_depth`] frames. What the
/// host gives beyond that is not used.
pub fn scan(
    program: &Program,
    memory: &mut [u8],
    stack: &mut [u64],
    calls: &mut [Frame],
    now: i64,
    budget: u32,
) -> Result<ScanReport, Fault> {
    if memory.len() != program.memory_size() {
        return Err(Fault::WrongSizes);
    }
    // The stack is as deep, and calls may nest as deep, as the program declares and no
    // deeper, whatever the host gave: a program behaves the same on every host, and one that
    // would go deeper than it says is found on a PC, not on the controller.
    let stack = stack
        .get_mut(..program.stack_depth())
        .ok_or(Fault::WrongSizes)?;
    let calls = calls
        .get_mut(..program.call_depth())
        .ok_or(Fault::WrongSizes)?;
    let code = program.code;
    let mut s = Stack {
        slots: stack,
        sp: 0,
        pc: 0,
    };
    let mut pc = program.entry() as usize;
    // The instance a function block's body runs on, and how many calls are in progress.
    let mut base = 0u32;
    let mut depth = 0usize;
    let mut count = 0u32;
    loop {
        if count >= budget {
            return Err(Fault::BudgetExceeded);
        }
        count += 1;
        s.pc = pc as u32;
        let at = pc as u32;
        let opcode = *code.get(pc).ok_or(Fault::BadCode(at))?;
        let len = crate::format::instruction_len(opcode).ok_or(Fault::BadCode(at))?;
        if pc + len > code.len() {
            return Err(Fault::BadCode(at));
        }
        let operand = || u32_at(code, pc + 1);
        let mut next = pc + len;
        // Loads and stores were checked against the memory size when the program was loaded,
        // but `get` keeps that a fact of this function too.
        let mem = |addr: u32, n: usize| -> Result<&[u8], Fault> {
            range(addr, n, memory.len())
                .and_then(|r| memory.get(r))
                .ok_or(Fault::BadCode(at))
        };
        match opcode {
            op::END => {
                return Ok(ScanReport {
                    instructions: count,
                })
            }
            op::JMP => next = operand() as usize,
            op::JZ => {
                if s.pop()? == 0 {
                    next = operand() as usize;
                }
            }
            op::CALL | op::CALLFB => {
                let (target, instance) = program
                    .body(u16::from_le_bytes([code[pc + 1], code[pc + 2]]))
                    .ok_or(Fault::BadCode(at))?;
                // The instance of a function block is where its place says, whole: an offset
                // may not put its end outside the place's region, nor outside the memory.
                let callee_base = if opcode == op::CALLFB {
                    let n = instance as usize;
                    let start = resolve(place_at(code, pc + 3), n, base, &mut s)?;
                    range(start, n, memory.len()).ok_or(Fault::BadCode(at))?;
                    start
                } else {
                    base
                };
                let frame = calls.get_mut(depth).ok_or(Fault::CallDepthExceeded(at))?;
                *frame = Frame {
                    ret: next as u32,
                    base,
                };
                depth += 1;
                base = callee_base;
                next = target as usize;
            }
            op::RET => {
                // The loader lets no RET into the program's body; a return with no call in
                // progress is still not followed.
                depth = depth.checked_sub(1).ok_or(Fault::BadCode(at))?;
                let frame = calls.get(depth).ok_or(Fault::BadCode(at))?;
                base = frame.base;
                next = frame.ret as usize;
            }
            op::INIT => {
                let r = range(operand(), u32_at(code, pc + 5) as usize, memory.len())
                    .ok_or(Fault::BadCode(at))?;
                let from = program.init.get(r.clone()).ok_or(Fault::BadCode(at))?;
                memory
                    .get_mut(r)
                    .ok_or(Fault::BadCode(at))?
                    .copy_from_slice(from);
            }
            op::MOVE => {
                let n = operand() as usize;
                let src = resolve(place_at(code, pc + 5 + PLACE_LEN), n, base, &mut s)?;
                let dst = resolve(place_at(code, pc + 5), n, base, &mut s)?;
                let src = range(src, n, memory.len()).ok_or(Fault::BadCode(at))?;
                let dst = range(dst, n, memory.len()).ok_or(Fault::BadCode(at))?;
                copy(memory, src, dst).ok_or(Fault::BadCode(at))?;
            }
            op::IDX | op::IDX64 => {
                let v = s.pop()?;
                let lo = operand() as i32;
                let count = u32_at(code, pc + 5);
                let stride = u32_at(code, pc + 9);
                // How far the subscript is above the lower bound; negative when it is below.
                let above = if opcode == op::IDX {
                    (i32_of(v) as i64) - (lo as i64)
                } else {
                    (v as i64).checked_sub(lo as i64).unwrap_or(-1)
                };
                // IEC 61131-3 ed. 3 §6.4.4.5.1: a subscript outside the declared range is an
                // error. Here it stops the scan; it never reaches another variable.
                let k = u32::try_from(above)
                    .ok()
                    .filter(|k| *k < count)
                    .ok_or(Fault::IndexOutOfRange(at))?;
                // The loader checked that `count * stride` fits in the memory, so this does
                // not wrap.
                s.push(k.wrapping_mul(stride) as u64)?;
            }
            op::LD_BOOL => {
                let v = mem(operand(), 1)?[0] != 0;
                s.push(v as u64)?;
            }
            op::LD_I8 => {
                let v = mem(operand(), 1)?[0] as i8;
                s.push(of_i32(v as i32))?;
            }
            op::LD_I16 => {
                let b = mem(operand(), 2)?;
                s.push(of_i32(i16::from_le_bytes([b[0], b[1]]) as i32))?;
            }
            op::LD_I32 | op::LD_F32 => {
                let b = mem(operand(), 4)?;
                s.push(u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as u64)?;
            }
            op::LD_I64 | op::LD_F64 => {
                let b = mem(operand(), 8)?;
                let mut x = [0u8; 8];
                x.copy_from_slice(b);
                s.push(u64::from_le_bytes(x))?;
            }
            op::ST_BOOL | op::ST_8 | op::ST_16 | op::ST_32 | op::ST_64 => {
                let v = s.pop()?;
                let n = match opcode {
                    op::ST_BOOL | op::ST_8 => 1,
                    op::ST_16 => 2,
                    op::ST_32 => 4,
                    _ => 8,
                };
                let r = range(operand(), n, memory.len()).ok_or(Fault::BadCode(at))?;
                let dst = memory.get_mut(r).ok_or(Fault::BadCode(at))?;
                let v = if opcode == op::ST_BOOL {
                    (v != 0) as u64
                } else {
                    v
                };
                dst.copy_from_slice(&v.to_le_bytes()[..n]);
            }
            op::PUSH32 => s.push(operand() as u64)?,
            op::PUSH64 => {
                let mut x = [0u8; 8];
                x.copy_from_slice(&code[pc + 1..pc + 9]);
                s.push(u64::from_le_bytes(x))?;
            }
            op::ADD_I32..=op::MOD_I32 => {
                let (a, b) = s.pop2()?;
                let (a, b) = (i32_of(a), i32_of(b));
                let r = match opcode {
                    op::ADD_I32 => a.wrapping_add(b),
                    op::SUB_I32 => a.wrapping_sub(b),
                    op::MUL_I32 => a.wrapping_mul(b),
                    op::DIV_I32 => {
                        if b == 0 {
                            return Err(Fault::DivideByZero(at));
                        }
                        a.wrapping_div(b)
                    }
                    // IEC 61131-3 ed. 3 Table 29 footnote e: MOD by zero is 0, not an error.
                    _ => {
                        if b == 0 {
                            0
                        } else {
                            a.wrapping_rem(b)
                        }
                    }
                };
                s.push(of_i32(r))?;
            }
            op::NEG_I32 => {
                let a = i32_of(s.pop()?);
                s.push(of_i32(a.wrapping_neg()))?;
            }
            op::ADD_I64..=op::MOD_I64 => {
                let (a, b) = s.pop2()?;
                let (a, b) = (a as i64, b as i64);
                let r = match opcode {
                    op::ADD_I64 => a.wrapping_add(b),
                    op::SUB_I64 => a.wrapping_sub(b),
                    op::MUL_I64 => a.wrapping_mul(b),
                    op::DIV_I64 => {
                        if b == 0 {
                            return Err(Fault::DivideByZero(at));
                        }
                        a.wrapping_div(b)
                    }
                    // IEC 61131-3 ed. 3 Table 29 footnote e: MOD by zero is 0, not an error.
                    _ => {
                        if b == 0 {
                            0
                        } else {
                            a.wrapping_rem(b)
                        }
                    }
                };
                s.push(r as u64)?;
            }
            op::NEG_I64 => {
                let a = s.pop()? as i64;
                s.push(a.wrapping_neg() as u64)?;
            }
            op::ADD_F32..=op::DIV_F32 => {
                let (a, b) = s.pop2()?;
                let (a, b) = (f32_of(a), f32_of(b));
                let r = match opcode {
                    op::ADD_F32 => a + b,
                    op::SUB_F32 => a - b,
                    op::MUL_F32 => a * b,
                    _ => a / b,
                };
                s.push(of_f32(r))?;
            }
            op::NEG_F32 => {
                let a = f32_of(s.pop()?);
                s.push(of_f32(-a))?;
            }
            op::ADD_F64..=op::DIV_F64 => {
                let (a, b) = s.pop2()?;
                let (a, b) = (f64::from_bits(a), f64::from_bits(b));
                let r = match opcode {
                    op::ADD_F64 => a + b,
                    op::SUB_F64 => a - b,
                    op::MUL_F64 => a * b,
                    _ => a / b,
                };
                s.push(r.to_bits())?;
            }
            op::NEG_F64 => {
                let a = f64::from_bits(s.pop()?);
                s.push((-a).to_bits())?;
            }
            x if (op::CMP_I32..op::CMP_I32 + 6).contains(&x) => {
                let (a, b) = s.pop2()?;
                s.push(cmp(x - op::CMP_I32, i32_of(a), i32_of(b)))?;
            }
            x if (op::CMP_I64..op::CMP_I64 + 6).contains(&x) => {
                let (a, b) = s.pop2()?;
                s.push(cmp(x - op::CMP_I64, a as i64, b as i64))?;
            }
            x if (op::CMP_F32..op::CMP_F32 + 6).contains(&x) => {
                let (a, b) = s.pop2()?;
                s.push(cmp(x - op::CMP_F32, f32_of(a), f32_of(b)))?;
            }
            x if (op::CMP_F64..op::CMP_F64 + 6).contains(&x) => {
                let (a, b) = s.pop2()?;
                s.push(cmp(x - op::CMP_F64, f64::from_bits(a), f64::from_bits(b)))?;
            }
            op::AND | op::OR | op::XOR => {
                let (a, b) = s.pop2()?;
                let (a, b) = (a != 0, b != 0);
                let r = match opcode {
                    op::AND => a && b,
                    op::OR => a || b,
                    _ => a != b,
                };
                s.push(r as u64)?;
            }
            op::NOT => {
                let a = s.pop()?;
                s.push((a == 0) as u64)?;
            }
            op::I32_TO_I64 => {
                let a = i32_of(s.pop()?);
                s.push(a as i64 as u64)?;
            }
            op::I64_TO_I32 => {
                let a = s.pop()? as i64;
                s.push(of_i32(a as i32))?;
            }
            op::I32_TO_F32 => {
                let a = i32_of(s.pop()?);
                s.push(of_f32(a as f32))?;
            }
            op::I32_TO_F64 => {
                let a = i32_of(s.pop()?);
                s.push((a as f64).to_bits())?;
            }
            op::I64_TO_F64 => {
                let a = s.pop()? as i64;
                s.push((a as f64).to_bits())?;
            }
            op::I64_TO_F32 => {
                let a = s.pop()? as i64;
                s.push(of_f32(a as f32))?;
            }
            op::F32_TO_F64 => {
                let a = f32_of(s.pop()?);
                s.push((a as f64).to_bits())?;
            }
            op::F64_TO_F32 => {
                let a = f64::from_bits(s.pop()?);
                s.push(of_f32(a as f32))?;
            }
            op::F32_TO_I32 => {
                let a = f32_of(s.pop()?) as f64;
                s.push(of_i32(round_f64(a) as i32))?;
            }
            op::F64_TO_I32 => {
                let a = f64::from_bits(s.pop()?);
                s.push(of_i32(round_f64(a) as i32))?;
            }
            op::F32_TO_I64 => {
                let a = f32_of(s.pop()?) as f64;
                s.push(round_f64(a) as i64 as u64)?;
            }
            op::F64_TO_I64 => {
                let a = f64::from_bits(s.pop()?);
                s.push(round_f64(a) as i64 as u64)?;
            }
            op::WRAP_I16 => {
                let a = i32_of(s.pop()?);
                s.push(of_i32(a as i16 as i32))?;
            }
            op::WRAP_I8 => {
                let a = i32_of(s.pop()?);
                s.push(of_i32(a as i8 as i32))?;
            }
            op::I64_TO_BOOL => {
                let a = s.pop()?;
                s.push((a != 0) as u64)?;
            }
            op::FB | op::FBP => {
                let kind = FbKind::from_code(code[pc + 1]).ok_or(Fault::BadCode(at))?;
                let n = kind.size() as usize;
                let start = if opcode == op::FB {
                    u32_at(code, pc + 2)
                } else {
                    resolve(place_at(code, pc + 2), n, base, &mut s)?
                };
                let r = range(start, n, memory.len()).ok_or(Fault::BadCode(at))?;
                let x = memory.get_mut(r).ok_or(Fault::BadCode(at))?;
                fb::run(kind, x, now);
            }
            op::RFN | op::RFN32 => {
                let v = s.pop()?;
                let x = if opcode == op::RFN {
                    f64::from_bits(v)
                } else {
                    f32_of(v) as f64
                };
                let r = match code[pc + 1] {
                    rfn::SQRT => math::sqrt(x),
                    rfn::LN => math::ln(x),
                    rfn::LOG => math::log10(x),
                    rfn::EXP => math::exp(x),
                    rfn::SIN => math::sin(x),
                    rfn::COS => math::cos(x),
                    rfn::TAN => math::tan(x),
                    rfn::ASIN => math::asin(x),
                    rfn::ACOS => math::acos(x),
                    rfn::ATAN => math::atan(x),
                    rfn::TRUNC => math::trunc(x),
                    _ => return Err(Fault::BadCode(at)),
                };
                s.push(if opcode == op::RFN {
                    r.to_bits()
                } else {
                    of_f32(r as f32)
                })?;
            }
            op::ATAN2_F64..=op::EXPTI_F32 => {
                let (a, b) = s.pop2()?;
                let wide = matches!(opcode, op::ATAN2_F64 | op::EXPT_F64 | op::EXPTI_F64);
                let real = |v: u64| {
                    if wide {
                        f64::from_bits(v)
                    } else {
                        f32_of(v) as f64
                    }
                };
                let r = match opcode {
                    op::ATAN2_F64 | op::ATAN2_F32 => math::atan2(real(a), real(b)),
                    op::EXPT_F64 | op::EXPT_F32 => math::pow(real(a), real(b)),
                    _ => math::pow(real(a), b as i64 as f64),
                };
                s.push(if wide { r.to_bits() } else { of_f32(r as f32) })?;
            }
            op::ABS_I32 => {
                let a = i32_of(s.pop()?);
                s.push(of_i32(a.wrapping_abs()))?;
            }
            op::ABS_I64 => {
                let a = s.pop()? as i64;
                s.push(a.wrapping_abs() as u64)?;
            }
            op::ABS_F32 => {
                let a = s.pop()?;
                s.push(a & 0x7FFF_FFFF)?;
            }
            op::ABS_F64 => {
                let a = s.pop()?;
                s.push(a & !(1 << 63))?;
            }
            op::CHKSEL => {
                let k = s.pop()? as i64;
                if k < 0 || k >= operand() as i64 {
                    return Err(Fault::SelectorOutOfRange(at));
                }
            }
            x if string_shape(x).is_some() => {
                let (operands, ints) = string_shape(x).unwrap_or((0, 0));
                // The integers are on top, the last pushed first popped.
                let mut n = [0i64; 2];
                for k in (0..ints).rev() {
                    n[k] = s.pop()? as i64;
                }
                let first = pc + 1 + (x == op::SCMP) as usize;
                let mut o = [Str { cap: 0, at: 0 }; 3];
                for k in (0..operands).rev() {
                    let (cap, p) = string_operand_at(code, first + k * SOP_LEN);
                    let size = string_size(cap) as usize;
                    let start = resolve(p, size, base, &mut s)?;
                    range(start, size, memory.len()).ok_or(Fault::BadCode(at))?;
                    o[k] = Str { cap, at: start };
                }
                match x {
                    op::SMOVE => strings::assign(memory, o[0], o[1], at)?,
                    op::SCMP => {
                        let order = strings::compare(memory, o[0], o[1], at)?;
                        s.push(cmp(code[pc + 1], order, 0))?;
                    }
                    op::SLEN => s.push(of_i32(strings::length(memory, o[0], at)?))?,
                    op::SCAT => strings::concat(memory, o[0], o[1], o[2], at)?,
                    op::SLEFT | op::SRIGHT => {
                        strings::side(memory, o[0], o[1], n[0], x == op::SLEFT, at)?
                    }
                    op::SMID => strings::mid(memory, o[0], o[1], n[0], n[1], at)?,
                    op::SDELETE => strings::delete(memory, o[0], o[1], n[0], n[1], at)?,
                    op::SINSERT => strings::insert(memory, o[0], o[1], o[2], n[0], at)?,
                    op::SREPLACE => strings::replace(memory, o[0], o[1], o[2], n[0], n[1], at)?,
                    op::SFIND => s.push(of_i32(strings::find(memory, o[0], o[1], at)?))?,
                    op::ITOS => strings::from_int(memory, o[0], n[0], at)?,
                    op::STOI => s.push(strings::to_int(memory, o[0], at)? as u64)?,
                    _ => return Err(Fault::BadCode(at)),
                }
            }
            // Loads and stores relative to the base, or through a place.
            _ => {
                let Some((store, cell, reach)) = access(opcode) else {
                    return Err(Fault::BadCode(at));
                };
                let n = cell.size();
                let value = if store { s.pop()? } else { 0 };
                let start = match reach {
                    Reach::Relative => base.checked_add(operand()).ok_or(Fault::BadCode(at))?,
                    Reach::Place => resolve(place_at(code, pc + 1), n, base, &mut s)?,
                    Reach::Absolute => return Err(Fault::BadCode(at)),
                };
                let r = range(start, n, memory.len()).ok_or(Fault::BadCode(at))?;
                if store {
                    let value = if cell == Cell::Bool {
                        (value != 0) as u64
                    } else {
                        value
                    };
                    memory
                        .get_mut(r)
                        .ok_or(Fault::BadCode(at))?
                        .copy_from_slice(&value.to_le_bytes()[..n]);
                } else {
                    let bytes = memory.get(r).ok_or(Fault::BadCode(at))?;
                    s.push(fetch(cell, bytes))?;
                }
            }
        }
        pc = next;
    }
}
