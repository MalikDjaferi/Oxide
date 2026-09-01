//! Oxide kernel entry point.
//!
//! This file contains the kernel's startup sequence and main execution loop.
//!
//! Oxide is a bare-metal `no_std` / `no_main` operating system kernel.
//! There is therefore no normal Rust `main()` function.
//!
//! The bootloader transfers execution directly to `_start()`.
//!
//! Current initialization:
//
//!     Limine
//!       ↓
//!     _start()
//!       ↓
//!     GDT
//!       ↓
//!     Framebuffer
//!       ↓
//!     Terminal
//!       ↓
//!     Keyboard
//!       ↓
//!     Main kernel loop
//!
//! A temporary PS/2 diagnostic is included in this file to determine whether
//! raw keyboard bytes are reaching the kernel.

#![no_std]
#![no_main]

// ============================================================================
// KERNEL MODULES
// ============================================================================
//
// These are the existing Oxide kernel subsystems.
//
// We are not replacing or redesigning them here. The current investigation is
// specifically about why keyboard input is not reaching the terminal.

mod gdt;
mod keyboard;
mod terminal;

// ============================================================================
// LIMINE FRAMEBUFFER REQUEST
// ============================================================================
//
// Limine provides boot information to the kernel.
//
// We request a framebuffer so the terminal can draw text on the display.

use limine::request::FramebufferRequest;

/// Request a framebuffer from Limine.
///
/// Limine discovers this request through the `.limine_requests` linker
/// section.
#[used]
#[unsafe(link_section = ".limine_requests")]
static LIMINE_FRAMEBUFFER_REQUEST: FramebufferRequest = FramebufferRequest::new();

// ============================================================================
// QEMU DEBUG CONSOLE
// ============================================================================
//
// QEMU provides a simple debugging interface through I/O port 0xE9.
//
// If QEMU is started with:
//
//     -debugcon stdio
//
// anything written to port 0xE9 is displayed in the terminal running QEMU.
//
// This lets us debug the keyboard at a lower level than the framebuffer
// terminal.

/// Write one byte to QEMU's debug console.
///
/// # Safety
///
/// Accessing x86 I/O ports is inherently unsafe because the compiler cannot
/// verify that the selected hardware port is valid. This function is only
/// used on the x86_64 machine/emulator running Oxide.
#[inline(always)]
fn debug_qemu_byte(byte: u8) {
    unsafe {
        core::arch::asm!(
            // Send the byte in AL to the port specified by DX.
            "out dx, al",

            // QEMU's debug console port.
            in("dx") 0xE9u16,

            // The byte to output.
            in("al") byte,

            // The instruction accesses hardware I/O only.
            options(nomem, nostack, preserves_flags)
        );
    }
}

/// Print an 8-bit value as two hexadecimal characters.
///
/// For example:
///
///     0x1C
///
/// becomes:
///
///     1C
///
/// This is useful because PS/2 keyboard scancodes are normally represented
/// as hexadecimal values.
fn debug_qemu_hex(byte: u8) {
    /// Convert one hexadecimal digit into its ASCII representation.
    #[inline(always)]
    fn hex_digit(value: u8) -> u8 {
        match value {
            // Values 0-9 become ASCII '0'-'9'.
            0..=9 => b'0' + value,

            // Values 10-15 become ASCII 'A'-'F'.
            _ => b'A' + (value - 10),
        }
    }

    // Print the high nibble.
    debug_qemu_byte(hex_digit(byte >> 4));

    // Print the low nibble.
    debug_qemu_byte(hex_digit(byte & 0x0F));
}

// ============================================================================
// RAW PS/2 DEBUGGING
// ============================================================================
//
// A traditional PC-compatible keyboard communicates through a PS/2
// controller.
//
// Important ports:
//
//     0x60 = data port
//     0x64 = status/command port
//
// The status register contains an Output Buffer Status flag at bit 0:
//
//     0 = output buffer empty
//     1 = output buffer contains data
//
// If pressing a key causes bit 0 to become set, we can read the corresponding
// raw byte from port 0x60.
//
// This diagnostic exists specifically to determine whether the problem is:
//
//     keyboard/controller
//             ↓
//          driver
//             ↓
//          terminal
//
// NOTE:
// The raw diagnostic consumes the byte from port 0x60. Therefore the normal
// keyboard driver will not see bytes consumed by this diagnostic.
//
// That is intentional for this temporary debugging stage.

/// Read the PS/2 controller status register.
///
/// The status register is located at I/O port 0x64.
#[inline(always)]
fn debug_ps2_status() -> u8 {
    let mut status: u8;

    unsafe {
        core::arch::asm!(
            // Read one byte from the I/O port in DX.
            "in al, dx",

            // PS/2 controller status port.
            in("dx") 0x64u16,

            // Store the returned byte here.
            out("al") status,

            // This instruction performs hardware I/O only.
            options(nomem, nostack, preserves_flags)
        );
    }

    status
}

/// Read one raw byte from the PS/2 controller.
///
/// The caller should only use this after confirming that the controller's
/// output buffer contains data.
#[inline(always)]
fn debug_ps2_read_data() -> u8 {
    let mut data: u8;

    unsafe {
        core::arch::asm!(
            // Read one byte from the I/O port in DX.
            "in al, dx",

            // PS/2 data port.
            in("dx") 0x60u16,

            // Store the returned byte here.
            out("al") data,

            // Hardware I/O only.
            options(nomem, nostack, preserves_flags)
        );
    }

    data
}

/// Print a raw PS/2 byte to QEMU's debug console.
///
/// Example:
///
///     [PS2] 1C
///
/// This makes it obvious which values came from the PS/2 controller.
fn debug_print_ps2_byte(byte: u8) {
    debug_qemu_byte(b'[');
    debug_qemu_byte(b'P');
    debug_qemu_byte(b'S');
    debug_qemu_byte(b'2');
    debug_qemu_byte(b']');
    debug_qemu_byte(b' ');

    // Print the raw byte as hexadecimal.
    debug_qemu_hex(byte);

    // End the diagnostic line.
    debug_qemu_byte(b'\r');
    debug_qemu_byte(b'\n');
}

/// Print a message showing that the PS/2 diagnostic has started.
///
/// This is useful because if we see this message but no `[PS2]` messages,
/// then the kernel is running while the PS/2 controller is not providing
/// keyboard bytes.
fn debug_print_startup() {
    debug_qemu_byte(b'\r');
    debug_qemu_byte(b'\n');

    debug_qemu_byte(b'=');
    debug_qemu_byte(b'=');
    debug_qemu_byte(b'=');
    debug_qemu_byte(b' ');

    debug_qemu_byte(b'O');
    debug_qemu_byte(b'X');
    debug_qemu_byte(b'I');
    debug_qemu_byte(b'D');
    debug_qemu_byte(b'E');

    debug_qemu_byte(b' ');

    debug_qemu_byte(b'P');
    debug_qemu_byte(b'S');
    debug_qemu_byte(b'2');

    debug_qemu_byte(b' ');

    debug_qemu_byte(b'D');
    debug_qemu_byte(b'E');
    debug_qemu_byte(b'B');
    debug_qemu_byte(b'U');
    debug_qemu_byte(b'G');

    debug_qemu_byte(b' ');

    debug_qemu_byte(b'=');
    debug_qemu_byte(b'=');
    debug_qemu_byte(b'=');

    debug_qemu_byte(b'\r');
    debug_qemu_byte(b'\n');
}

// ============================================================================
// KERNEL ENTRY POINT
// ============================================================================

/// Main Oxide kernel entry point.
///
/// The bootloader jumps here after loading the kernel.
///
/// This function never returns because a kernel cannot return to a caller in
/// the same way a normal userspace application can.
#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    // ------------------------------------------------------------------------
    // GDT INITIALIZATION
    // ------------------------------------------------------------------------
    //
    // Initialize the existing Global Descriptor Table.
    gdt::init();

    // ------------------------------------------------------------------------
    // FRAMEBUFFER INITIALIZATION
    // ------------------------------------------------------------------------
    //
    // The Limine request itself is not the framebuffer.
    //
    // `.response()` retrieves the response generated by Limine.
    //
    // In the version of the Limine crate used by Oxide, `.framebuffers()`
    // returns a slice rather than an iterator.
    //
    // Therefore we use array indexing instead of `.next()`.

    let framebuffer_response = LIMINE_FRAMEBUFFER_REQUEST
        .response()
        .expect("Limine did not provide a framebuffer");

    // Get the first framebuffer returned by Limine.
    //
    // `.first()` works because `framebuffers()` returns a slice.
    let framebuffer = framebuffer_response
        .framebuffers()
        .first()
        .copied()
        .expect("Limine framebuffer response contained no framebuffer");

    // Give the actual framebuffer to the existing terminal subsystem.
    terminal::init(framebuffer);

    // ------------------------------------------------------------------------
    // KEYBOARD INITIALIZATION
    // ------------------------------------------------------------------------
    //
    // Initialize the existing keyboard subsystem.
    keyboard::init();

    // ------------------------------------------------------------------------
    // INITIAL TERMINAL PROMPT
    // ------------------------------------------------------------------------
    //
    // This confirms that the kernel successfully reached the main loop.
    terminal::print("OXIDE> ");

    // ------------------------------------------------------------------------
    // QEMU PS/2 DEBUG STARTUP MESSAGE
    // ------------------------------------------------------------------------
    //
    // This message is invisible on the Oxide framebuffer.
    //
    // It appears in the terminal where QEMU was launched when `-debugcon
    // stdio` is used.
    debug_print_startup();

    // ========================================================================
    // MAIN KERNEL LOOP
    // ========================================================================
    //
    // Oxide currently uses polling for keyboard input.
    //
    // Eventually we can replace this with a proper interrupt-driven input
    // system using IRQ1 and the IDT.
    //
    // For now, the goal is simply to prove that keyboard bytes are arriving.

    loop {
        // --------------------------------------------------------------------
        // RAW PS/2 DIAGNOSTIC
        // --------------------------------------------------------------------
        //
        // Read the current PS/2 controller status.
        let ps2_status = debug_ps2_status();

        // Bit 0 is the Output Buffer Status flag.
        //
        // If it is set, port 0x60 contains a byte waiting for us.
        if ps2_status & 0x01 != 0 {
            // Read the raw byte from the controller.
            let raw_byte = debug_ps2_read_data();

            // Send that raw byte to QEMU's debug console.
            debug_print_ps2_byte(raw_byte);
        }

        // --------------------------------------------------------------------
        // EXISTING KEYBOARD DRIVER
        // --------------------------------------------------------------------
        //
        // Keep the existing keyboard decoder running.
        //
        // During this diagnostic, the raw reader above may consume the
        // controller byte first. That is expected.
        //
        // Once we determine exactly what the controller is sending, this
        // diagnostic reader can be removed and the proper driver can be
        // fixed without changing unrelated kernel code.

        if let Some(character) = keyboard::poll() {
            // ----------------------------------------------------------------
            // BACKSPACE
            // ----------------------------------------------------------------
            if character == '\x08' {
                terminal::backspace();

            // ----------------------------------------------------------------
            // ENTER
            // ----------------------------------------------------------------
            } else if character == '\n' {
                terminal::println("");
                terminal::print("OXIDE> ");

            // ----------------------------------------------------------------
            // TAB
            // ----------------------------------------------------------------
            } else if character == '\t' {
                terminal::print("    ");

            // ----------------------------------------------------------------
            // NORMAL CHARACTER
            // ----------------------------------------------------------------
            } else {
                // A Rust `char` can require up to four bytes in UTF-8.
                let mut buffer = [0u8; 4];

                // Encode the character into UTF-8.
                let text = character.encode_utf8(&mut buffer);

                // Send the encoded text to the existing terminal.
                terminal::print(text);
            }
        }

        // --------------------------------------------------------------------
        // CPU SPIN HINT
        // --------------------------------------------------------------------
        //
        // The kernel is currently using a polling loop.
        //
        // `spin_loop()` tells the CPU that this is an intentional busy-wait.
        core::hint::spin_loop();
    }
}

// ============================================================================
// PANIC HANDLER
// ============================================================================
//
// Because Oxide is a `no_std` kernel, we provide our own panic handler.

/// Handles unrecoverable kernel panics.
///
/// The current behavior is to display the panic location and permanently
/// halt the CPU.
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    // Add a blank line before the panic information.
    terminal::println("");

    // Make the fatal error obvious.
    terminal::println("!!! KERNEL PANIC !!!");

    // Display the source location when Rust provides it.
    if let Some(location) = info.location() {
        // Print the source file.
        terminal::print("File: ");
        terminal::print(location.file());

        // Print the line number.
        terminal::print(" Line: ");
        terminal::print_u64(location.line() as u64);

        // Print the column number.
        terminal::print(" Column: ");
        terminal::print_u64(location.column() as u64);

        terminal::println("");
    }

    // Tell the user that the kernel has stopped.
    terminal::println("Kernel halted.");

    // Disable interrupts before permanently halting.
    unsafe {
        core::arch::asm!("cli");
    }

    // Permanently halt the processor.
    loop {
        unsafe {
            // Stop the CPU until an interrupt arrives.
            //
            // Interrupts are disabled, so the CPU stays halted.
            core::arch::asm!("hlt");
        }
    }
}