# ADR-013 evidence: Format versions

Part of [ADR-013](../ADR-013-logic-compiler-and-vm.md), section 5.

- **Format versions** (review of L2c-1: the loader refused format 1, which every controller
  running a program held). The six corpus programs compiled by the format-1 and the format-2
  compiler, each tree extracted with its own target directory, differ only in the version
  field and the checksum (the reviewer's comparison); the diff of the VM changes no format-1
  opcode, operand layout, loader check or run-time step. 5 tests added to the five logic
  crates (184, from 179), and one to the runtime module, which takes a format-1 program
  through its own configuration and refuses a newer one. `older_formats_run_alike.rs`: the
  six format-1 fixtures (made once by `dev`'s compiler at `f4637cd`, committed with their
  hashes) run 200 scans each beside the same source compiled now, the same inputs written
  before every scan, memory equal after every scan and changed by more than half of them.
  Mutation-checked, each in a copy of the tree: reading only the current version turns
  `every_format_this_vm_extends_is_read_and_runs_alike`,
  `an_instruction_newer_than_the_programs_format_is_refused_at_load`,
  `format_1_bytecode_runs_to_the_same_scans_as_its_source_compiled_now` and the runtime
  module's `a_program_built_before_an_update_of_the_vm_is_still_taken` red; reading any
  version turns `a_format_newer_than_this_vm_is_refused_not_guessed_at`,
  `a_program_of_the_format_before_this_one_is_refused_by_its_version` and the runtime
  module's test red; skipping the check of each instruction against the program's version
  turns `an_instruction_newer_than_the_programs_format_is_refused_at_load` red. The oracle
  and the emulators are unchanged (7,500 values, 287 deliberate; all 14 runs equal).
