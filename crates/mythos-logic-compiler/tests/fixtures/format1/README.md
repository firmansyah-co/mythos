# Format-1 bytecode, as the format-1 compiler made it

Each `.mlbc` here is a program of `tests/differential/` compiled by the compiler of
`origin/dev` at `f4637cd71879dd24b7a85d749bc9ea3908d04159`, the last commit whose VM made
format 1, for the `pc` profile. `older_formats_run_alike.rs` loads them and runs them against
the same sources compiled now.

They are evidence of what a controller holds today, so they are **never regenerated**:
regenerated, they would be what the current compiler makes and prove nothing. A program added
to `tests/differential/` after format 1 has no file here.

Made once, in a copy of that commit (`git archive f4637cd | tar -x -C <dir>`, with its own
`CARGO_TARGET_DIR`), by a test file that was not committed:

```rust
for each tests/differential/*.st:
    let c = mythos_logic_compiler::compile(&source, &TargetProfile::pc()).unwrap();
    std::fs::write(format!("{out}/{stem}.mlbc"), &c.bytecode).unwrap();
```

```
sha256                                                            bytes  file
d49cf12a4f43ddbc679bfdf81f117d2610a575a1ad292f0326d6310feb6b4f1f   1424  functions_and_blocks.mlbc
954f548f67730363829b77c894c4bd82024d0015b7a9b788ee3d802f1e28e5a7    451  integers_and_control.mlbc
41f6fb1c239c495207b07786ccef25a509a46d21d93188014a62577f7de5f17f    360  reals_and_conversions.mlbc
7d73c37e04027d0b891d5429caf10b8dd56d129833ad45d763272bec99f3c676   1196  small_plant.mlbc
bbd486699d833c57c8871708fdc99e6cefb126a624070bdcaaf7df54fa0ce1ac   1914  structures_and_arrays.mlbc
2c22bc2734d0d7e83ec252865de581f6d9d168ae3812402fddc732af59137c78    681  timers_and_counters.mlbc
```
