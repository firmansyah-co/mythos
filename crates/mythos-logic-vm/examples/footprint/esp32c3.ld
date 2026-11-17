/* Enough of an ESP32-C3 (RISC-V) memory map to link the VM and measure it; not a boot image. */
MEMORY {
  FLASH : ORIGIN = 0x42000000, LENGTH = 4096K
  RAM   : ORIGIN = 0x3FC80000, LENGTH = 320K
}
ENTRY(_start)
SECTIONS {
  .text : { *(.text .text.*) } > FLASH
  .rodata : { *(.rodata .rodata.* .srodata .srodata.*) } > FLASH
  .data : { *(.data .data.* .sdata .sdata.*) } > RAM AT > FLASH
  .bss (NOLOAD) : { *(.bss .bss.* .sbss .sbss.*) *(COMMON) } > RAM
}
