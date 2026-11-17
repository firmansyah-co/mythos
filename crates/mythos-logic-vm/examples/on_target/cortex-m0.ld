/* QEMU's `microbit` machine (nRF51, Cortex-M0): flash at 0, 16K of RAM. */
MEMORY {
  FLASH : ORIGIN = 0x00000000, LENGTH = 256K
  RAM   : ORIGIN = 0x20000000, LENGTH = 16K
}
ENTRY(reset)
SECTIONS {
  .vector_table ORIGIN(FLASH) : {
    LONG(ORIGIN(RAM) + LENGTH(RAM));   /* initial stack pointer */
    KEEP(*(.vector_table.reset));      /* reset vector */
  } > FLASH
  .text : { *(.text .text.*) } > FLASH
  .rodata : { *(.rodata .rodata.*) } > FLASH
  .data : { *(.data .data.*) } > RAM AT > FLASH
  .bss (NOLOAD) : { *(.bss .bss.*) *(COMMON) } > RAM
  /DISCARD/ : { *(.ARM.exidx .ARM.exidx.*) }
}
