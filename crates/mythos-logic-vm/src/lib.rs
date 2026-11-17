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
#![no_std]
#![forbid(unsafe_code)]

pub mod fb;
pub mod format;
mod vm;

pub use format::{crc32, ElemType, Limits, LoadError, Program, FORMAT_VERSION, MAGIC};
pub use vm::{cold_start, read, scan, write, Fault, Frame, ScanReport, Value};
