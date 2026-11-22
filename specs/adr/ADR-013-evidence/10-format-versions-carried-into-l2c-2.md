# ADR-013 evidence: Format versions, carried into L2c-2

Part of [ADR-013](../ADR-013-logic-compiler-and-vm.md), section 5.

- **Format versions, carried into L2c-2.** The seven corpus programs compiled by L2c-1's
  compiler (`78c84a7`, format 2) and by L2c-2's (format 3) differ only in the version field
  and the checksum; the diff of the VM adds the thirteen numeric instructions on new opcodes
  and changes nothing before them. One test added (198 in the five logic crates):
  `format_2_bytecode_runs_to_the_same_scans_as_its_source_compiled_now`, over the seven
  format-2 fixtures, committed with their hashes; the loader tests check every later
  instruction in every format from 1 to 3, and the runtime module takes a format-2 program.
  Mutation-checked, each in a copy of the tree: reading only the current version turns
  `every_format_this_vm_extends_is_read_and_runs_alike`,
  `an_instruction_newer_than_the_programs_format_is_refused_at_load`, both
  `format_N_bytecode_runs_…` tests and the runtime module's
  `a_program_built_before_an_update_of_the_vm_is_still_taken` red; reading any version turns
  `a_format_newer_than_this_vm_is_refused_not_guessed_at`,
  `a_program_of_the_format_before_this_one_is_refused_by_its_version` and the runtime
  module's test red; skipping the check of each instruction against its program's version
  turns `an_instruction_newer_than_the_programs_format_is_refused_at_load` red. The oracle
  (9,480 values, 287 deliberate) and the emulators (all 16 runs equal) are unchanged.
