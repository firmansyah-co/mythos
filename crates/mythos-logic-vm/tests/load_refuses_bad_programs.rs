//! A program the VM is given may have been damaged on the way, or made by something other than
//! our compiler. Every way it can be wrong is refused at load, before a scan.

use mythos_logic_vm::format::op;
use mythos_logic_vm::{cold_start, crc32, scan, Fault, LoadError, Program, FORMAT_VERSION, MAGIC};

/// A program from its parts, with a correct checksum.
fn build(version: u16, stack: u16, memory: &[u8], code: &[u8]) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(&MAGIC);
    b.extend_from_slice(&version.to_le_bytes());
    b.extend_from_slice(&stack.to_le_bytes());
    b.extend_from_slice(&(memory.len() as u32).to_le_bytes());
    b.extend_from_slice(&(code.len() as u32).to_le_bytes());
    b.extend_from_slice(memory);
    b.extend_from_slice(code);
    let crc = crc32(&b);
    b.extend_from_slice(&crc.to_le_bytes());
    b
}

fn with_addr(opcode: u8, addr: u32) -> Vec<u8> {
    let mut v = vec![opcode];
    v.extend_from_slice(&addr.to_le_bytes());
    v
}

#[test]
fn a_good_program_loads_and_runs() {
    // mem[0] := mem[1] + 1 (DINT at 0 and at 4 would overlap; use bytes 0..4 and 4..8)
    let mut code = with_addr(op::LD_I32, 4);
    code.extend(with_addr(op::PUSH32, 1));
    code.push(op::ADD_I32);
    code.extend(with_addr(op::ST_32, 0));
    code.push(op::END);
    let mut init = [0u8; 8];
    init[4..8].copy_from_slice(&41i32.to_le_bytes());
    let bytes = build(FORMAT_VERSION, 2, &init, &code);
    let p = Program::load(&bytes).unwrap();
    let mut mem = [0u8; 8];
    let mut stack = [0u64; 2];
    cold_start(&p, &mut mem).unwrap();
    scan(&p, &mut mem, &mut stack, 0, 100).unwrap();
    assert_eq!(i32::from_le_bytes(mem[0..4].try_into().unwrap()), 42);
}

#[test]
fn a_damaged_byte_is_found_by_the_checksum() {
    let mut bytes = build(FORMAT_VERSION, 1, &[0; 4], &[op::END]);
    bytes[16] ^= 1;
    assert_eq!(Program::load(&bytes).err(), Some(LoadError::Corrupt));
}

#[test]
fn another_format_version_is_refused_not_guessed_at() {
    let bytes = build(FORMAT_VERSION + 1, 1, &[], &[op::END]);
    assert_eq!(
        Program::load(&bytes).err(),
        Some(LoadError::UnsupportedVersion(FORMAT_VERSION + 1))
    );
}

#[test]
fn something_that_is_not_a_program_is_refused() {
    assert_eq!(
        Program::load(b"hello world, not a program").err(),
        Some(LoadError::NotAProgram)
    );
    assert_eq!(Program::load(b"MLBC").err(), Some(LoadError::WrongLength));
}

#[test]
fn a_load_or_store_outside_memory_is_refused_at_load() {
    for opcode in [op::LD_I32, op::ST_64, op::LD_BOOL] {
        let mut code = with_addr(opcode, 2);
        code.push(op::END);
        let bytes = build(FORMAT_VERSION, 1, &[0; 4], &code);
        let r = Program::load(&bytes).err();
        if opcode == op::LD_BOOL {
            assert_eq!(r, None, "one byte at 2 is inside four");
        } else {
            assert_eq!(r, Some(LoadError::OutOfMemory(0)), "opcode {opcode:#x}");
        }
    }
    let mut code = with_addr(op::LD_BOOL, u32::MAX);
    code.push(op::END);
    let bytes = build(FORMAT_VERSION, 1, &[0; 4], &code);
    assert_eq!(
        Program::load(&bytes).err(),
        Some(LoadError::OutOfMemory(0)),
        "no overflow past u32::MAX"
    );
}

#[test]
fn a_jump_out_of_the_code_or_an_unknown_opcode_is_refused() {
    let mut code = with_addr(op::JMP, 1000);
    code.push(op::END);
    assert_eq!(
        Program::load(&build(0, 1, &[], &code)).err(),
        Some(LoadError::BadJump(0))
    );
    assert_eq!(
        Program::load(&build(0, 1, &[], &[0xEE])).err(),
        Some(LoadError::UnknownInstruction(0))
    );
    assert_eq!(
        Program::load(&build(0, 1, &[], &[op::PUSH64, 1, 2])).err(),
        Some(LoadError::Truncated(0))
    );
}

#[test]
fn a_stack_smaller_than_the_program_uses_is_a_fault_not_a_write_past_it() {
    // Declares a depth of 1 but pushes 2.
    let mut code = with_addr(op::PUSH32, 1);
    code.extend(with_addr(op::PUSH32, 2));
    code.push(op::ADD_I32);
    code.push(op::END);
    let bytes = build(FORMAT_VERSION, 1, &[], &code);
    let p = Program::load(&bytes).unwrap();
    let mut stack = [0u64; 1];
    assert_eq!(
        scan(&p, &mut [], &mut stack, 0, 100),
        Err(Fault::StackOverflow(5))
    );
}

#[test]
fn running_off_the_end_of_the_code_is_a_fault() {
    let bytes = build(FORMAT_VERSION, 1, &[], &with_addr(op::PUSH32, 1));
    let p = Program::load(&bytes).unwrap();
    let mut stack = [0u64; 1];
    assert_eq!(
        scan(&p, &mut [], &mut stack, 0, 100),
        Err(Fault::BadCode(5))
    );
}
