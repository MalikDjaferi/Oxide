#![no_std]
#![no_main]

// Oxide does not use the normal Rust standard library because
// there is no operating system underneath the kernel providing
// things like files, threads, heap allocation, or console I/O.
//
// We also disable Rust's normal main function because the kernel
// is entered directly by the bootloader.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn _start() -> ! {
    // Limine has successfully loaded the Oxide kernel and has
    // transferred CPU control to our entry point.
    //
    // For now, we deliberately do not access hardware, memory
    // maps, framebuffers, or interrupts. Keeping this first path
    // extremely small makes early boot debugging much easier.
    loop {
        // Tell the CPU that this is an intentional waiting loop.
        //
        // This prevents the processor from continuously executing
        // pointless instructions while the kernel has nothing else
        // to do yet.
        core::hint::spin_loop();
    }
}

// A kernel has no operating-system runtime to handle a panic.
//
// If something goes catastrophically wrong, there is currently
// nowhere to display an error or recover to, so Oxide simply
// stops executing here.
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    // Keep the CPU in an intentional waiting loop after a panic.
    loop {
        // Avoid wasting CPU resources while halted.
        core::hint::spin_loop();
    }
}