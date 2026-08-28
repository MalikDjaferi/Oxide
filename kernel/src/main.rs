#![no_std]
#![no_main]

// We use bootloader_api to receive control from the bootloader and
// obtain information about the machine that was discovered during boot.
use bootloader_api::{entry_point, BootInfo};

// Declare the function that the bootloader should call when it
// finishes setting up the machine and transfers control to Oxide.
entry_point!(kernel_main);

/// Oxide's kernel entry point.
///
/// The bootloader calls this function after it has loaded the kernel,
/// configured the CPU environment, created the memory map, and prepared
/// the `BootInfo` structure.
///
/// `BootInfo` will eventually give Oxide access to important hardware
/// information such as:
///
/// - The physical memory map
/// - The framebuffer
/// - Bootloader configuration
/// - Other information discovered during the boot process
///
/// For now, we deliberately do nothing with `BootInfo`. The goal is to
/// first establish a completely reliable kernel entry point before we
/// start interacting with hardware.
fn kernel_main(_boot_info: &'static mut BootInfo) -> ! {
    // The kernel has successfully received control from the bootloader.
    //
    // We intentionally do not touch memory, the framebuffer, or any
    // other hardware here yet. This gives us the smallest possible
    // kernel and makes debugging early boot failures much easier.
    loop {
        // `spin_loop()` tells the CPU that this loop is intentionally
        // waiting and prevents us from performing completely pointless
        // work on every iteration.
        core::hint::spin_loop();
    }
}

/// Kernel panic handler.
///
/// Normal Rust programs can unwind or terminate after a panic. A
/// `no_std` kernel has no operating-system runtime available to handle
/// a panic, so Oxide must provide its own panic handler.
///
/// Until Oxide has a proper panic screen/logger, we simply stop here.
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    // A panic is unrecoverable at this stage of the kernel.
    //
    // Keep the CPU here rather than returning, because the panic handler
    // has the `!` (never) return type.
    loop {
        // Give the CPU a hint that this is an intentional waiting loop.
        core::hint::spin_loop();
    }
}