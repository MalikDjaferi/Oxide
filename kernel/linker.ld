/*
 * Oxide kernel linker script.
 *
 * The linker script controls the final layout of the kernel ELF.
 *
 * Limine requires ELF program headers with different permissions
 * to not share the same memory page. Each permission group is
 * therefore separated by a full 4 KiB page boundary.
 */

/*
 * Tell the linker that _start is the first instruction executed
 * when Limine transfers control to the Oxide kernel.
 */
ENTRY(_start)

/*
 * Define the kernel's virtual memory layout.
 */
SECTIONS
{
    /*
     * Oxide lives in the higher half of the virtual address space.
     */
    . = 0xffffffff80000000;

    /*
     * Read-only data.
     *
     * This region contains ELF metadata and other read-only data.
     */
    .rodata : ALIGN(0x1000)
    {
        *(.rodata .rodata.*)
    }

    /*
     * Executable kernel code.
     *
     * Place the code on its own 4 KiB page so it cannot share
     * a page with read-only or writable memory.
     */
    . = ALIGN(0x1000);

    .text :
    {
        *(.text .text.*)
    }

    /*
     * Writable initialized data.
     *
     * Move to the next complete page before beginning writable data.
     */
    . = ALIGN(0x1000);

    .data :
    {
        *(.data .data.*)
    }

    /*
     * Writable zero-initialized data.
     *
     * Keep BSS on a page boundary as well.
     */
    . = ALIGN(0x1000);

    .bss :
    {
        *(COMMON)
        *(.bss .bss.*)
    }
}