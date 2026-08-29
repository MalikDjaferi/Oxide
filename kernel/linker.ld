ENTRY(_start)

PHDRS
{
    rodata PT_LOAD FLAGS(4);
    text   PT_LOAD FLAGS(5);
    data   PT_LOAD FLAGS(6);
    dynamic PT_DYNAMIC FLAGS(6);
}

SECTIONS
{
    . = 0xffffffff80000000;

    /* ---------------------------------------------------------------
     * READ-ONLY
     * --------------------------------------------------------------- */

    .rodata ALIGN(0x1000) :
    {
        *(.rodata .rodata.*)
    } :rodata

    .limine_requests_start :
    {
        KEEP(*(.limine_requests_start))
    } :rodata

    .limine_requests :
    {
        KEEP(*(.limine_requests))
    } :rodata

    .limine_requests_end :
    {
        KEEP(*(.limine_requests_end))
    } :rodata

    .dynsym :
    {
        *(.dynsym)
    } :rodata

    .gnu.hash :
    {
        *(.gnu.hash)
    } :rodata

    .hash :
    {
        *(.hash)
    } :rodata

    .dynstr :
    {
        *(.dynstr)
    } :rodata

    .rela.dyn :
    {
        *(.rela.dyn)
    } :rodata

    .eh_frame_hdr :
    {
        *(.eh_frame_hdr)
    } :rodata

    .eh_frame :
    {
        *(.eh_frame)
    } :rodata

    /* ---------------------------------------------------------------
     * EXECUTABLE
     * --------------------------------------------------------------- */

    . = ALIGN(0x1000);

    .text :
    {
        *(.text .text.*)
    } :text

    /* ---------------------------------------------------------------
     * WRITABLE
     * --------------------------------------------------------------- */

    . = ALIGN(0x1000);

    .data :
    {
        *(.data .data.*)
    } :data

    .got :
    {
        *(.got)
        *(.got.plt)
    } :data

    .bss ALIGN(0x1000) :
    {
        *(COMMON)
        *(.bss .bss.*)
    } :data

    /*
     * IMPORTANT:
     *
     * .dynamic is mapped to BOTH the writable LOAD segment and the
     * PT_DYNAMIC program header.
     */
    .dynamic :
    {
        *(.dynamic)
    } :data :dynamic
}
