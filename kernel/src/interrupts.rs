//! ============================================================================
//! OXIDE INTERRUPT SYSTEM
//! ============================================================================
//!
//! This module contains the earliest x86_64 interrupt infrastructure for Oxide.
//!
//! Devices communicate with the CPU using interrupts.
//!
//! Example:
//!
//!     Keyboard
//!        |
//!        | IRQ 1
//!        v
//!     Legacy 8259 PIC
//!        |
//!        | interrupt vector 33
//!        v
//!     CPU
//!        |
//!        v
//!     Oxide Interrupt Descriptor Table
//!
//! Before hardware interrupts can be enabled, Oxide must:
//!
//!     1. Create an Interrupt Descriptor Table.
//!     2. Install handlers for expected hardware interrupts.
//!     3. Remap and initialize the legacy PIC.
//!     4. Tell the CPU to use the IDT.
//!     5. Enable interrupts.
//!
//! At this stage we install only a timer handler. This prevents the periodic
//! hardware timer interrupt from immediately causing an unhandled-interrupt
//! exception after interrupts are enabled.
//!
//! The next step will add the keyboard handler.
//!
//! ============================================================================

use core::sync::atomic::{AtomicBool, Ordering};

use pic8259::ChainedPics;

use x86_64::instructions::interrupts;
use x86_64::structures::idt::{
    InterruptDescriptorTable,
    InterruptStackFrame,
};

// -----------------------------------------------------------------------------
// PIC INTERRUPT OFFSETS
// -----------------------------------------------------------------------------

/// The first interrupt vector assigned to the legacy PIC.
///
/// CPU exceptions use vectors 0 through 31.
///
/// Hardware interrupts therefore begin at vector 32 so they do not overlap with
/// CPU exceptions.
pub const PIC_1_OFFSET: u8 = 32;

/// The second legacy PIC begins eight vectors after the first.
///
/// PIC 1 handles IRQs 0 through 7.
/// PIC 2 handles IRQs 8 through 15.
pub const PIC_2_OFFSET: u8 = PIC_1_OFFSET + 8;

// -----------------------------------------------------------------------------
// PIC IRQ NUMBERS
// -----------------------------------------------------------------------------

/// The hardware timer is connected to IRQ 0.
const TIMER_INTERRUPT_INDEX: u8 = PIC_1_OFFSET;

/// The keyboard is connected to IRQ 1.
///
/// We define this now even though the actual keyboard handler will be added in
/// the next stage.
pub const KEYBOARD_INTERRUPT_INDEX: u8 = PIC_1_OFFSET + 1;

// -----------------------------------------------------------------------------
// LEGACY PIC CONTROLLER
// -----------------------------------------------------------------------------

/// Global access to the two chained legacy 8259 PIC controllers.
///
/// The PIC is hardware state shared by the entire kernel.
///
/// Access is unsafe because hardware state is global. Oxide currently runs as
/// a very early single-core kernel, so this is sufficient for now.
///
/// Future versions will move toward APIC interrupt controllers.
static mut PICS: ChainedPics = unsafe {
    ChainedPics::new(PIC_1_OFFSET, PIC_2_OFFSET)
};

// -----------------------------------------------------------------------------
// INITIALIZATION STATE
// -----------------------------------------------------------------------------

/// Records whether the interrupt system has already been initialized.
static INITIALIZED: AtomicBool = AtomicBool::new(false);

// -----------------------------------------------------------------------------
// INTERRUPT DESCRIPTOR TABLE
// -----------------------------------------------------------------------------

/// Creates the complete Interrupt Descriptor Table used by Oxide.
///
/// A function is used instead of manually creating a mutable static table.
/// This allows us to install the interrupt handlers while the table is being
/// constructed.
fn create_idt() -> InterruptDescriptorTable {
    let mut idt = InterruptDescriptorTable::new();

    // Install the periodic timer interrupt handler.
    //
    // Without this handler, enabling interrupts would allow the first timer
    // interrupt to reach an empty IDT entry and crash the kernel.
    idt[TIMER_INTERRUPT_INDEX as usize]
        .set_handler_fn(timer_interrupt_handler);

    idt
}

/// Global Interrupt Descriptor Table.
///
/// The table is created once and remains in memory for the entire lifetime of
/// the kernel.
static mut IDT: Option<InterruptDescriptorTable> = None;

// -----------------------------------------------------------------------------
// INTERRUPT HANDLERS
// -----------------------------------------------------------------------------

/// Handles the periodic hardware timer interrupt.
///
/// The timer fires repeatedly in the background.
///
/// We do not use the timer for scheduling yet. The current purpose is simply
/// to prove that hardware interrupts are working and to correctly acknowledge
/// the interrupt so the PIC can deliver future interrupts.
extern "x86-interrupt" fn timer_interrupt_handler(
    _stack_frame: InterruptStackFrame,
) {
    // Tell the legacy PIC that the interrupt was handled.
    //
    // Without this acknowledgement, the PIC may stop delivering future
    // interrupts from the same interrupt line.
    unsafe {
        PICS.notify_end_of_interrupt(TIMER_INTERRUPT_INDEX);
    }
}

// -----------------------------------------------------------------------------
// INITIALIZATION
// -----------------------------------------------------------------------------

/// Initializes Oxide's early interrupt system.
///
/// Initialization order is critical:
///
///     1. Disable CPU interrupts.
///     2. Create the IDT.
///     3. Load the IDT.
///     4. Initialize and remap the PIC.
///     5. Enable CPU interrupts.
///
/// This order ensures the CPU always has a valid interrupt handler before
/// external hardware interrupts are allowed to arrive.
pub fn init() {
    // Prevent hardware interrupts while initialization is in progress.
    interrupts::disable();

    // Prevent accidental double initialization.
    if INITIALIZED.swap(true, Ordering::SeqCst) {
        return;
    }

    unsafe {
        // Build the IDT containing the handlers required before interrupts are
        // enabled.
        IDT = Some(create_idt());

        // Load the IDT into the CPU.
        //
        // The table lives in the global static above, so its memory remains
        // valid after this function returns.
        IDT.as_ref()
            .expect("Oxide IDT was not initialized")
            .load();

        // Initialize the remapped legacy PIC pair.
        PICS.initialize();
    }

    // All required interrupt infrastructure is now active.
    //
    // The CPU may now receive hardware interrupts.
    interrupts::enable();
}

/// Returns whether the interrupt system has already been initialized.
pub fn is_initialized() -> bool {
    INITIALIZED.load(Ordering::SeqCst)
}
