//! The bytecode format: what a compiled program is on the wire and in flash.
//!
//! ```text
//! offset  size  field
//! 0       4     magic "MLBC"
//! 4       2     format version (u16, little-endian)
//! 6       2     stack depth the program needs (u16)
//! 8       4     memory size in bytes (u32)
//! 12      4     code length in bytes (u32)
//! 16      m     initial memory image (m = memory size)
//! 16+m    c     code (c = code length)
//! 16+m+c  4     CRC-32 (IEEE) of every byte before it
//! ```
//!
//! Everything is little-endian on every target, so one file runs anywhere. Variables are packed
//! byte by byte with no alignment; the VM reads them with `from_le_bytes`, so a layout never
//! depends on the target.

use crate::fb::FbKind;

/// The first four bytes of every program.
pub const MAGIC: [u8; 4] = *b"MLBC";
/// The format this VM reads. A program of another version is refused, never guessed at.
pub const FORMAT_VERSION: u16 = 0;
const HEADER: usize = 16;

/// The type of a value in memory, as the VM sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElemType {
    Bool,
    Sint,
    Int,
    Dint,
    Lint,
    Real,
    Lreal,
    /// Nanoseconds, signed 64-bit.
    Time,
}

impl ElemType {
    /// Bytes it takes in memory.
    pub const fn size(self) -> u32 {
        match self {
            ElemType::Bool | ElemType::Sint => 1,
            ElemType::Int => 2,
            ElemType::Dint | ElemType::Real => 4,
            ElemType::Lint | ElemType::Lreal | ElemType::Time => 8,
        }
    }
}

/// Instruction codes. Operands follow the opcode, little-endian.
pub mod op {
    /// End of the scan.
    pub const END: u8 = 0x00;
    /// `JMP target:u32` (absolute offset in the code).
    pub const JMP: u8 = 0x01;
    /// `JZ target:u32`: pop a BOOL, jump when it is FALSE.
    pub const JZ: u8 = 0x02;

    // Loads: `LD_x addr:u32`, push the value at `addr`.
    pub const LD_BOOL: u8 = 0x10;
    pub const LD_I8: u8 = 0x11;
    pub const LD_I16: u8 = 0x13;
    pub const LD_I32: u8 = 0x15;
    pub const LD_I64: u8 = 0x17;
    pub const LD_F32: u8 = 0x18;
    pub const LD_F64: u8 = 0x19;

    // Stores: `ST_x addr:u32`, pop and write the low bytes at `addr`.
    pub const ST_BOOL: u8 = 0x20;
    pub const ST_8: u8 = 0x21;
    pub const ST_16: u8 = 0x22;
    pub const ST_32: u8 = 0x23;
    pub const ST_64: u8 = 0x24;

    /// `PUSH32 v:u32`
    pub const PUSH32: u8 = 0x30;
    /// `PUSH64 v:u64`
    pub const PUSH64: u8 = 0x31;

    // Arithmetic on two popped values, result pushed. Integers wrap.
    pub const ADD_I32: u8 = 0x40;
    pub const SUB_I32: u8 = 0x41;
    pub const MUL_I32: u8 = 0x42;
    pub const DIV_I32: u8 = 0x43;
    pub const MOD_I32: u8 = 0x44;
    pub const NEG_I32: u8 = 0x45;
    pub const ADD_I64: u8 = 0x48;
    pub const SUB_I64: u8 = 0x49;
    pub const MUL_I64: u8 = 0x4A;
    pub const DIV_I64: u8 = 0x4B;
    pub const MOD_I64: u8 = 0x4C;
    pub const NEG_I64: u8 = 0x4D;
    pub const ADD_F32: u8 = 0x50;
    pub const SUB_F32: u8 = 0x51;
    pub const MUL_F32: u8 = 0x52;
    pub const DIV_F32: u8 = 0x53;
    pub const NEG_F32: u8 = 0x55;
    pub const ADD_F64: u8 = 0x58;
    pub const SUB_F64: u8 = 0x59;
    pub const MUL_F64: u8 = 0x5A;
    pub const DIV_F64: u8 = 0x5B;
    pub const NEG_F64: u8 = 0x5D;

    // Comparisons: pop two, push a BOOL. Base + 0 EQ, 1 NE, 2 LT, 3 LE, 4 GT, 5 GE.
    pub const CMP_I32: u8 = 0x60;
    pub const CMP_I64: u8 = 0x68;
    pub const CMP_F32: u8 = 0x70;
    pub const CMP_F64: u8 = 0x78;

    pub const AND: u8 = 0x80;
    pub const OR: u8 = 0x81;
    pub const XOR: u8 = 0x82;
    pub const NOT: u8 = 0x83;

    // Conversions of the top of the stack.
    pub const I32_TO_I64: u8 = 0x90;
    /// Keep the low 32 bits, as a signed value.
    pub const I64_TO_I32: u8 = 0x91;
    pub const I32_TO_F32: u8 = 0x92;
    pub const I32_TO_F64: u8 = 0x93;
    pub const I64_TO_F64: u8 = 0x94;
    pub const F32_TO_F64: u8 = 0x95;
    pub const F64_TO_F32: u8 = 0x96;
    /// Round to nearest, a half to the nearest even integer; saturates at the type's bounds.
    pub const F32_TO_I32: u8 = 0x97;
    pub const F64_TO_I64: u8 = 0x98;
    pub const F64_TO_I32: u8 = 0x99;
    pub const I64_TO_F32: u8 = 0x9A;
    pub const F32_TO_I64: u8 = 0x9B;
    /// Wrap an i32 to the signed 16-bit range (INT arithmetic).
    pub const WRAP_I16: u8 = 0x9C;
    /// Wrap an i32 to the signed 8-bit range (SINT arithmetic).
    pub const WRAP_I8: u8 = 0x9D;
    /// Any non-zero integer to TRUE.
    pub const I64_TO_BOOL: u8 = 0x9E;

    /// `FB kind:u8 base:u32`: run a standard function block on the instance at `base`.
    pub const FB: u8 = 0xA0;
}

/// Bytes an instruction takes, opcode included, or `None` for an unknown opcode.
pub fn instruction_len(opcode: u8) -> Option<usize> {
    use op::*;
    Some(match opcode {
        END | NOT | AND | OR | XOR => 1,
        JMP | JZ => 5,
        LD_BOOL | LD_I8 | LD_I16 | LD_I32 | LD_I64 | LD_F32 | LD_F64 => 5,
        ST_BOOL | ST_8 | ST_16 | ST_32 | ST_64 => 5,
        PUSH32 => 5,
        PUSH64 => 9,
        ADD_I32..=NEG_I32 | ADD_I64..=NEG_I64 => 1,
        ADD_F32..=DIV_F32 | NEG_F32 | ADD_F64..=DIV_F64 | NEG_F64 => 1,
        x if (CMP_I32..CMP_I32 + 6).contains(&x)
            || (CMP_I64..CMP_I64 + 6).contains(&x)
            || (CMP_F32..CMP_F32 + 6).contains(&x)
            || (CMP_F64..CMP_F64 + 6).contains(&x) =>
        {
            1
        }
        I32_TO_I64..=I64_TO_BOOL => 1,
        FB => 6,
        _ => return None,
    })
}

/// Bytes a load or store at `addr` touches, for checking it at load time.
fn access_len(opcode: u8) -> Option<u32> {
    use op::*;
    Some(match opcode {
        LD_BOOL | LD_I8 | ST_BOOL | ST_8 => 1,
        LD_I16 | ST_16 => 2,
        LD_I32 | LD_F32 | ST_32 => 4,
        LD_I64 | LD_F64 | ST_64 => 8,
        _ => return None,
    })
}

/// Why a program was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadError {
    /// Shorter than its header says it is, or longer.
    WrongLength,
    NotAProgram,
    /// Written for another format version.
    UnsupportedVersion(u16),
    /// The checksum does not match: the bytes were damaged.
    Corrupt,
    /// An opcode this VM does not know, at this code offset.
    UnknownInstruction(u32),
    /// An instruction whose operands run past the end of the code.
    Truncated(u32),
    /// A load, store or function block outside the program's memory.
    OutOfMemory(u32),
    /// A jump outside the code.
    BadJump(u32),
    /// A function block kind this VM does not have.
    UnknownFunctionBlock(u32),
}

/// A program checked and ready to run. It borrows the bytes it was loaded from.
#[derive(Debug, Clone, Copy)]
pub struct Program<'a> {
    pub(crate) init: &'a [u8],
    pub(crate) code: &'a [u8],
    stack_depth: u16,
}

fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}
pub(crate) fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

impl<'a> Program<'a> {
    /// Check the bytes and every instruction in them. Nothing that passes can make the VM read
    /// or write outside the memory it is given.
    pub fn load(bytes: &'a [u8]) -> Result<Program<'a>, LoadError> {
        if bytes.len() < HEADER + 4 {
            return Err(LoadError::WrongLength);
        }
        if bytes[0..4] != MAGIC {
            return Err(LoadError::NotAProgram);
        }
        let version = u16_at(bytes, 4);
        if version != FORMAT_VERSION {
            return Err(LoadError::UnsupportedVersion(version));
        }
        let stack_depth = u16_at(bytes, 6);
        let memory = u32_at(bytes, 8) as usize;
        let code_len = u32_at(bytes, 12) as usize;
        let body = memory
            .checked_add(code_len)
            .and_then(|n| n.checked_add(HEADER + 4));
        if body != Some(bytes.len()) {
            return Err(LoadError::WrongLength);
        }
        let crc_at = bytes.len() - 4;
        if crc32(&bytes[..crc_at]) != u32_at(bytes, crc_at) {
            return Err(LoadError::Corrupt);
        }
        let init = &bytes[HEADER..HEADER + memory];
        let code = &bytes[HEADER + memory..crc_at];
        verify(code, memory as u32)?;
        Ok(Program {
            init,
            code,
            stack_depth,
        })
    }

    /// Bytes of memory the host must give it.
    pub fn memory_size(&self) -> usize {
        self.init.len()
    }

    /// Stack slots the host must give it.
    pub fn stack_depth(&self) -> usize {
        self.stack_depth as usize
    }

    /// Bytes of code.
    pub fn code_size(&self) -> usize {
        self.code.len()
    }
}

/// Walk the code once: every opcode known, every operand inside the code, every address inside
/// memory, every jump inside the code.
fn verify(code: &[u8], memory: u32) -> Result<(), LoadError> {
    let mut pc = 0usize;
    while pc < code.len() {
        let at = pc as u32;
        let opcode = code[pc];
        let len = instruction_len(opcode).ok_or(LoadError::UnknownInstruction(at))?;
        if pc + len > code.len() {
            return Err(LoadError::Truncated(at));
        }
        if let Some(n) = access_len(opcode) {
            let addr = u32_at(code, pc + 1);
            if addr.checked_add(n).is_none_or(|end| end > memory) {
                return Err(LoadError::OutOfMemory(at));
            }
        }
        if opcode == op::JMP || opcode == op::JZ {
            let target = u32_at(code, pc + 1) as usize;
            if target > code.len() {
                return Err(LoadError::BadJump(at));
            }
        }
        if opcode == op::FB {
            let kind =
                FbKind::from_code(code[pc + 1]).ok_or(LoadError::UnknownFunctionBlock(at))?;
            let base = u32_at(code, pc + 2);
            if base.checked_add(kind.size()).is_none_or(|end| end > memory) {
                return Err(LoadError::OutOfMemory(at));
            }
        }
        pc += len;
    }
    Ok(())
}

/// CRC-32 (IEEE 802.3, the zlib one), bit by bit: no table, so nothing extra in flash.
pub fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in bytes {
        crc ^= b as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_checksum_is_the_standard_crc32() {
        // The check value every CRC-32/IEEE implementation must give.
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }
}
