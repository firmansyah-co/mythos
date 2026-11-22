# ADR-013 evidence: Microcontrollers

Part of [ADR-013](../ADR-013-logic-compiler-and-vm.md), section 5.

- **Microcontrollers** (`scripts/logic-vm-footprint.sh`, CI job `logic-vm-bare-metal`): the VM
  with its loader and scan loop, `opt-level=s`, LTO, linked with `core` alone: **RP2040**
  (thumbv6m-none-eabi) 32,288 bytes of code with the real functions and formats 1 and 2 read
  (32,180 with the real functions before that, L2c-2; 23,216 with strings alone, 19,304 before them, 19,200 before L2b, 15,884 before format 1);
  **ESP32-C3** (riscv32imc-unknown-none-elf) 39,274 bytes (39,206 before the older formats were
  read, L2c-2; 27,204 with strings alone,
  22,812 before them, 22,908 before L2b, 19,146 before format 1). The real functions are most
  of L2c-2's: LREAL arithmetic in software on both. The L2b figures and the ones before it were both measured with rustc 1.99.1; the
  RISC-V build came out smaller with more in it, which is the compiler's doing and was not
  looked into. Run on a real board: not yet (none attached). The original ESP32 and the S3 are
  Xtensa, which needs Espressif's Rust toolchain: not built.
