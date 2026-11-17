//! The bytecode format: what a compiled program is on the wire and in flash.
//!
//! ```text
//! offset  size  field
//! 0       4     magic "MLBC"
//! 4       2     format version (u16, little-endian)
//! 6       2     stack depth the program needs (u16)
//! 8       4     memory size in bytes (u32)
//! 12      4     code length in bytes (u32)
//! 16      2     call depth the program needs (u16)
//! 18      2     number of bodies, n (u16, at least 1)
//! 20      8n    the bodies: for each, where its code starts (u32) and the size of the
//!               instance it runs on (u32; 0 for a function and for the program)
//! 20+8n   m     initial memory image (m = memory size)
//! ..      c     code (c = code length)
//! ..      4     CRC-32 (IEEE) of every byte before it
//! ```
//!
//! Everything is little-endian on every target, so one file runs anywhere. Variables are packed
//! byte by byte with no alignment; the VM reads them with `from_le_bytes`, so a layout never
//! depends on the target.
//!
//! **Bodies.** The code is one body after another: each function, each function block type,
//! and last the program, where a scan starts. A body may call only bodies before it in the
//! table, so nothing a loaded program does is recursive, and every variable has one fixed place
//! in memory: there are no data frames.
//!
//! **Addressing.** A function block's body is emitted once and runs on whichever instance it
//! was called for: the *base*. An instruction reaches memory in one of three ways:
//!
//! - absolute (`LD_x addr`): the program's variables, a function's parameters and locals;
//! - relative (`LDR_x off`): `base + off`, a member of the instance the body runs on;
//! - through a *place* operand, `mode:u8 addr:u32 span:u32` (9 bytes), for what the short forms
//!   cannot say. Bit 0 of `mode` makes `addr` relative to the base; bit 1 adds an offset popped
//!   from the stack, which is how an array element is reached. `addr` and `span` name a region
//!   (the array's own bytes): the access must lie inside it, whatever the offset.
//!
//! An offset is made by `IDX`, which is what checks a subscript against its declared bounds.
//!
//! **Strings** (format 2). A `STRING[n]` is `2 + n` bytes: its length now (u16), then room
//! for `n` characters of one byte each. It never moves to the stack: an instruction on strings
//! names each string by a *string operand*, `cap:u16 place` (11 bytes), the place reaching
//! the `2 + cap` bytes of a string of length `cap`. A stored length greater than `cap` is
//! never followed: the scan stops with `Fault::BadString`. Integer arguments (a length, a
//! position) are i64 on the stack, above the offsets of any operands that take one; an
//! instruction pops its integers first, then the offsets from its last operand to its first.

use crate::fb::FbKind;

/// The first four bytes of every program.
pub const MAGIC: [u8; 4] = *b"MLBC";
/// The format this VM's compiler makes, and the newest this VM reads.
///
/// 3: the numeric functions of the standard library (`RFN`, `ATAN2_*`, `EXPT*`, `ABS_*`,
/// `CHKSEL`). 2: strings (`STRING[n]` and the instructions on them). 1: functions, function
/// blocks, arrays and structures.
///
/// A VM reads every format it extends (ADR-013, "Format versions"): each format from
/// [`OLDEST_FORMAT_VERSION`] to this one only adds instructions to the one before, and
/// changes no header field, no operand layout and no instruction's meaning, so a program
/// of an older version is read exactly as it was written. A format that changed what an
/// older program means would start a new line, and this VM would refuse what came before
/// it. A version outside the range is refused, never guessed at.
pub const FORMAT_VERSION: u16 = 3;
/// The oldest format this VM reads. Format 0, the spike's, had a 16-byte header without a
/// call depth or a table of bodies: this VM does not extend it.
pub const OLDEST_FORMAT_VERSION: u16 = 1;
const HEADER: usize = 20;
/// Bytes of one entry of the table of bodies.
const BODY: usize = 8;

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
    /// Return from a function or a function block body to its caller, whose base comes back.
    pub const RET: u8 = 0x03;
    /// `CALL body:u16`: run a function's body. The base is kept.
    pub const CALL: u8 = 0x04;
    /// `CALLFB body:u16 place`: run a function block's body on the instance at `place`, which
    /// becomes the base until it returns.
    pub const CALLFB: u8 = 0x05;
    /// `INIT addr:u32 len:u32`: put the initial values back over `len` bytes at `addr`
    /// (a function's locals, at each call).
    pub const INIT: u8 = 0x06;
    /// `MOVE len:u32 dst:place src:place`: copy `len` bytes (a structure, an array). When both
    /// places take an offset, the source's is on top of the stack.
    pub const MOVE: u8 = 0x07;
    /// `IDX lo:i32 count:u32 stride:u32`: pop a subscript (an i32); outside `lo .. lo+count-1`
    /// the scan stops with a fault; otherwise push the offset `(subscript - lo) * stride`.
    pub const IDX: u8 = 0x08;
    /// As `IDX`, for a subscript that is an i64 (LINT).
    pub const IDX64: u8 = 0x09;

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
    /// `FBP kind:u8 place`: run a standard function block on the instance at `place`.
    pub const FBP: u8 = 0xA1;

    // Relative loads and stores: `LDR_x off:u32`, at the base plus `off`. A 32-bit load serves
    // DINT and REAL alike, a 64-bit one LINT, LREAL and TIME.
    pub const LDR_BOOL: u8 = 0xB0;
    pub const LDR_I8: u8 = 0xB1;
    pub const LDR_I16: u8 = 0xB2;
    pub const LDR_32: u8 = 0xB3;
    pub const LDR_64: u8 = 0xB4;
    pub const STR_BOOL: u8 = 0xB8;
    pub const STR_8: u8 = 0xB9;
    pub const STR_16: u8 = 0xBA;
    pub const STR_32: u8 = 0xBB;
    pub const STR_64: u8 = 0xBC;

    // Loads and stores through a place: `LDP_x place`, `STP_x place`. A store pops the value,
    // then the place's offset if it takes one.
    pub const LDP_BOOL: u8 = 0xC0;
    pub const LDP_I8: u8 = 0xC1;
    pub const LDP_I16: u8 = 0xC2;
    pub const LDP_32: u8 = 0xC3;
    pub const LDP_64: u8 = 0xC4;
    pub const STP_BOOL: u8 = 0xC8;
    pub const STP_8: u8 = 0xC9;
    pub const STP_16: u8 = 0xCA;
    pub const STP_32: u8 = 0xCB;
    pub const STP_64: u8 = 0xCC;

    // Strings: each `s` below is a string operand, `cap:u16 place`. A string a function
    // gives is written to its `dst`; one longer than `dst`'s length stops the scan with
    // `StringTooLong`. IEC 61131-3 ed. 3 §6.6.2.5.11: an integer argument less than zero, or
    // one naming a character position the string does not have, stops it with
    // `StringArgument`.
    /// `SMOVE dst:s src:s`: assignment, cut to `dst`'s length (§6.6.1.2.2 leaves a longer
    /// source to the implementer).
    pub const SMOVE: u8 = 0xD0;
    /// `SCMP which:u8 a:s b:s`: push a BOOL, as `CMP_*` (0 EQ … 5 GE). The shorter string is
    /// taken as extended with characters of code zero; characters compare by their codes.
    pub const SCMP: u8 = 0xD1;
    /// `SLEN a:s`: push the length (an i32).
    pub const SLEN: u8 = 0xD2;
    /// `SCAT dst:s a:s b:s`: `a` then `b`.
    pub const SCAT: u8 = 0xD3;
    /// `SLEFT dst:s a:s`, pops L: the first L characters.
    pub const SLEFT: u8 = 0xD4;
    /// `SRIGHT dst:s a:s`, pops L: the last L characters.
    pub const SRIGHT: u8 = 0xD5;
    /// `SMID dst:s a:s`, pops P then L: L characters from position P (the first is 1).
    pub const SMID: u8 = 0xD6;
    /// `SDELETE dst:s a:s`, pops P then L: `a` without L characters from position P.
    pub const SDELETE: u8 = 0xD7;
    /// `SINSERT dst:s a:s b:s`, pops P: `b` put in `a` after its P-th character (0: first).
    pub const SINSERT: u8 = 0xD8;
    /// `SREPLACE dst:s a:s b:s`, pops P then L: L characters of `a` from position P made `b`.
    pub const SREPLACE: u8 = 0xD9;
    /// `SFIND a:s b:s`: push (an i32) the position where `b` first occurs in `a`, 0 if it
    /// does not.
    pub const SFIND: u8 = 0xDA;
    /// `ITOS dst:s`, pops an i64: its decimal text.
    pub const ITOS: u8 = 0xDB;
    /// `STOI a:s`: push (an i64) the integer `a` writes, in the form of an integer literal
    /// (§6.3.2): a sign, decimal digits, or `2#`, `8#`, `16#` and digits of that base, with
    /// single underscores between digits. Anything else, or a value beyond 64 bits, stops the
    /// scan with `BadNumber`.
    pub const STOI: u8 = 0xDC;

    // The numeric functions of IEC 61131-3 ed. 3 Table 28 and EXPT (Table 29), format 3. The
    // VM computes them itself (`crate::math`), the same bits on every target.
    /// `RFN which:u8`: replace the LREAL on top by its function `which` (`rfn`).
    pub const RFN: u8 = 0xE0;
    /// `RFN32 which:u8`: the same for a REAL, computed in LREAL and rounded once.
    pub const RFN32: u8 = 0xE1;
    /// Pop X, then Y; push ATAN2(Y, X).
    pub const ATAN2_F64: u8 = 0xE2;
    pub const ATAN2_F32: u8 = 0xE3;
    /// Pop the exponent (a real of the same type), then the base; push base ** exponent.
    pub const EXPT_F64: u8 = 0xE4;
    pub const EXPT_F32: u8 = 0xE5;
    /// Pop the exponent (an i64), then the base (a real); push base ** exponent.
    pub const EXPTI_F64: u8 = 0xE6;
    pub const EXPTI_F32: u8 = 0xE7;
    /// The absolute value; of the most negative integer it wraps to itself.
    pub const ABS_I32: u8 = 0xE8;
    pub const ABS_I64: u8 = 0xE9;
    pub const ABS_F32: u8 = 0xEA;
    pub const ABS_F64: u8 = 0xEB;
    /// `CHKSEL count:u32`: pop an i64; outside `0 .. count-1` the scan stops with
    /// `SelectorOutOfRange` (MUX's K, Table 32 footnote e).
    pub const CHKSEL: u8 = 0xEC;
}

/// The functions of one real that `RFN` and `RFN32` compute, by their `which`.
pub mod rfn {
    pub const SQRT: u8 = 0;
    pub const LN: u8 = 1;
    pub const LOG: u8 = 2;
    pub const EXP: u8 = 3;
    pub const SIN: u8 = 4;
    pub const COS: u8 = 5;
    pub const TAN: u8 = 6;
    pub const ASIN: u8 = 7;
    pub const ACOS: u8 = 8;
    pub const ATAN: u8 = 9;
    /// Toward zero, as a real (TRUNC is this, then a conversion of an integral value).
    pub const TRUNC: u8 = 10;
    pub const LAST: u8 = TRUNC;
}

/// Bytes of a string operand: `cap:u16 place`.
pub const SOP_LEN: usize = 2 + PLACE_LEN;
/// The bytes a string of length `cap` takes: its length, then its characters.
pub const fn string_size(cap: u16) -> u32 {
    2 + cap as u32
}

/// An instruction on strings: how many string operands follow its opcode (after `SCMP`'s
/// `which`), and how many integers it pops.
pub(crate) fn string_shape(opcode: u8) -> Option<(usize, usize)> {
    use op::*;
    Some(match opcode {
        SLEN | STOI => (1, 0),
        ITOS => (1, 1),
        SMOVE | SCMP | SFIND => (2, 0),
        SLEFT | SRIGHT => (2, 1),
        SMID | SDELETE => (2, 2),
        SCAT => (3, 0),
        SINSERT => (3, 1),
        SREPLACE => (3, 2),
        _ => return None,
    })
}

/// Bytes of a place operand: `mode:u8 addr:u32 span:u32`.
pub const PLACE_LEN: usize = 9;
/// Bit 0 of a place's mode: `addr` is relative to the base.
pub const PLACE_RELATIVE: u8 = 1;
/// Bit 1 of a place's mode: an offset popped from the stack is added.
pub const PLACE_INDEXED: u8 = 2;

/// A place operand, decoded.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Place {
    pub mode: u8,
    pub addr: u32,
    pub span: u32,
}

/// The string operand at `at`: the string's length and its place.
pub(crate) fn string_operand_at(code: &[u8], at: usize) -> (u16, Place) {
    (u16_at(code, at), place_at(code, at + 2))
}

/// The place operand at `at`. The caller has checked the instruction lies inside the code.
pub(crate) fn place_at(code: &[u8], at: usize) -> Place {
    Place {
        mode: code[at],
        addr: u32_at(code, at + 1),
        span: u32_at(code, at + 5),
    }
}

/// How a load or store reaches memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Reach {
    Absolute,
    Relative,
    Place,
}

/// What a load or store moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Cell {
    Bool,
    I8,
    I16,
    W32,
    W64,
}

impl Cell {
    pub(crate) const fn size(self) -> usize {
        match self {
            Cell::Bool | Cell::I8 => 1,
            Cell::I16 => 2,
            Cell::W32 => 4,
            Cell::W64 => 8,
        }
    }
}

/// A load or store instruction, taken apart: (is a store, what it moves, how it reaches it).
pub(crate) fn access(opcode: u8) -> Option<(bool, Cell, Reach)> {
    use op::*;
    const CELLS: [Cell; 5] = [Cell::Bool, Cell::I8, Cell::I16, Cell::W32, Cell::W64];
    Some(match opcode {
        LD_BOOL => (false, Cell::Bool, Reach::Absolute),
        LD_I8 => (false, Cell::I8, Reach::Absolute),
        LD_I16 => (false, Cell::I16, Reach::Absolute),
        LD_I32 | LD_F32 => (false, Cell::W32, Reach::Absolute),
        LD_I64 | LD_F64 => (false, Cell::W64, Reach::Absolute),
        ST_BOOL..=ST_64 => (true, CELLS[(opcode - ST_BOOL) as usize], Reach::Absolute),
        LDR_BOOL..=LDR_64 => (false, CELLS[(opcode - LDR_BOOL) as usize], Reach::Relative),
        STR_BOOL..=STR_64 => (true, CELLS[(opcode - STR_BOOL) as usize], Reach::Relative),
        LDP_BOOL..=LDP_64 => (false, CELLS[(opcode - LDP_BOOL) as usize], Reach::Place),
        STP_BOOL..=STP_64 => (true, CELLS[(opcode - STP_BOOL) as usize], Reach::Place),
        _ => return None,
    })
}

/// The format that brought `opcode` in, or `None` for an unknown opcode. A program of an
/// older format that holds it is refused: whatever wrote it did not write that format.
pub fn introduced_in(opcode: u8) -> Option<u16> {
    instruction_len(opcode)?;
    Some(match opcode {
        op::RFN..=op::CHKSEL => 3,
        x if string_shape(x).is_some() => 2,
        _ => 1,
    })
}

/// Bytes an instruction takes, opcode included, or `None` for an unknown opcode.
pub fn instruction_len(opcode: u8) -> Option<usize> {
    use op::*;
    Some(match opcode {
        END | RET | NOT | AND | OR | XOR => 1,
        JMP | JZ => 5,
        CALL => 3,
        CALLFB => 3 + PLACE_LEN,
        INIT => 9,
        MOVE => 5 + 2 * PLACE_LEN,
        IDX | IDX64 => 13,
        LDR_BOOL..=LDR_64 | STR_BOOL..=STR_64 => 5,
        LDP_BOOL..=LDP_64 | STP_BOOL..=STP_64 => 1 + PLACE_LEN,
        FBP => 2 + PLACE_LEN,
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
        RFN | RFN32 => 2,
        ATAN2_F64..=ABS_F64 => 1,
        CHKSEL => 5,
        SCMP => 2 + 2 * SOP_LEN,
        x if string_shape(x).is_some() => {
            1 + string_shape(x).map_or(0, |(operands, _)| operands) * SOP_LEN
        }
        _ => return None,
    })
}

/// Why a program was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadError {
    /// Shorter than its header says it is, or longer.
    WrongLength,
    NotAProgram,
    /// Written for a format version this VM does not read: one newer than
    /// [`FORMAT_VERSION`], or one older than [`OLDEST_FORMAT_VERSION`].
    UnsupportedVersion(u16),
    /// The checksum does not match: the bytes were damaged.
    Corrupt,
    /// An opcode this VM does not know, at this code offset.
    UnknownInstruction(u32),
    /// An instruction the format version the program declares does not have, at this code
    /// offset: a later format brought it in ([`introduced_in`]).
    NotInFormat(u32),
    /// An instruction whose operands run past the end of the code.
    Truncated(u32),
    /// A load, store, copy, function block instance or array region outside the program's
    /// memory, or, in a function block's body, outside the instance it runs on.
    OutOfMemory(u32),
    /// A jump outside the body it is in, or to a byte that is not where one of the body's
    /// instructions starts.
    BadJump(u32),
    /// A function block kind this VM does not have.
    UnknownFunctionBlock(u32),
    /// The table of bodies does not divide the code: no body, a start out of order or outside
    /// the code, or an instance larger than the memory.
    BadBodies,
    /// A call of a body that does not exist, or that is not before its caller in the table
    /// (which is what rules recursion out).
    BadCall(u32),
    /// An addressing mode this VM does not have.
    BadOperand(u32),
    /// A body whose last instruction lets execution run into the next one, an `END` outside
    /// the program's body, or a `RET` inside it.
    BadEnd(u32),
    /// The program declares this many bytes of memory, more than the host has ([`Limits`]).
    TooMuchMemory(u32),
    /// The program declares a stack this deep, deeper than the host's.
    StackTooDeep(u16),
    /// The program declares calls nested this deep, deeper than the host has frames for.
    CallsTooDeep(u16),
}

/// What a host has to run a program in. A PC runner gives each program what it declares; the
/// firmware of a microcontroller has arrays of a fixed size, and says here how large they are
/// so that a program too large for them is refused when it is loaded, not at its first scan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Bytes of memory.
    pub memory: usize,
    /// Slots of the evaluation stack.
    pub stack_depth: usize,
    /// Call frames.
    pub call_depth: usize,
}

/// A program checked and ready to run. It borrows the bytes it was loaded from.
#[derive(Debug, Clone, Copy)]
pub struct Program<'a> {
    pub(crate) init: &'a [u8],
    pub(crate) code: &'a [u8],
    /// The table of bodies, 8 bytes each.
    bodies: &'a [u8],
    version: u16,
    stack_depth: u16,
    call_depth: u16,
}

pub(crate) fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}
pub(crate) fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

impl<'a> Program<'a> {
    /// Check the bytes and every instruction in them. Nothing that passes can make the VM read
    /// or write outside the memory it is given.
    pub fn load(bytes: &'a [u8]) -> Result<Program<'a>, LoadError> {
        if bytes.len() < 4 {
            return Err(LoadError::WrongLength);
        }
        if bytes[0..4] != MAGIC {
            return Err(LoadError::NotAProgram);
        }
        if bytes.len() < 6 {
            return Err(LoadError::WrongLength);
        }
        // The version comes before anything else is believed: another format may lay its
        // header out differently. Every version this VM reads lays it out as format 1 does.
        let version = u16_at(bytes, 4);
        if !(OLDEST_FORMAT_VERSION..=FORMAT_VERSION).contains(&version) {
            return Err(LoadError::UnsupportedVersion(version));
        }
        if bytes.len() < HEADER + 4 {
            return Err(LoadError::WrongLength);
        }
        let stack_depth = u16_at(bytes, 6);
        let memory = u32_at(bytes, 8) as usize;
        let code_len = u32_at(bytes, 12) as usize;
        let call_depth = u16_at(bytes, 16);
        let table = u16_at(bytes, 18) as usize * BODY;
        let body = memory
            .checked_add(code_len)
            .and_then(|n| n.checked_add(table))
            .and_then(|n| n.checked_add(HEADER + 4));
        if body != Some(bytes.len()) {
            return Err(LoadError::WrongLength);
        }
        let crc_at = bytes.len() - 4;
        if crc32(&bytes[..crc_at]) != u32_at(bytes, crc_at) {
            return Err(LoadError::Corrupt);
        }
        let bodies = &bytes[HEADER..HEADER + table];
        let init = &bytes[HEADER + table..HEADER + table + memory];
        let code = &bytes[HEADER + table + memory..crc_at];
        verify(code, bodies, memory as u32, version)?;
        Ok(Program {
            init,
            code,
            bodies,
            version,
            stack_depth,
            call_depth,
        })
    }

    /// [`Program::load`], for a host with `limits`: a program that declares more memory, a
    /// deeper stack or deeper calls than the host has is refused, naming what it declares.
    pub fn load_within(bytes: &'a [u8], limits: &Limits) -> Result<Program<'a>, LoadError> {
        let program = Program::load(bytes)?;
        if program.memory_size() > limits.memory {
            return Err(LoadError::TooMuchMemory(program.init.len() as u32));
        }
        if program.stack_depth() > limits.stack_depth {
            return Err(LoadError::StackTooDeep(program.stack_depth));
        }
        if program.call_depth() > limits.call_depth {
            return Err(LoadError::CallsTooDeep(program.call_depth));
        }
        Ok(program)
    }

    /// The format version it was written in.
    pub fn format_version(&self) -> u16 {
        self.version
    }

    /// Bytes of memory the host must give it.
    pub fn memory_size(&self) -> usize {
        self.init.len()
    }

    /// Stack slots the host must give it. A scan uses that many and no more: a push beyond
    /// them is a fault, however many the host gave.
    pub fn stack_depth(&self) -> usize {
        self.stack_depth as usize
    }

    /// Call frames the host must give it: how deep its calls nest at most.
    pub fn call_depth(&self) -> usize {
        self.call_depth as usize
    }

    /// Bytes of code.
    pub fn code_size(&self) -> usize {
        self.code.len()
    }

    /// Where body `index` starts in the code and the size of the instance it runs on; `None`
    /// when there is no such body.
    pub(crate) fn body(&self, index: u16) -> Option<(u32, u32)> {
        let at = index as usize * BODY;
        let entry = self.bodies.get(at..at + BODY)?;
        Some((u32_at(entry, 0), u32_at(entry, 4)))
    }

    /// Where a scan starts: the last body, the program's.
    pub(crate) fn entry(&self) -> u32 {
        let last = (self.bodies.len() / BODY).saturating_sub(1);
        self.body(last as u16).map_or(0, |(start, _)| start)
    }
}

/// `addr .. addr + len` lies inside `limit` bytes.
fn inside(addr: u32, len: u32, limit: u32) -> bool {
    addr.checked_add(len).is_some_and(|end| end <= limit)
}

/// Walk the table of bodies and then every body: every opcode known and in the program's
/// format `version`, every operand inside the code, every address inside memory (or,
/// relative to the base, inside the instance the body runs on), every jump to the start of
/// an instruction of its own body, every call to an earlier body. (A call goes to the start of a body, which is where its first instruction
/// starts: each body is decoded from its own start.)
fn verify(code: &[u8], bodies: &[u8], memory: u32, version: u16) -> Result<(), LoadError> {
    let count = bodies.len() / BODY;
    if count == 0 {
        return Err(LoadError::BadBodies);
    }
    let start_of = |k: usize| u32_at(bodies, k * BODY) as usize;
    let size_of = |k: usize| u32_at(bodies, k * BODY + 4);
    for k in 0..count {
        let ordered = if k == 0 {
            start_of(0) == 0
        } else {
            start_of(k) > start_of(k - 1)
        };
        if !ordered || start_of(k) >= code.len() || size_of(k) > memory {
            return Err(LoadError::BadBodies);
        }
    }
    for k in 0..count {
        let end = if k + 1 < count {
            start_of(k + 1)
        } else {
            code.len()
        };
        verify_body(code, start_of(k)..end, k, count, &size_of, memory, version)?;
    }
    Ok(())
}

fn verify_body(
    code: &[u8],
    body: core::ops::Range<usize>,
    this: usize,
    count: usize,
    size_of: &dyn Fn(usize) -> u32,
    memory: u32,
    version: u16,
) -> Result<(), LoadError> {
    let (start, end) = (body.start, body.end);
    let is_program = this + 1 == count;
    let instance = size_of(this);
    // A place of `len` bytes: a known mode, its region inside the memory or the instance, and
    // room in the region for what is reached.
    let place = |p: Place, len: u32, at: u32| -> Result<(), LoadError> {
        if p.mode > (PLACE_RELATIVE | PLACE_INDEXED) {
            return Err(LoadError::BadOperand(at));
        }
        let limit = if p.mode & PLACE_RELATIVE != 0 {
            instance
        } else {
            memory
        };
        if !inside(p.addr, p.span, limit) || len > p.span {
            return Err(LoadError::OutOfMemory(at));
        }
        Ok(())
    };
    // A call must name a body before this one.
    let callee = |pc: usize, at: u32| -> Result<usize, LoadError> {
        let q = u16_at(code, pc + 1) as usize;
        if q >= this {
            return Err(LoadError::BadCall(at));
        }
        Ok(q)
    };
    let mut pc = start;
    let mut last = (start, op::NOT);
    while pc < end {
        let at = pc as u32;
        let opcode = code[pc];
        let len = instruction_len(opcode).ok_or(LoadError::UnknownInstruction(at))?;
        if introduced_in(opcode).is_some_and(|since| since > version) {
            return Err(LoadError::NotInFormat(at));
        }
        if pc + len > end {
            return Err(LoadError::Truncated(at));
        }
        if let Some((_, cell, reach)) = access(opcode) {
            let n = cell.size() as u32;
            match reach {
                Reach::Absolute | Reach::Relative => {
                    let limit = if reach == Reach::Relative {
                        instance
                    } else {
                        memory
                    };
                    if !inside(u32_at(code, pc + 1), n, limit) {
                        return Err(LoadError::OutOfMemory(at));
                    }
                }
                Reach::Place => place(place_at(code, pc + 1), n, at)?,
            }
        }
        match opcode {
            op::JMP | op::JZ => {
                let target = u32_at(code, pc + 1) as usize;
                if target < start || target >= end {
                    return Err(LoadError::BadJump(at));
                }
            }
            op::END if !is_program => return Err(LoadError::BadEnd(at)),
            op::RET if is_program => return Err(LoadError::BadEnd(at)),
            op::CALL => {
                callee(pc, at)?;
            }
            op::CALLFB => {
                let q = callee(pc, at)?;
                place(place_at(code, pc + 3), size_of(q), at)?;
            }
            op::INIT => {
                if !inside(u32_at(code, pc + 1), u32_at(code, pc + 5), memory) {
                    return Err(LoadError::OutOfMemory(at));
                }
            }
            op::MOVE => {
                let n = u32_at(code, pc + 1);
                place(place_at(code, pc + 5), n, at)?;
                place(place_at(code, pc + 5 + PLACE_LEN), n, at)?;
            }
            op::IDX | op::IDX64 => {
                // The largest offset it can give is inside the memory, so adding it to an
                // address cannot wrap.
                let reach = u32_at(code, pc + 5) as u64 * u32_at(code, pc + 9) as u64;
                if reach > memory as u64 {
                    return Err(LoadError::OutOfMemory(at));
                }
            }
            op::FB => {
                let kind =
                    FbKind::from_code(code[pc + 1]).ok_or(LoadError::UnknownFunctionBlock(at))?;
                if !inside(u32_at(code, pc + 2), kind.size(), memory) {
                    return Err(LoadError::OutOfMemory(at));
                }
            }
            op::FBP => {
                let kind =
                    FbKind::from_code(code[pc + 1]).ok_or(LoadError::UnknownFunctionBlock(at))?;
                place(place_at(code, pc + 2), kind.size(), at)?;
            }
            op::RFN | op::RFN32 if code[pc + 1] > rfn::LAST => {
                return Err(LoadError::BadOperand(at));
            }
            x if string_shape(x).is_some() => {
                let (operands, _) = string_shape(x).unwrap_or((0, 0));
                let mut from = pc + 1;
                if x == op::SCMP {
                    if code[from] > 5 {
                        return Err(LoadError::BadOperand(at));
                    }
                    from += 1;
                }
                for k in 0..operands {
                    let (cap, p) = string_operand_at(code, from + k * SOP_LEN);
                    place(p, string_size(cap), at)?;
                }
            }
            _ => {}
        }
        last = (pc, opcode);
        pc += len;
    }
    // Nothing may run off the end of a body into the next one's, where the base would be
    // another instance's.
    if !matches!(last.1, op::END | op::RET | op::JMP) {
        return Err(LoadError::BadEnd(last.0 as u32));
    }
    // A second walk, now that every instruction of the body is known to decode: a jump must
    // land where one of them starts. Landing inside one would run its operands as code.
    //
    // There is no memory to note every start in, so a target is proved by walking to it from
    // a place where an instruction is known to start. Such places are the start of the body,
    // the jump itself, and four targets already proved: the farthest one ahead (the END of
    // the CASE or IF being walked, which every arm jumps to), the last one ahead, the last
    // one behind (the head of the last loop), and the one behind before that (the head of
    // the loop before the nest of loops being walked). Whichever is nearest before the
    // target is where the walk starts; any of them would give the same answer.
    let (mut far, mut near, mut back, mut older) = (start, start, start, start);
    let mut pc = start;
    while pc < end {
        let opcode = code[pc];
        let len = instruction_len(opcode).ok_or(LoadError::UnknownInstruction(pc as u32))?;
        if matches!(opcode, op::JMP | op::JZ) {
            let target = u32_at(code, pc + 1) as usize;
            let mut from = start;
            for known in [pc, far, near, back, older] {
                if known <= target && known > from {
                    from = known;
                }
            }
            if !starts_instruction(code, from, target) {
                return Err(LoadError::BadJump(pc as u32));
            }
            if target <= pc {
                // The head of an inner loop is proved before that of the loop around it,
                // which lies before it: the head proved before the nest began stays known.
                if target > back {
                    older = back;
                }
                back = target;
            } else if far <= pc || target > far {
                far = target;
            } else {
                near = target;
            }
        }
        pc += len;
    }
    Ok(())
}

/// Whether an instruction starts at `target`, walking from `from`, where one does. The walk
/// must start at such a place: from anywhere else it could fall in step with the real
/// instructions by chance, and take the middle of one for a start.
fn starts_instruction(code: &[u8], from: usize, target: usize) -> bool {
    let mut pc = from;
    while pc < target {
        match code.get(pc).copied().and_then(instruction_len) {
            Some(len) => pc += len,
            None => return false,
        }
    }
    pc == target
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
