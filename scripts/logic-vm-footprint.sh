#!/usr/bin/env bash
# Build the logic VM into bare-metal firmware for each microcontroller target MythOS plans to
# run on, and print what it costs in flash and RAM (ADR-013).
#
#   scripts/logic-vm-footprint.sh
#
# Needs the targets: rustup target add thumbv6m-none-eabi riscv32imc-unknown-none-elf
# A build that fails here means something in the VM now needs `std` or an allocator, which
# would stop it running on a microcontroller.
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT="$PWD"
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"

SIZE="${LLVM_SIZE:-}"
if [ -z "$SIZE" ]; then
    SYSROOT="$(rustc --print sysroot)"
    SIZE="$(find "$SYSROOT" -name llvm-size -type f 2>/dev/null | head -n 1)"
    [ -n "$SIZE" ] || SIZE="$(command -v llvm-size || true)"
fi
[ -n "$SIZE" ] || { echo "llvm-size not found: rustup component add llvm-tools"; exit 1; }

for pair in "thumbv6m-none-eabi rp2040" "riscv32imc-unknown-none-elf esp32c3"; do
    read -r TRIPLE BOARD <<<"$pair"
    echo "== $BOARD ($TRIPLE)"
    RUSTFLAGS="-C link-arg=-T$ROOT/crates/mythos-logic-vm/examples/footprint/$BOARD.ld" \
    CARGO_PROFILE_RELEASE_OPT_LEVEL=s \
    CARGO_PROFILE_RELEASE_LTO=true \
    CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1 \
    CARGO_PROFILE_RELEASE_PANIC=abort \
        cargo build --quiet -p mythos-logic-vm --example footprint --release --target "$TRIPLE"
    "$SIZE" -A "$TARGET_DIR/$TRIPLE/release/examples/footprint" | grep -E '^(section|\.text|\.rodata|\.data|\.bss|Total)'
done
