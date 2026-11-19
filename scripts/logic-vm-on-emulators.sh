#!/usr/bin/env bash
# Run compiled programs on the logic VM inside emulated microcontrollers, and require what they
# give there to be what they give on this machine, scan by scan (ADR-013 §2.9).
#
#   scripts/logic-vm-on-emulators.sh
#
# For each program in crates/mythos-logic-compiler/tests/differential/ (Structured Text) and
# crates/mythos-logic-compiler/tests/ladder/ (Ladder Diagram, TC6 XML) and tests/fbd/ (Function
# Block Diagram, TC6 XML), and each machine:
#   1. compile it and run it here (`expected_scans`): the expected lines;
#   2. build the `on_target` firmware around its bytecode for the machine's instruction set;
#   3. run that in QEMU and compare what it prints with the expected lines.
# A line is `scan <n> <crc32 of the VM's whole memory> <instructions>`.
#
# Needs: rustup target add thumbv6m-none-eabi riscv32imc-unknown-none-elf
#        qemu-system-arm and qemu-system-riscv32 (QEMU_ARM, QEMU_RISCV to name them)
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT="$PWD"
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
WORK="$TARGET_DIR/logic-on-emulators"
QEMU_ARM="${QEMU_ARM:-qemu-system-arm}"
QEMU_RISCV="${QEMU_RISCV:-qemu-system-riscv32}"
for tool in "$QEMU_ARM" "$QEMU_RISCV"; do
    command -v "$tool" >/dev/null || { echo "FAIL: $tool not found"; exit 1; }
done
mkdir -p "$WORK"

cargo build --quiet -p mythos-logic-compiler --example expected_scans
EXPECTED_SCANS="$TARGET_DIR/debug/examples/expected_scans"

# name | rust target | linker script | profile | emulator command (the firmware is appended)
# What the firmware prints goes to the file @OUT@: semihosting's console on the Arm machine,
# the UART on the RISC-V one. The emulator's own messages stay on its stdout and stderr.
QUIET="-display none -monitor none"
MACHINES=(
    "cortex-m0|thumbv6m-none-eabi|cortex-m0.ld|rp2040|$QEMU_ARM -M microbit $QUIET -serial none -chardev file,id=out,path=@OUT@ -semihosting-config enable=on,target=native,chardev=out -kernel"
    "rv32imc|riscv32imc-unknown-none-elf|riscv-virt.ld|esp32|$QEMU_RISCV -M virt -bios none $QUIET -serial file:@OUT@ -kernel"
)

FAILED=0
RAN=0
for st in "$ROOT"/crates/mythos-logic-compiler/tests/differential/*.st \
    "$ROOT"/crates/mythos-logic-compiler/tests/ladder/*.xml \
    "$ROOT"/crates/mythos-logic-compiler/tests/fbd/*.xml; do
    name="$(basename "$st")"
    name="${name%.*}"
    for machine in "${MACHINES[@]}"; do
        IFS='|' read -r mname triple script profile emulator <<<"$machine"
        bytecode="$WORK/$name.$mname.mlbc"
        expected="$WORK/$name.$mname.expected"
        actual="$WORK/$name.$mname.actual"
        "$EXPECTED_SCANS" "$st" "$profile" "$bytecode" >"$expected"
        MYTHOS_LOGIC_PROGRAM="$bytecode" \
        RUSTFLAGS="-C link-arg=-T$ROOT/crates/mythos-logic-vm/examples/on_target/$script" \
        CARGO_PROFILE_RELEASE_OPT_LEVEL=s \
        CARGO_PROFILE_RELEASE_LTO=true \
        CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1 \
        CARGO_PROFILE_RELEASE_PANIC=abort \
            cargo build --quiet -p mythos-logic-vm --example on_target --release --target "$triple"
        firmware="$TARGET_DIR/$triple/release/examples/on_target"
        status=0
        rm -f "$actual"
        # shellcheck disable=SC2086
        timeout 120 ${emulator//@OUT@/$actual} "$firmware" </dev/null >"$actual.log" 2>&1 || status=$?
        [ -f "$actual" ] || : >"$actual"
        RAN=$((RAN + 1))
        lines="$(grep -c '^scan ' "$actual" || true)"
        if [ "$status" = "0" ] && cmp -s "$expected" "$actual"; then
            echo "ok   $name on $mname: $lines scans, every one equal to this machine's"
        else
            echo "FAIL $name on $mname (emulator exit $status, $lines scans printed)"
            { diff "$expected" "$actual" || true; } | head -n 10 | sed 's/^/       /'
            head -n 5 "$actual.log" | sed 's/^/       /'
            FAILED=1
        fi
    done
done
[ "$RAN" -gt 0 ] || { echo "FAIL: nothing was run"; exit 1; }
if [ "$FAILED" = "0" ]; then echo "all $RAN runs equal"; else echo "FAILED"; fi
exit "$FAILED"
