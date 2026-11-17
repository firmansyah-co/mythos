# Format-2 bytecode, as the format-2 compiler made it

Each `.mlbc` here is a program of `tests/differential/` compiled by the compiler of
`78c84a729e9fb56f1991aa2d77a5d1842968574b` (the head of L2c-1, strings, the last commit whose
VM made format 2), for the `pc` profile. `older_formats_run_alike.rs` loads them and runs
them against the same sources compiled now.

They are evidence of what a controller holds after L2c-1, so they are **never regenerated**:
regenerated, they would be what the current compiler makes and prove nothing. A program added
to `tests/differential/` after format 2 has no file here.

Made once, in a copy of that commit (`git archive 78c84a7 | tar -x -C <dir>`, with its own
`CARGO_TARGET_DIR`), by a test file that was not committed:

```rust
for each tests/differential/*.st:
    let c = mythos_logic_compiler::compile(&source, &TargetProfile::pc()).unwrap();
    std::fs::write(format!("{out}/{stem}.mlbc"), &c.bytecode).unwrap();
```

```
sha256                                                            bytes  file
e0bf1e68023c24bf17f6895bd726ccd6e65e7f378bce53e43c04e8fb64dbffc6   1424  functions_and_blocks.mlbc
b51e014202107f1e7b88813befd1c97c9026227ef8de1b0176510bd4c4529711    451  integers_and_control.mlbc
dfe2da2b657dd3f724a876dfcd56e0ecb5ddc7db313e0b6c7c4c1f5481fbb45d    360  reals_and_conversions.mlbc
76ae01959f502f2a09c99faf88657342302a133c9880690c7792e8d71f974a0d   1196  small_plant.mlbc
9f6ff40eb3d89af7cea6b5bf800f67d30f138f1370f23b3bbbad018dfcaf3f75   2640  strings.mlbc
5826dcb106184b51696f1edc2938663e28d054be2f6206d734c6d48d3042c01f   1914  structures_and_arrays.mlbc
8656a4678aecf586d9ac743c4531622328673f9a8282a1f23f12d75a172aad0d    681  timers_and_counters.mlbc
```
