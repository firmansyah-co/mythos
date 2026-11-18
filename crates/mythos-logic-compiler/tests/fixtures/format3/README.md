# Format-3 bytecode, as the format-3 compiler made it, with its symbols

Each `.mlbc` here is a program of `tests/differential/` compiled by the compiler of
`9aa89e8b9731fb3f00fe897811e3aa78f669fff6` (the head of the C7b-1 branch; its tree,
`16426ba`, is that of `ca35b59` on `dev`, #137, the last commit whose compiler made format 3
and ran the standard function blocks natively in the VM), for the `pc` profile.
Beside each is the symbol table that compiler gave it (`<name>.symbols.json`): where each
variable of the program is, the public members of a standard function block instance among
them. `older_formats_run_alike.rs` loads them and runs them against the same sources compiled
now, comparing every variable the two programs share after every scan; and
`standard_blocks_run_as_the_native_ones.rs` runs the standard blocks in them.

The bytes of format 1 and format 2 here (`../format1`, `../format2`) differ from these only in
the version field and the checksum, so these symbols say where the variables of those are too.
A test holds that.

They are evidence of what a controller holds after C7b, so they are **never regenerated**:
regenerated, they would be what the current compiler makes and prove nothing.

Made once, in a copy of that commit (`git archive 9aa89e8 | tar -x -C <dir>`, with its own
`CARGO_TARGET_DIR`), by a test file that was not committed:

```rust
for each tests/differential/*.st:
    let c = mythos_logic_compiler::compile(&source, &TargetProfile::pc()).unwrap();
    std::fs::write(format!("{out}/{stem}.mlbc"), &c.bytecode).unwrap();
    std::fs::write(format!("{out}/{stem}.symbols.json"),
                   serde_json::to_string_pretty(&c.symbols).unwrap() + "\n").unwrap();
```

```
sha256                                                            bytes  file
ef41213dad5cd2046d4d1e2dd5368d1d6d073bc61f8f703ebeee2d064e630913   1424  functions_and_blocks.mlbc
3f7ae6fcbc6069cb6c385f81963fdd10e01914c01d752d99d287987b6e6361f2    451  integers_and_control.mlbc
6ba015ec984ec10cb02ab3cc18748e2e6f82cff5fbbceb3ff243a79fb860f28e   1384  library.mlbc
89b5d3a06ebb6da3365b4799cb37a35b9716ef4b56a056e61b6235424ace28a0    360  reals_and_conversions.mlbc
92c6461a915280aee3bce871806b1e33483733e57025be8e754ad89efbb4029c   1196  small_plant.mlbc
8808bf447e3f6d4740f909fe295f978e1c921343fb343c8954182e5fdf3666da   2640  strings.mlbc
f95e624e63e51933cd4ebcb5b68f5ab54650df4942af37200778a176727c3b49   1914  structures_and_arrays.mlbc
c7f95fd6dc1ce0132ad172661693aebbb1f816c790a1aad3ea0f807010d348cf    681  timers_and_counters.mlbc
```
