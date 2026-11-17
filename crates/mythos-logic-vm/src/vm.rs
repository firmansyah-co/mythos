//! Running a scan.

use crate::fb::{self, FbKind};
use crate::format::{op, u32_at, ElemType, Program};

/// Why a scan stopped before its end. The host then treats the program as stopped: its outputs
/// go to their safe values (ADR-013).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fault {
    /// The memory or stack given is smaller than the program declares.
    WrongSizes,
    /// More instructions than the budget: most likely a loop that never ends.
    BudgetExceeded,
    /// Integer division by zero, at this code offset. (`MOD` by zero is 0.)
    DivideByZero(u32),
    StackOverflow(u32),
    StackUnderflow(u32),
    /// Execution reached a byte that is not an instruction, or ran off the end of the code.
    BadCode(u32),
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

/// Run one scan of `program` over `memory` at time `now` (nanoseconds of a monotonic clock),
/// executing at most `budget` instructions.
pub fn scan(
    program: &Program,
    memory: &mut [u8],
    stack: &mut [u64],
    now: i64,
    budget: u32,
) -> Result<ScanReport, Fault> {
    if memory.len() != program.memory_size() || stack.len() < program.stack_depth() {
        return Err(Fault::WrongSizes);
    }
    let code = program.code;
    let mut s = Stack {
        slots: stack,
        sp: 0,
        pc: 0,
    };
    let mut pc = 0usize;
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
            let a = addr as usize;
            memory.get(a..a + n).ok_or(Fault::BadCode(at))
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
                let a = operand() as usize;
                let dst = memory.get_mut(a..a + n).ok_or(Fault::BadCode(at))?;
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
            op::FB => {
                let kind = FbKind::from_code(code[pc + 1]).ok_or(Fault::BadCode(at))?;
                let base = u32_at(code, pc + 2) as usize;
                let n = kind.size() as usize;
                let x = memory.get_mut(base..base + n).ok_or(Fault::BadCode(at))?;
                fb::run(kind, x, now);
            }
            _ => return Err(Fault::BadCode(at)),
        }
        pc = next;
    }
}
