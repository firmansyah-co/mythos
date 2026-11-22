//! The MythOS logic VM (ADR-013).
//!
//! It runs a program compiled from IEC 61131-3 by `mythos-logic-compiler`. The same code runs
//! in the logic runner on a PC and in firmware on a microcontroller, so it has three rules:
//!
//! - `no_std`: only `core`. Clock, I/O and communication belong to the host that embeds it.
//! - It allocates nothing. The host gives it the program's bytes, a memory of the size the
//!   program declares, a stack of the depth it declares, and call frames for as deep as its
//!   calls nest. A program has no recursion, so every variable, parameter and function block
//!   instance has one fixed place in that memory.
//! - No `unsafe`. Every memory and stack access is checked; a program that would go outside
//!   them stops with a [`Fault`] instead.
//!
//! A scan is bounded: the host gives an instruction budget, and a program that does not finish
//! within it (an endless `WHILE`) stops with [`Fault::BudgetExceeded`] rather than hanging the
//! controller.
//!
//! A scan can also run in pieces: a [`Run`] is suspended after a bounded number of instructions
//! and resumed later, computing exactly what [`scan`] computes, so that one thread can preempt a
//! program of low priority with one of higher priority; [`sched`] decides which (ADR-013 §2.19).
#![no_std]
#![forbid(unsafe_code)]

pub mod fb;
pub mod format;
pub mod math;
pub mod sched;
mod strings;
mod vm;

pub use format::{
    crc32, string_size, ElemType, Limits, LoadError, Program, FORMAT_VERSION, MAGIC,
    OLDEST_FORMAT_VERSION,
};
pub use vm::{
    cold_start, read, read_string, scan, write, Fault, Frame, Piece, Run, ScanReport, Value,
};
