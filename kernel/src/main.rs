#![no_std]
#![no_main]

// ============================================================================
// OXIDE KERNEL
// ============================================================================
//
// This file contains the earliest part of the Oxide operating system.
//
// At this stage, the kernel is responsible for:
//
//     1. Being loaded by Limine.
//     2. Receiving the framebuffer supplied by Limine.
//     3. Initializing the Oxide framebuffer terminal.
//     4. Printing the first messages to the graphical display.
//     5. Remaining alive in the kernel main loop.
//
// Oxide does not use VGA text mode here. Instead, it uses the framebuffer
// provided by the bootloader.
//
// Boot flow:
//
//     Firmware
//        |
//        v
//     Limine
//        |
//        | loads oxide-kernel
//        v
//     _start()
//        |
//        | request framebuffer
//        v
//     Limine framebuffer
//        |
//        | initialize
//        v
//     terminal::init()
//        |
//        v
//     Oxide terminal
//
// ============================================================================

mod terminal;

use core::panic::PanicInfo;

use limine::request::FramebufferRequest;

// ============================================================================
// LIMINE FRAMEBUFFER REQUEST
// ============================================================================
//
// This request tells Limine that the Oxide kernel wants access to a graphical
// framebuffer.
//
// Limine will process this request while booting the kernel and provide a
// response containing one or more available framebuffers.
//
// The framebuffer contains:
//
//     - Physical/virtual address
//     - Width
//     - Height
//     - Pitch
//     - Bits per pixel
//     - Pixel format information
//
// The terminal renderer in terminal.rs uses this information to draw text
// directly into the framebuffer.
//
// ============================================================================

#[used]
#[unsafe(link_section = ".limine_requests")]
static LIMINE_FRAMEBUFFER_REQUEST: FramebufferRequest =
    FramebufferRequest::new();

// ============================================================================
// KERNEL ENTRY POINT
// ============================================================================
//
// This is the first Rust function executed after Limine transfers control to
// the Oxide kernel.
//
// The function uses:
//
//     #[unsafe(no_mangle)]
//
// so that the linker can find the symbol using the exact name "_start".
//
// It is declared:
//
//     pub extern "C" fn _start() -> !
//
// because the kernel entry point follows the C calling convention and never
// returns.
//
// ============================================================================

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    // ------------------------------------------------------------------------
    // EARLY BOOT DIAGNOSTICS
    // ------------------------------------------------------------------------
    //
    // These messages are sent through QEMU's debug console using port 0xE9.
    //
    // They are intentionally performed before the graphical terminal is
    // initialized.
    //
    // This means that even if framebuffer rendering is broken, we can still
    // determine whether the kernel itself is actually executing.
    //

    debug_print("\n");

    debug_print("========================================\n");
    debug_print("        OXIDE KERNEL STARTED\n");
    debug_print("========================================\n");

    // ------------------------------------------------------------------------
    // REQUEST FRAMEBUFFER RESPONSE
    // ------------------------------------------------------------------------
    //
    // Ask Limine for the response to our framebuffer request.
    //
    // If there is no response, the bootloader did not provide the requested
    // framebuffer information.
    //
    // At this point there is no safe way to initialize our graphical
    // terminal, so we print the failure to the debug console and halt.
    //

    debug_print("OXIDE: requesting framebuffer...\n");

    let Some(response) = LIMINE_FRAMEBUFFER_REQUEST.response() else {
        debug_print("OXIDE: NO FRAMEBUFFER RESPONSE\n");
        debug_print("OXIDE: kernel execution works!\n");

        halt();
    };

    debug_print("OXIDE: framebuffer response received!\n");

    // ------------------------------------------------------------------------
    // FIND FIRST FRAMEBUFFER
    // ------------------------------------------------------------------------
    //
    // Limine can provide multiple framebuffers.
    //
    // For the initial Oxide terminal we simply use the first framebuffer
    // available.
    //
    // If Limine returned a response but the framebuffer list is empty, we
    // cannot initialize the graphical terminal.
    //

    let Some(framebuffer) = response.framebuffers().first() else {
        debug_print("OXIDE: framebuffer list is empty\n");
        debug_print("OXIDE: kernel execution works!\n");

        halt();
    };

    debug_print("OXIDE: framebuffer found!\n");

    // ------------------------------------------------------------------------
    // PRINT FRAMEBUFFER INFORMATION
    // ------------------------------------------------------------------------
    //
    // These values are extremely useful when debugging graphical output.
    //
    // Example:
    //
    //     width  = 1280
    //     height = 800
    //     bpp    = 32
    //     pitch  = 5120
    //
    // A 1280x800 framebuffer with 32 bits per pixel normally has a pitch of
    // 1280 * 4 = 5120 bytes per row.
    //
    // We print these values before touching framebuffer memory.
    //

    debug_print("OXIDE: width  = ");
    debug_print_u64(framebuffer.width);
    debug_print("\n");

    debug_print("OXIDE: height = ");
    debug_print_u64(framebuffer.height);
    debug_print("\n");

    debug_print("OXIDE: bpp    = ");
    debug_print_u64(framebuffer.bpp as u64);
    debug_print("\n");

    debug_print("OXIDE: pitch  = ");
    debug_print_u64(framebuffer.pitch);
    debug_print("\n");

    // ------------------------------------------------------------------------
    // BOOT DIAGNOSTIC COMPLETE
    // ------------------------------------------------------------------------
    //
    // Reaching this point means:
    //
    //     - Limine loaded the kernel.
    //     - _start() executed.
    //     - The Limine framebuffer request worked.
    //     - A framebuffer was found.
    //     - The framebuffer properties can be read.
    //
    // The next step is to actually give this framebuffer to the Oxide
    // terminal renderer.
    //

    debug_print("----------------------------------------\n");
    debug_print("OXIDE: BOOT DIAGNOSTIC PASSED\n");
    debug_print("OXIDE: _start() is running correctly.\n");
    debug_print("----------------------------------------\n");

    // ========================================================================
    // INITIALIZE OXIDE TERMINAL
    // ========================================================================
    //
    // This is where the framebuffer finally becomes useful to the kernel.
    //
    // terminal::init() performs the initial terminal setup:
    //
    //     - Stores the framebuffer address.
    //     - Stores the framebuffer pitch.
    //     - Stores the framebuffer width.
    //     - Stores the framebuffer height.
    //     - Clears the screen.
    //     - Resets the terminal cursor.
    //
    // After this call, terminal::print() and terminal::println() can draw
    // characters directly onto the graphical framebuffer.
    //
    // This uses the existing terminal implementation in:
    //
    //     kernel/src/terminal.rs
    //
    // ========================================================================

    terminal::init(framebuffer);

    // ========================================================================
    // OXIDE TERMINAL STARTUP
    // ========================================================================
    //
    // The framebuffer terminal is now active.
    //
    // Everything below this point is rendered onto the QEMU graphical display
    // instead of the QEMU debug console.
    //
    // This gives Oxide its first actual graphical user-facing output.
    //
    // ========================================================================

    terminal::println("========================================");
    terminal::println("           OXIDE KERNEL");
    terminal::println("========================================");

    terminal::println("");

    terminal::println("Framebuffer initialized.");
    terminal::println("Terminal renderer online.");

    terminal::println("");

    terminal::println("Welcome to Oxide.");

    terminal::println("");

    // ------------------------------------------------------------------------
    // DISPLAY FRAMEBUFFER RESOLUTION
    // ------------------------------------------------------------------------
    //
    // Demonstrate that the terminal can also print numbers.
    //
    // The current terminal implementation provides print_u64(), which
    // converts an unsigned 64-bit integer into decimal text.
    //

    terminal::print("Resolution: ");

    terminal::print_u64(framebuffer.width);

    terminal::print("x");

    terminal::print_u64(framebuffer.height);

    terminal::println("");

    // ------------------------------------------------------------------------
    // DISPLAY COLOR DEPTH
    // ------------------------------------------------------------------------
    //
    // Show the framebuffer's bits-per-pixel value.
    //
    // QEMU is currently providing a 32-bit framebuffer, which is the format
    // supported by the current Oxide terminal renderer.
    //

    terminal::print("Framebuffer: ");

    terminal::print_u64(framebuffer.bpp as u64);

    terminal::println("-bit");

    terminal::println("");

    // ------------------------------------------------------------------------
    // INITIAL COMMAND PROMPT
    // ------------------------------------------------------------------------
    //
    // This is not a real shell yet.
    //
    // It is simply the first visual representation of what will eventually
    // become the Oxide command-line interface.
    //
    // Keyboard input will later be connected to this terminal so that the
    // prompt can actually accept commands.
    //

    terminal::println("OXIDE> _");

    // ========================================================================
    // KERNEL MAIN LOOP
    // ========================================================================
    //
    // The kernel entry point must never return.
    //
    // For now Oxide does not yet have:
    //
    //     - Interrupt handling
    //     - Keyboard drivers
    //     - Timers
    //     - Processes
    //     - Scheduler
    //     - Userspace
    //     - Shell
    //
    // Therefore we simply keep the CPU alive.
    //
    // core::hint::spin_loop() tells the processor that this is an intentional
    // busy-wait loop.
    //
    // Later this will be replaced by an actual kernel event loop or a halt
    // instruction while waiting for interrupts.
    //

    loop {
        core::hint::spin_loop();
    }
}

// ============================================================================
// QEMU DEBUG CONSOLE
// ============================================================================
//
// QEMU supports a very useful debugging mechanism through I/O port 0xE9.
//
// When QEMU is launched with:
//
//     -debugcon stdio
//
// bytes written to port 0xE9 appear directly in the terminal from which QEMU
// was launched.
//
// This is extremely useful during early kernel development because it works
// independently of the graphical framebuffer.
//
// ============================================================================

/// Write a string to the QEMU debug console.
///
/// Each byte is sent individually through I/O port 0xE9.
fn debug_print(text: &str) {
    for byte in text.bytes() {
        unsafe {
            core::arch::asm!(
                "out dx, al",
                in("dx") 0xe9u16,
                in("al") byte,
                options(nomem, nostack, preserves_flags)
            );
        }
    }
}

// ============================================================================
// QEMU DEBUG INTEGER OUTPUT
// ============================================================================
//
// The kernel is `#![no_std]`, so we cannot use the normal Rust formatting
// machinery at this early stage.
//
// This function manually converts a u64 into decimal ASCII characters.
//
// Example:
//
//     1280 -> "1280"
//     800  -> "800"
//     32   -> "32"
//
// ============================================================================

/// Print an unsigned 64-bit integer to the QEMU debug console.
fn debug_print_u64(mut value: u64) {
    // Special case for zero because the conversion loop below would otherwise
    // produce an empty result.
    if value == 0 {
        debug_print("0");
        return;
    }

    // A u64 can contain at most 20 decimal digits.
    let mut buffer = [0u8; 20];

    // Start at the end of the buffer and fill it backwards.
    let mut index = buffer.len();

    while value > 0 {
        index -= 1;

        // Extract the final decimal digit.
        buffer[index] = b'0' + (value % 10) as u8;

        // Remove the final digit.
        value /= 10;
    }

    // Send the resulting ASCII characters to QEMU.
    for &byte in &buffer[index..] {
        unsafe {
            core::arch::asm!(
                "out dx, al",
                in("dx") 0xe9u16,
                in("al") byte,
                options(nomem, nostack, preserves_flags)
            );
        }
    }
}

// ============================================================================
// CPU HALT
// ============================================================================
//
// This function is used when the kernel reaches a fatal condition during
// early boot.
//
// Interrupts are disabled and the CPU is placed into the HLT state.
//
// The loop is necessary because _start() is declared as returning `!`, which
// means the function must never return.
//
// ============================================================================

/// Halt the CPU forever.
fn halt() -> ! {
    loop {
        unsafe {
            core::arch::asm!(
                "cli",
                "hlt",
                options(nomem, nostack, preserves_flags)
            );
        }
    }
}

// ============================================================================
// PANIC HANDLER
// ============================================================================
//
// Because Oxide uses `#![no_std]`, there is no standard-library panic handler.
//
// Any Rust panic eventually reaches this function.
//
// Instead of trying to display the panic graphically, we currently send the
// important location information to QEMU's debug console.
//
// This allows us to diagnose kernel crashes even when the framebuffer terminal
// itself is not working.
//
// ============================================================================

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    debug_print("\n");

    debug_print("========================================\n");
    debug_print("          OXIDE KERNEL PANIC\n");
    debug_print("========================================\n");

    // ------------------------------------------------------------------------
    // PANIC LOCATION
    // ------------------------------------------------------------------------
    //
    // PanicInfo can contain the source file, line and column where the panic
    // occurred.
    //

    if let Some(location) = info.location() {
        debug_print("File: ");

        debug_print(location.file());

        debug_print("\nLine: ");

        debug_print_u64(location.line() as u64);

        debug_print("\nColumn: ");

        debug_print_u64(location.column() as u64);

        debug_print("\n");
    }

    debug_print("========================================\n");

    // A panic is fatal at this stage of Oxide development.
    halt();
}