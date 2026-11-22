# ADR-013 evidence: Tests

Part of [ADR-013](../ADR-013-logic-compiler-and-vm.md), section 5.

- **Tests**: 107 in the five logic crates (42 before format 1). Format 1 was mutation-checked
  with 29 mutations (index range, call depth, recursion at three layers, locals re-initialised,
  region confinement, the loader's check of each new instruction), each turning named tests
  red; and a damaged-bytecode test goes red when one run-time range check is removed. Earlier
  mutation checks: removing INT wrapping, the TP reset, the loader's address check, the
  division-by-zero fault, or counting CTU only to PV each turns a named test red (run before the crates were split;
  the code moved, it did not change). So does undoing each rule taken from the standard:
  rounding a half away from zero, faulting on MOD by zero, the earlier F_TRIG, and giving `=`
  a lower precedence than `<`.
