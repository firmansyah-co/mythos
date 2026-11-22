# ADR-013 evidence: Depth

Part of [ADR-013](../ADR-013-logic-compiler-and-vm.md), section 5.

- **Depth** (L2a, after a stack overflow on Windows CI). The overflow was a damaged source in
  which a structure held itself (`range : Limits` inside `Limits`): `holds_instance` followed it
  round without end before the containment check ran, on `dev` too, so any `check` sent to the
  logic module with such a type killed the module's process. Windows met it first only because
  its checkout gives the corpus CRLF line ends, so the fixed seed damaged other text there
  (`.gitattributes` now keeps `*.st` LF everywhere, and the damage test damages CRLF forms on
  purpose). Nesting thousands deep overflowed as well, in each of the 14 kinds and for chains
  of 3,000 structures, named array types and function blocks. Measured before the bound, in a
  debug build: a level of parentheses took 14.8 KiB of stack, a nested IF 11.7 KiB. The parser now
  reads operators by precedence in one loop rather than a function per precedence, and the
  recursive functions of the parser and the checker keep what they say off the path a nested
  construct takes: the deepest allowed takes 320 KiB in parentheses, 536 KiB in IFs and 544 KiB
  in CASEs, all of it included.
  `deep_sources_are_refused.rs` (6 tests) refuses each kind nested 5,000 deep with one
  diagnostic where the limit is reached, compiles the deepest allowed of each on a 1 MiB
  thread, and refuses structures that contain themselves; `logic_module.rs` sends such sources
  to the real module's `check` and `build`, and the same process answers the next request.
