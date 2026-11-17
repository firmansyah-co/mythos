/* QEMU's RISC-V `virt` machine: RAM at 0x80000000, where it starts executing with -bios none. */
MEMORY {
  RAM : ORIGIN = 0x80000000, LENGTH = 1M
}
ENTRY(_start)
SECTIONS {
  .text : { KEEP(*(.text.entry)) *(.text .text.*) } > RAM
  .rodata : { *(.rodata .rodata.* .srodata .srodata.*) } > RAM
  .data : { *(.data .data.* .sdata .sdata.*) } > RAM
  .bss (NOLOAD) : { *(.bss .bss.* .sbss .sbss.*) *(COMMON) } > RAM
  _stack_top = ORIGIN(RAM) + LENGTH(RAM);
}
