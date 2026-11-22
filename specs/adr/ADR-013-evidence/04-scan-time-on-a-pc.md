# ADR-013 evidence: Scan time on a PC

Part of [ADR-013](../ADR-013-logic-compiler-and-vm.md), section 5.

- **Scan time on a PC** (`scan_time.rs`, release, i9-11980HK, Linux, not real-time, desktop
  load): a program shaped like a small plant (CASE sequence, 8 TON, CTU, R_TRIG, a PI loop in
  REAL, a 50-turn FOR) is 884 bytes of code and 280 of memory, 970 instructions per scan:
  over 200,000 scans the mean was between 7.3 and 13.1 µs in six runs of the same code (the
  machine was in use; the fastest run: p99 9.3 µs, max 100 µs; the slowest: p99 22.5 µs; the
  worst single scan seen in any run: 336 µs). The VM has not been
  optimised. Windows and macOS: not measured.
