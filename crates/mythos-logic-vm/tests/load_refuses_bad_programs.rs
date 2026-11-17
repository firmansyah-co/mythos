//! A program the VM is given may have been damaged on the way, or made by something other than
//! our compiler. Every way it can be wrong is refused at load, before a scan.

use mythos_logic_vm::format::{op, PLACE_INDEXED, PLACE_RELATIVE};
use mythos_logic_vm::{
    cold_start, crc32, scan, Fault, Frame, Limits, LoadError, Program, FORMAT_VERSION, MAGIC,
};

/// A program from its parts, with a correct checksum. `bodies` are each body's code and the
/// size of the instance it runs on, the program's last.
fn program(
    version: u16,
    stack: u16,
    calls: u16,
    memory: &[u8],
    bodies: &[(&[u8], u32)],
) -> Vec<u8> {
    let code_len: usize = bodies.iter().map(|(c, _)| c.len()).sum();
    let mut b = Vec::new();
    b.extend_from_slice(&MAGIC);
    b.extend_from_slice(&version.to_le_bytes());
    b.extend_from_slice(&stack.to_le_bytes());
    b.extend_from_slice(&(memory.len() as u32).to_le_bytes());
    b.extend_from_slice(&(code_len as u32).to_le_bytes());
    b.extend_from_slice(&calls.to_le_bytes());
    b.extend_from_slice(&(bodies.len() as u16).to_le_bytes());
    let mut start = 0u32;
    for (code, instance) in bodies {
        b.extend_from_slice(&start.to_le_bytes());
        b.extend_from_slice(&instance.to_le_bytes());
        start += code.len() as u32;
    }
    b.extend_from_slice(memory);
    for (code, _) in bodies {
        b.extend_from_slice(code);
    }
    let crc = crc32(&b);
    b.extend_from_slice(&crc.to_le_bytes());
    b
}

/// A program of one body, the program's own.
fn build(version: u16, stack: u16, memory: &[u8], code: &[u8]) -> Vec<u8> {
    program(version, stack, 0, memory, &[(code, 0)])
}

/// The same, with the table of bodies written by hand: (start, instance size) each.
fn with_table(memory: &[u8], code: &[u8], table: &[(u32, u32)]) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(&MAGIC);
    b.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&(memory.len() as u32).to_le_bytes());
    b.extend_from_slice(&(code.len() as u32).to_le_bytes());
    b.extend_from_slice(&0u16.to_le_bytes());
    b.extend_from_slice(&(table.len() as u16).to_le_bytes());
    for (start, instance) in table {
        b.extend_from_slice(&start.to_le_bytes());
        b.extend_from_slice(&instance.to_le_bytes());
    }
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

const ABSOLUTE: u8 = 0;
const RELATIVE: u8 = PLACE_RELATIVE;
const INDEXED: u8 = PLACE_INDEXED;

/// A place operand.
fn place(mode: u8, addr: u32, span: u32) -> Vec<u8> {
    let mut v = vec![mode];
    v.extend_from_slice(&addr.to_le_bytes());
    v.extend_from_slice(&span.to_le_bytes());
    v
}

/// An instruction from its opcode and the bytes of its operands.
fn ins(opcode: u8, operands: &[&[u8]]) -> Vec<u8> {
    let mut v = vec![opcode];
    for o in operands {
        v.extend_from_slice(o);
    }
    v
}

fn idx(opcode: u8, lo: i32, count: u32, stride: u32) -> Vec<u8> {
    ins(
        opcode,
        &[
            &lo.to_le_bytes(),
            &count.to_le_bytes(),
            &stride.to_le_bytes(),
        ],
    )
}

fn call(body: u16) -> Vec<u8> {
    ins(op::CALL, &[&body.to_le_bytes()])
}

fn call_fb(body: u16, at: &[u8]) -> Vec<u8> {
    ins(op::CALLFB, &[&body.to_le_bytes(), at])
}

/// Load it, give it the memory and working space it declares, and run one scan.
fn run(bytes: &[u8]) -> (Result<(), Fault>, Vec<u8>) {
    let p = Program::load(bytes).expect("it loads");
    let mut mem = vec![0u8; p.memory_size()];
    let mut stack = vec![0u64; p.stack_depth()];
    let mut calls = vec![Frame::default(); p.call_depth()];
    cold_start(&p, &mut mem).unwrap();
    let r = scan(&p, &mut mem, &mut stack, &mut calls, 0, 1000).map(|_| ());
    (r, mem)
}

fn refused(bytes: &[u8]) -> Option<LoadError> {
    Program::load(bytes).err()
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
    scan(&p, &mut mem, &mut stack, &mut [], 0, 100).unwrap();
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
        Program::load(&build(FORMAT_VERSION, 1, &[], &code)).err(),
        Some(LoadError::BadJump(0))
    );
    assert_eq!(
        Program::load(&build(FORMAT_VERSION, 1, &[], &[0xEE])).err(),
        Some(LoadError::UnknownInstruction(0))
    );
    assert_eq!(
        Program::load(&build(FORMAT_VERSION, 1, &[], &[op::PUSH64, 1, 2])).err(),
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
        scan(&p, &mut [], &mut stack, &mut [], 0, 100),
        Err(Fault::StackOverflow(5))
    );
}

#[test]
fn a_program_of_the_format_before_this_one_is_refused_by_its_version() {
    // Format 0 had a 16-byte header and no table of bodies. Its bytes must not be read as
    // format 1, whatever they hold.
    let mut b = Vec::new();
    b.extend_from_slice(&MAGIC);
    b.extend_from_slice(&0u16.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&0u32.to_le_bytes());
    b.extend_from_slice(&1u32.to_le_bytes());
    b.push(op::END);
    let crc = crc32(&b);
    b.extend_from_slice(&crc.to_le_bytes());
    assert_eq!(refused(&b), Some(LoadError::UnsupportedVersion(0)));
}

#[test]
fn a_body_must_end_by_leaving_and_only_the_program_ends_the_scan() {
    // Nothing after the last instruction: execution would run off the end.
    let bytes = build(FORMAT_VERSION, 1, &[], &with_addr(op::PUSH32, 1));
    assert_eq!(refused(&bytes), Some(LoadError::BadEnd(0)));

    // A function's body that does not return would run on into the program's.
    let bytes = program(
        FORMAT_VERSION,
        1,
        1,
        &[],
        &[(&[op::NOT], 0), (&[op::END], 0)],
    );
    assert_eq!(refused(&bytes), Some(LoadError::BadEnd(0)));

    // END inside a function would end the scan in the middle of a call.
    let bytes = program(
        FORMAT_VERSION,
        1,
        1,
        &[],
        &[(&[op::END], 0), (&[op::END], 0)],
    );
    assert_eq!(refused(&bytes), Some(LoadError::BadEnd(0)));

    // RET in the program has nowhere to return to.
    let bytes = build(FORMAT_VERSION, 1, &[], &[op::RET]);
    assert_eq!(refused(&bytes), Some(LoadError::BadEnd(0)));

    // And what is allowed loads: a function that returns, a program that ends.
    let bytes = program(
        FORMAT_VERSION,
        1,
        1,
        &[],
        &[(&[op::RET], 0), (&[op::END], 0)],
    );
    assert_eq!(refused(&bytes), None);
}

#[test]
fn a_jump_into_another_body_is_refused() {
    // The function's code is bytes 0..1, the program's 1..7. A jump from the program to 0
    // would run the function's body with nothing to return to.
    let mut main = with_addr(op::JMP, 0);
    main.push(op::END);
    let bytes = program(FORMAT_VERSION, 1, 1, &[], &[(&[op::RET], 0), (&main, 0)]);
    assert_eq!(refused(&bytes), Some(LoadError::BadJump(1)));

    // To its own end is refused too: there is no instruction there.
    let mut main = with_addr(op::JMP, 6);
    main.push(op::END);
    let bytes = build(FORMAT_VERSION, 1, &[], &main);
    assert_eq!(refused(&bytes), Some(LoadError::BadJump(0)));
}

#[test]
fn the_table_of_bodies_must_divide_the_code_in_order() {
    let code = [op::RET, op::END];
    assert_eq!(
        refused(&with_table(&[0; 4], &code, &[(0, 0), (1, 0)])),
        None
    );
    for (table, why) in [
        (&[][..], "no body at all"),
        (&[(1, 0)][..], "the first body does not start the code"),
        (&[(0, 0), (0, 0)][..], "two bodies at one place"),
        (
            &[(0, 0), (2, 0)][..],
            "a body that starts where the code ends",
        ),
        (&[(0, 0), (9, 0)][..], "a body that starts outside the code"),
        (&[(0, 5), (1, 0)][..], "an instance larger than the memory"),
    ] {
        assert_eq!(
            refused(&with_table(&[0; 4], &code, table)),
            Some(LoadError::BadBodies),
            "{why}"
        );
    }
}

#[test]
fn a_relative_load_or_store_outside_the_instance_is_refused() {
    // A function block whose instance is 4 bytes, in a memory of 16.
    let block = |code: Vec<u8>| {
        let mut body = code;
        body.push(op::RET);
        program(
            FORMAT_VERSION,
            1,
            1,
            &[0; 16],
            &[(&body, 4), (&[op::END], 0)],
        )
    };
    assert_eq!(refused(&block(with_addr(op::LDR_32, 0))), None);
    assert_eq!(refused(&block(with_addr(op::STR_16, 2))), None);
    for (opcode, offset) in [
        (op::LDR_32, 1),
        (op::LDR_64, 0),
        (op::LDR_BOOL, 4),
        (op::LDR_I8, 4),
        (op::LDR_I16, 3),
        (op::STR_BOOL, 4),
        (op::STR_8, 4),
        (op::STR_16, 3),
        (op::STR_32, 1),
        (op::STR_64, 0),
        (op::LDR_BOOL, u32::MAX),
    ] {
        assert_eq!(
            refused(&block(with_addr(opcode, offset))),
            Some(LoadError::OutOfMemory(0)),
            "opcode {opcode:#x} at {offset}: inside the memory, outside the instance"
        );
    }
    // The program runs on no instance: nothing is relative there.
    let mut main = with_addr(op::LDR_BOOL, 0);
    main.push(op::END);
    assert_eq!(
        refused(&build(FORMAT_VERSION, 1, &[0; 16], &main)),
        Some(LoadError::OutOfMemory(0))
    );
}

#[test]
fn a_load_or_store_through_a_place_must_keep_its_region_inside_the_memory_or_the_instance() {
    let main = |at: Vec<u8>, opcode: u8| {
        let mut code = ins(opcode, &[&at]);
        code.push(op::END);
        build(FORMAT_VERSION, 1, &[0; 16], &code)
    };
    assert_eq!(refused(&main(place(INDEXED, 8, 8), op::LDP_32)), None);
    for opcode in [
        op::LDP_BOOL,
        op::LDP_I8,
        op::LDP_I16,
        op::LDP_32,
        op::LDP_64,
        op::STP_BOOL,
        op::STP_8,
        op::STP_16,
        op::STP_32,
        op::STP_64,
    ] {
        assert_eq!(
            refused(&main(place(INDEXED, 8, 9), opcode)),
            Some(LoadError::OutOfMemory(0)),
            "opcode {opcode:#x}: a region of 9 bytes at 8 leaves a memory of 16"
        );
        assert_eq!(
            refused(&main(place(INDEXED, u32::MAX, 2), opcode)),
            Some(LoadError::OutOfMemory(0)),
            "opcode {opcode:#x}: no overflow past u32::MAX"
        );
        assert_eq!(
            refused(&main(place(4, 0, 8), opcode)),
            Some(LoadError::BadOperand(0)),
            "opcode {opcode:#x}: there is no addressing mode 4"
        );
    }
    assert_eq!(
        refused(&main(place(INDEXED, 8, 3), op::LDP_32)),
        Some(LoadError::OutOfMemory(0)),
        "four bytes do not fit in a region of three"
    );
    // Relative to the base, the region must lie inside the instance, not merely the memory.
    let block = |at: Vec<u8>| {
        let mut body = ins(op::LDP_I16, &[&at]);
        body.push(op::RET);
        program(
            FORMAT_VERSION,
            1,
            1,
            &[0; 16],
            &[(&body, 6), (&[op::END], 0)],
        )
    };
    assert_eq!(refused(&block(place(RELATIVE | INDEXED, 0, 6))), None);
    assert_eq!(
        refused(&block(place(RELATIVE | INDEXED, 0, 8))),
        Some(LoadError::OutOfMemory(0))
    );
}

#[test]
fn a_call_must_name_a_body_before_its_caller_so_nothing_loaded_is_recursive() {
    let two = |first: Vec<u8>, main: Vec<u8>| {
        let (mut f, mut m) = (first, main);
        f.push(op::RET);
        m.push(op::END);
        program(FORMAT_VERSION, 1, 4, &[0; 8], &[(&f, 0), (&m, 0)])
    };
    assert_eq!(refused(&two(vec![], call(0))), None);
    assert_eq!(
        refused(&two(call(0), vec![])),
        Some(LoadError::BadCall(0)),
        "a body that calls itself"
    );
    assert_eq!(
        refused(&two(call(1), vec![])),
        Some(LoadError::BadCall(0)),
        "a body that calls a later one, which could call it back"
    );
    assert_eq!(
        refused(&two(vec![], call(7))),
        Some(LoadError::BadCall(1)),
        "a body that does not exist"
    );
    assert_eq!(
        refused(&two(vec![], call(1))),
        Some(LoadError::BadCall(1)),
        "the program calling itself"
    );
    // The same of a function block call.
    let at = place(ABSOLUTE, 0, 0);
    assert_eq!(refused(&two(vec![], call_fb(0, &at))), None);
    assert_eq!(
        refused(&two(call_fb(0, &at), vec![])),
        Some(LoadError::BadCall(0))
    );
    assert_eq!(
        refused(&two(vec![], call_fb(1, &at))),
        Some(LoadError::BadCall(1))
    );
}

#[test]
fn a_function_block_call_whose_instance_does_not_fit_its_place_is_refused() {
    // A block of 4 bytes, called by the program in a memory of 8.
    let main = |at: Vec<u8>| {
        let mut m = call_fb(0, &at);
        m.push(op::END);
        program(FORMAT_VERSION, 1, 1, &[0; 8], &[(&[op::RET], 4), (&m, 0)])
    };
    assert_eq!(refused(&main(place(ABSOLUTE, 4, 4))), None);
    assert_eq!(
        refused(&main(place(ABSOLUTE, 5, 4))),
        Some(LoadError::OutOfMemory(1)),
        "the instance would end past the memory"
    );
    assert_eq!(
        refused(&main(place(ABSOLUTE, 4, 3))),
        Some(LoadError::OutOfMemory(1)),
        "the place is smaller than the instance"
    );
    assert_eq!(
        refused(&main(place(INDEXED, 0, 3))),
        Some(LoadError::OutOfMemory(1)),
        "an array region smaller than one instance"
    );
    assert_eq!(
        refused(&main(place(9, 0, 4))),
        Some(LoadError::BadOperand(1))
    );
    // An inner instance must lie inside the outer one: outer is 6 bytes, inner 4.
    let outer = |at: Vec<u8>| {
        let mut o = call_fb(0, &at);
        o.push(op::RET);
        program(
            FORMAT_VERSION,
            1,
            2,
            &[0; 16],
            &[(&[op::RET], 4), (&o, 6), (&[op::END], 0)],
        )
    };
    assert_eq!(refused(&outer(place(RELATIVE, 2, 4))), None);
    assert_eq!(
        refused(&outer(place(RELATIVE, 3, 4))),
        Some(LoadError::OutOfMemory(1)),
        "inside the memory, but past the end of the outer instance"
    );
}

#[test]
fn an_init_or_a_copy_outside_the_memory_is_refused() {
    let main = |code: Vec<u8>| {
        let mut m = code;
        m.push(op::END);
        build(FORMAT_VERSION, 1, &[0; 8], &m)
    };
    let init = |addr: u32, len: u32| ins(op::INIT, &[&addr.to_le_bytes(), &len.to_le_bytes()]);
    assert_eq!(refused(&main(init(4, 4))), None);
    assert_eq!(refused(&main(init(4, 5))), Some(LoadError::OutOfMemory(0)));
    assert_eq!(
        refused(&main(init(u32::MAX, 2))),
        Some(LoadError::OutOfMemory(0))
    );

    let copy =
        |len: u32, dst: Vec<u8>, src: Vec<u8>| ins(op::MOVE, &[&len.to_le_bytes(), &dst, &src]);
    let (lo, hi) = (place(ABSOLUTE, 0, 4), place(ABSOLUTE, 4, 4));
    assert_eq!(refused(&main(copy(4, lo.clone(), hi.clone()))), None);
    assert_eq!(
        refused(&main(copy(4, place(ABSOLUTE, 5, 4), lo.clone()))),
        Some(LoadError::OutOfMemory(0)),
        "the destination ends past the memory"
    );
    assert_eq!(
        refused(&main(copy(4, lo.clone(), place(ABSOLUTE, 5, 4)))),
        Some(LoadError::OutOfMemory(0)),
        "the source ends past the memory"
    );
    assert_eq!(
        refused(&main(copy(5, lo.clone(), hi.clone()))),
        Some(LoadError::OutOfMemory(0)),
        "five bytes do not fit in places of four"
    );
    assert_eq!(
        refused(&main(copy(4, lo, place(6, 4, 4)))),
        Some(LoadError::BadOperand(0))
    );
}

#[test]
fn an_index_that_could_give_an_offset_beyond_the_memory_is_refused() {
    let main = |code: Vec<u8>| {
        let mut m = with_addr(op::PUSH32, 0);
        m.extend(code);
        m.extend(ins(op::STP_8, &[&place(INDEXED, 0, 8)]));
        m.push(op::END);
        build(FORMAT_VERSION, 2, &[0; 8], &m)
    };
    for opcode in [op::IDX, op::IDX64] {
        assert_eq!(refused(&main(idx(opcode, 1, 4, 2))), None);
        assert_eq!(
            refused(&main(idx(opcode, 1, 5, 2))),
            Some(LoadError::OutOfMemory(5)),
            "five elements of two bytes in a memory of eight"
        );
        assert_eq!(
            refused(&main(idx(opcode, 0, u32::MAX, u32::MAX))),
            Some(LoadError::OutOfMemory(5)),
            "no overflow in the product"
        );
    }
}

#[test]
fn a_standard_block_through_a_place_is_checked_like_one_at_an_address() {
    let main = |kind: u8, at: Vec<u8>| {
        let mut m = ins(op::FBP, &[&[kind], &at]);
        m.push(op::END);
        build(FORMAT_VERSION, 1, &[0; 8], &m)
    };
    // Kind 4 is CTU, 8 bytes.
    assert_eq!(refused(&main(4, place(ABSOLUTE, 0, 8))), None);
    assert_eq!(
        refused(&main(0xEE, place(ABSOLUTE, 0, 8))),
        Some(LoadError::UnknownFunctionBlock(0))
    );
    assert_eq!(
        refused(&main(4, place(ABSOLUTE, 1, 8))),
        Some(LoadError::OutOfMemory(0))
    );
    assert_eq!(
        refused(&main(4, place(INDEXED, 0, 7))),
        Some(LoadError::OutOfMemory(0)),
        "a region smaller than the block"
    );
    assert_eq!(
        refused(&main(4, place(8, 0, 8))),
        Some(LoadError::BadOperand(0))
    );
}

#[test]
fn a_function_blocks_body_runs_on_the_instance_it_is_called_for() {
    // The block: its DINT at 0 goes up by one. Two instances, at 0 and at 4; an outer block
    // of 8 bytes holds an inner one at its offset 4.
    let mut inner = with_addr(op::LDR_32, 0);
    inner.extend(with_addr(op::PUSH32, 1));
    inner.push(op::ADD_I32);
    inner.extend(with_addr(op::STR_32, 0));
    inner.push(op::RET);
    let mut outer = call_fb(0, &place(RELATIVE, 4, 4));
    outer.push(op::RET);
    let mut main = call_fb(0, &place(ABSOLUTE, 0, 4));
    main.extend(call_fb(0, &place(ABSOLUTE, 4, 4)));
    main.extend(call_fb(0, &place(ABSOLUTE, 4, 4)));
    main.extend(call_fb(1, &place(ABSOLUTE, 8, 8)));
    main.push(op::END);
    let bytes = program(
        FORMAT_VERSION,
        2,
        2,
        &[0; 16],
        &[(&inner, 4), (&outer, 8), (&main, 0)],
    );
    let (result, mem) = run(&bytes);
    assert_eq!(result, Ok(()));
    let dint = |at: usize| i32::from_le_bytes(mem[at..at + 4].try_into().unwrap());
    assert_eq!(
        [dint(0), dint(4), dint(8), dint(12)],
        [1, 2, 0, 1],
        "one call at 0, two at 4, and one through the outer block at 8 + 4"
    );
}

#[test]
fn calls_nested_deeper_than_the_program_declares_are_a_fault_not_a_write_past_the_frames() {
    // main calls f1, which calls f0: two calls in progress at once.
    let mut f1 = call(0);
    f1.push(op::RET);
    let mut main = call(1);
    main.push(op::END);
    let bodies: &[(&[u8], u32)] = &[(&[op::RET], 0), (&f1, 0), (&main, 0)];
    let (result, _) = run(&program(FORMAT_VERSION, 1, 2, &[], bodies));
    assert_eq!(result, Ok(()), "two frames are enough");

    let declared_one = program(FORMAT_VERSION, 1, 1, &[], bodies);
    let (result, _) = run(&declared_one);
    assert_eq!(
        result,
        Err(Fault::CallDepthExceeded(1)),
        "the call inside f1, at code offset 1, is the second in progress"
    );
    // The declared depth is the limit even when the host has more frames to give.
    let p = Program::load(&declared_one).unwrap();
    let mut calls = [Frame::default(); 8];
    assert_eq!(
        scan(&p, &mut [], &mut [0u64; 1], &mut calls, 0, 100),
        Err(Fault::CallDepthExceeded(1))
    );
    // And fewer frames than declared is refused before anything runs.
    let declared_two = program(FORMAT_VERSION, 1, 2, &[], bodies);
    let p = Program::load(&declared_two).unwrap();
    assert_eq!(
        scan(
            &p,
            &mut [],
            &mut [0u64; 1],
            &mut [Frame::default(); 1],
            0,
            100
        ),
        Err(Fault::WrongSizes)
    );
}

/// A program that reads element `subscript` of three INTs (1..3) at 0 into the INT at 6.
fn read_element(opcode: u8, subscript: i64) -> Vec<u8> {
    let mut main = if opcode == op::IDX {
        with_addr(op::PUSH32, subscript as i32 as u32)
    } else {
        ins(op::PUSH64, &[&subscript.to_le_bytes()])
    };
    main.extend(idx(opcode, 1, 3, 2));
    main.extend(ins(op::LDP_I16, &[&place(INDEXED, 0, 6)]));
    main.extend(with_addr(op::ST_16, 6));
    main.push(op::END);
    let mut memory = [0u8; 8];
    for (k, v) in [11i16, 22, 33].into_iter().enumerate() {
        memory[k * 2..k * 2 + 2].copy_from_slice(&v.to_le_bytes());
    }
    build(FORMAT_VERSION, 1, &memory, &main)
}

#[test]
fn a_subscript_inside_its_bounds_reaches_its_element_and_outside_them_is_a_fault() {
    for opcode in [op::IDX, op::IDX64] {
        let at = if opcode == op::IDX { 5 } else { 9 };
        for (subscript, element) in [(1, 11), (2, 22), (3, 33)] {
            let (result, mem) = run(&read_element(opcode, subscript));
            assert_eq!(result, Ok(()));
            assert_eq!(i16::from_le_bytes([mem[6], mem[7]]), element);
        }
        for subscript in [0, 4, -1, i32::MAX as i64, i32::MIN as i64] {
            let (result, mem) = run(&read_element(opcode, subscript));
            assert_eq!(
                result,
                Err(Fault::IndexOutOfRange(at)),
                "opcode {opcode:#x}, subscript {subscript}"
            );
            assert_eq!([mem[6], mem[7]], [0, 0], "and nothing was read");
        }
    }
    // A LINT subscript is not cut to 32 bits first: 2^32 + 1 is not element 1.
    for subscript in [(1i64 << 32) + 1, i64::MIN, i64::MAX] {
        let (result, _) = run(&read_element(op::IDX64, subscript));
        assert_eq!(result, Err(Fault::IndexOutOfRange(9)), "{subscript}");
    }
}

#[test]
fn an_offset_that_would_leave_its_places_region_is_a_fault_not_a_read_of_what_lies_beyond() {
    // No IDX made this offset: a region of 6 bytes, and an offset of 5 for a 2-byte load.
    // Bytes 6..8 are inside the memory; they are still not this place's to read.
    let mut main = with_addr(op::PUSH32, 5);
    main.extend(ins(op::LDP_I16, &[&place(INDEXED, 0, 6)]));
    main.extend(with_addr(op::ST_16, 6));
    main.push(op::END);
    let (result, _) = run(&build(FORMAT_VERSION, 1, &[0xAA; 8], &main));
    assert_eq!(result, Err(Fault::BadCode(5)));

    // The same of a store, a copy and a block call.
    let mut main = with_addr(op::PUSH32, 5);
    main.extend(with_addr(op::PUSH32, 1));
    main.extend(ins(op::STP_16, &[&place(INDEXED, 0, 6)]));
    main.push(op::END);
    let (result, mem) = run(&build(FORMAT_VERSION, 2, &[0xAA; 8], &main));
    assert_eq!(result, Err(Fault::BadCode(10)));
    assert_eq!(mem, [0xAA; 8], "nothing was written");

    let mut main = with_addr(op::PUSH32, 3);
    main.extend(ins(
        op::MOVE,
        &[
            &2u32.to_le_bytes(),
            &place(INDEXED, 0, 4),
            &place(ABSOLUTE, 6, 2),
        ],
    ));
    main.push(op::END);
    let (result, mem) = run(&build(FORMAT_VERSION, 1, &[0xAA; 8], &main));
    assert_eq!(result, Err(Fault::BadCode(5)));
    assert_eq!(mem, [0xAA; 8]);

    let mut main = with_addr(op::PUSH32, 5);
    main.extend(call_fb(0, &place(INDEXED, 0, 8)));
    main.push(op::END);
    let bytes = program(
        FORMAT_VERSION,
        1,
        1,
        &[0; 8],
        &[(&[op::RET], 4), (&main, 0)],
    );
    // An instance of 4 bytes at offset 5 would end at 9, past its region of 8. (The call is
    // at code offset 6: the block's one-byte body, then the program's PUSH32.)
    assert_eq!(run(&bytes).0, Err(Fault::BadCode(6)));
}

#[test]
fn init_puts_initial_values_back_and_move_copies_even_onto_itself() {
    // Memory starts as 1 2 3 4 5 6 7 8. The program zeroes bytes 0..4 by copying from a
    // run of four it first cleared, then INIT restores 0..2.
    let mut main = Vec::new();
    // bytes 2..6 := bytes 0..4 (overlapping)
    main.extend(ins(
        op::MOVE,
        &[
            &4u32.to_le_bytes(),
            &place(ABSOLUTE, 2, 4),
            &place(ABSOLUTE, 0, 4),
        ],
    ));
    main.push(op::END);
    let (result, mem) = run(&build(FORMAT_VERSION, 1, &[1, 2, 3, 4, 5, 6, 7, 8], &main));
    assert_eq!(result, Ok(()));
    assert_eq!(mem, [1, 2, 1, 2, 3, 4, 7, 8]);
    // Every way two runs of bytes can lie: (length, destination, source, memory afterwards).
    for (len, dst, src, after) in [
        (4u32, 0u32, 2u32, [3, 4, 5, 6, 5, 6, 7, 8]),
        (3, 5, 0, [1, 2, 3, 4, 5, 1, 2, 3]),
        (3, 0, 5, [6, 7, 8, 4, 5, 6, 7, 8]),
        (4, 4, 0, [1, 2, 3, 4, 1, 2, 3, 4]),
        (4, 0, 4, [5, 6, 7, 8, 5, 6, 7, 8]),
        (8, 0, 0, [1, 2, 3, 4, 5, 6, 7, 8]),
        (7, 1, 0, [1, 1, 2, 3, 4, 5, 6, 7]),
        (7, 0, 1, [2, 3, 4, 5, 6, 7, 8, 8]),
        (0, 8, 0, [1, 2, 3, 4, 5, 6, 7, 8]),
    ] {
        let mut main = ins(
            op::MOVE,
            &[
                &len.to_le_bytes(),
                &place(ABSOLUTE, dst, len),
                &place(ABSOLUTE, src, len),
            ],
        );
        main.push(op::END);
        let (result, mem) = run(&build(FORMAT_VERSION, 1, &[1, 2, 3, 4, 5, 6, 7, 8], &main));
        assert_eq!(result, Ok(()));
        assert_eq!(mem, after, "{len} bytes from {src} to {dst}");
    }

    let mut main = with_addr(op::PUSH32, 0x99);
    main.extend(with_addr(op::ST_8, 1));
    main.extend(with_addr(op::PUSH32, 0x77));
    main.extend(with_addr(op::ST_8, 5));
    main.extend(ins(op::INIT, &[&0u32.to_le_bytes(), &4u32.to_le_bytes()]));
    main.push(op::END);
    let (result, mem) = run(&build(FORMAT_VERSION, 1, &[1, 2, 3, 4, 5, 6, 7, 8], &main));
    assert_eq!(result, Ok(()));
    assert_eq!(
        mem,
        [1, 2, 3, 4, 5, 0x77, 7, 8],
        "byte 1 is back to its initial value; byte 5, outside the INIT, keeps what was stored"
    );
}

/// `JMP target`, `JZ target`.
fn jump(opcode: u8, target: u32) -> Vec<u8> {
    with_addr(opcode, target)
}

#[test]
fn a_jump_into_the_middle_of_an_instruction_is_refused_at_load() {
    // 0: PUSH32 v   5: JZ 11   10: NOT... is not reached; 11: END.
    // The good program: both jumps land where an instruction starts.
    let good = |forward: u32, back: u32| {
        let mut code = with_addr(op::PUSH32, 0); // 0..5
        code.extend(jump(op::JZ, forward)); // 5..10
        code.extend(with_addr(op::PUSH32, 0)); // 10..15
        code.extend(jump(op::JZ, back)); // 15..20
        code.push(op::END); // 20
        build(FORMAT_VERSION, 1, &[], &code)
    };
    assert_eq!(
        refused(&good(20, 0)),
        None,
        "to the END, and back to the start"
    );
    assert_eq!(refused(&good(10, 5)), None, "to an instruction between");
    let (result, _) = run(&good(20, 0));
    assert_eq!(result, Ok(()), "and what loads runs");

    // Forward, into the operand of the PUSH32 at 10: every byte of it.
    for target in 11..15 {
        assert_eq!(
            refused(&good(target, 0)),
            Some(LoadError::BadJump(5)),
            "forward to {target}"
        );
    }
    // Back, into the operand of the PUSH32 at 0, and into the JZ at 5.
    for target in (1..5).chain(6..10) {
        assert_eq!(
            refused(&good(20, target)),
            Some(LoadError::BadJump(15)),
            "back to {target}"
        );
    }
    // A jump into its own operand, forward by one byte.
    assert_eq!(refused(&good(6, 0)), Some(LoadError::BadJump(5)));
    assert_eq!(refused(&good(20, 16)), Some(LoadError::BadJump(15)));
}

#[test]
fn a_jump_into_operands_that_read_as_instructions_is_still_refused() {
    // The operand of the PUSH32 is four bytes that are each a whole instruction (NOT), so
    // code read from offset 1 decodes and falls in step with the real instructions at 5.
    // Only walking from where an instruction is known to start tells that 1 is not one.
    let operand = u32::from_le_bytes([op::NOT; 4]);
    let mut code = with_addr(op::PUSH32, operand); // 0..5
    code.extend(jump(op::JMP, 1)); // 5..10, back into the operand
    assert_eq!(
        refused(&build(FORMAT_VERSION, 1, &[], &code)),
        Some(LoadError::BadJump(5))
    );
    // The same forward: the JMP at 0 lands in the operand of the PUSH32 at 5.
    let mut code = jump(op::JMP, 7); // 0..5
    code.extend(with_addr(op::PUSH32, operand)); // 5..10
    code.extend(with_addr(op::ST_32, 0)); // 10..15
    code.push(op::END);
    assert_eq!(
        refused(&build(FORMAT_VERSION, 1, &[0; 4], &code)),
        Some(LoadError::BadJump(0))
    );
    // And to the instruction itself it loads.
    let mut code = jump(op::JMP, 5);
    code.extend(with_addr(op::PUSH32, operand));
    code.extend(with_addr(op::ST_32, 0));
    code.push(op::END);
    assert_eq!(refused(&build(FORMAT_VERSION, 1, &[0; 4], &code)), None);
}

#[test]
fn a_jump_is_checked_against_the_instructions_of_its_own_body() {
    // Two bodies. In the second, at code offset 6, a jump back to the start of its body
    // (6) loads; a jump to 7, the middle of itself, does not. Offsets are of the whole
    // code, and the walk starts at the body's start, not at 0.
    let f: &[u8] = &[op::RET];
    let body = |target: u32| {
        let mut main = with_addr(op::PUSH32, 0); // 1..6
        main.extend(jump(op::JZ, target)); // 6..11
        main.push(op::END); // 11
        program(FORMAT_VERSION, 1, 0, &[], &[(f, 0), (&main, 0)])
    };
    assert_eq!(refused(&body(1)), None, "the first instruction of the body");
    assert_eq!(refused(&body(6)), None, "the jump itself");
    assert_eq!(refused(&body(11)), None, "the END");
    assert_eq!(refused(&body(3)), Some(LoadError::BadJump(6)));
    assert_eq!(refused(&body(8)), Some(LoadError::BadJump(6)));
}

#[test]
fn the_stack_stops_at_the_depth_the_program_declares_whatever_the_host_gives() {
    // Declares a depth of 1 but pushes 2.
    let mut code = with_addr(op::PUSH32, 1);
    code.extend(with_addr(op::PUSH32, 2));
    code.push(op::ADD_I32);
    code.extend(with_addr(op::ST_32, 0));
    code.push(op::END);
    let one = build(FORMAT_VERSION, 1, &[0; 4], &code);
    let p = Program::load(&one).unwrap();
    // A host with a stack of 64: the program faults all the same, as it will on a
    // controller that has exactly what it declares.
    let mut memory = [0u8; 4];
    assert_eq!(
        scan(&p, &mut memory, &mut [0u64; 64], &mut [], 0, 100),
        Err(Fault::StackOverflow(5))
    );
    assert_eq!(memory, [0; 4], "and nothing was stored");

    // Declaring the 2 it uses, it runs: in exactly 2 and in 64.
    let two = build(FORMAT_VERSION, 2, &[0; 4], &code);
    let p = Program::load(&two).unwrap();
    for host in [2usize, 64] {
        let mut memory = [0u8; 4];
        let mut stack = vec![0u64; host];
        assert!(scan(&p, &mut memory, &mut stack, &mut [], 0, 100).is_ok());
        assert_eq!(memory, 3i32.to_le_bytes(), "a stack of {host}");
    }
    // A host with less than the program declares is refused before anything runs.
    assert_eq!(
        scan(&p, &mut [0u8; 4], &mut [0u64; 1], &mut [], 0, 100),
        Err(Fault::WrongSizes)
    );
}

#[test]
fn a_program_that_declares_more_than_the_host_has_is_refused_when_it_is_loaded() {
    // main calls f: a stack of 2, one call frame, 4 bytes of memory.
    let mut main = call(0);
    main.extend(with_addr(op::PUSH32, 1));
    main.extend(with_addr(op::PUSH32, 2));
    main.push(op::ADD_I32);
    main.extend(with_addr(op::ST_32, 0));
    main.push(op::END);
    let bytes = program(
        FORMAT_VERSION,
        2,
        1,
        &[0; 4],
        &[(&[op::RET], 0), (&main, 0)],
    );
    let host = Limits {
        memory: 4,
        stack_depth: 2,
        call_depth: 1,
    };
    let p = Program::load_within(&bytes, &host).expect("exactly what it declares is enough");
    assert_eq!(
        (p.memory_size(), p.stack_depth(), p.call_depth()),
        (4, 2, 1)
    );
    let within = |limits: Limits| Program::load_within(&bytes, &limits).err();
    assert_eq!(
        within(Limits { memory: 3, ..host }),
        Some(LoadError::TooMuchMemory(4))
    );
    assert_eq!(
        within(Limits {
            stack_depth: 1,
            ..host
        }),
        Some(LoadError::StackTooDeep(2))
    );
    assert_eq!(
        within(Limits {
            call_depth: 0,
            ..host
        }),
        Some(LoadError::CallsTooDeep(1))
    );
    // A host with more than enough takes it, and a damaged program is refused for its
    // damage whatever the host has.
    let roomy = Limits {
        memory: 4096,
        stack_depth: 64,
        call_depth: 16,
    };
    assert!(Program::load_within(&bytes, &roomy).is_ok());
    let mut damaged = bytes.clone();
    damaged[25] ^= 0x10;
    assert_eq!(
        Program::load_within(&damaged, &roomy).err(),
        Some(LoadError::Corrupt)
    );
}
