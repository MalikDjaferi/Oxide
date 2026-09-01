//! ============================================================================
//! OXIDE GLOBAL DESCRIPTOR TABLE
//! ============================================================================
//!
//! This module initializes the x86_64 Global Descriptor Table (GDT) and the
//! Task State Segment (TSS).
//!
//! The GDT contains:
//!
//! - Kernel code segment
//! - Kernel data segment
//! - Task State Segment
//!
//! The TSS provides a dedicated stack for critical CPU exceptions such as
//! double faults.
//!
//! This module is intentionally kept simple because Oxide is still in its
//! early kernel-development stage.
//!
//! ============================================================================

use core::mem::MaybeUninit;

use x86_64::instructions::segmentation::{Segment, CS, SS};
use x86_64::instructions::tables::load_tss;
use x86_64::structures::gdt::{
    Descriptor,
    GlobalDescriptorTable,
    SegmentSelector,
};
use x86_64::structures::tss::TaskStateSegment;
use x86_64::VirtAddr;

// ============================================================================
// DOUBLE FAULT STACK
// ============================================================================
//
// The CPU stack grows downward.
//
// Therefore, the address immediately after this array is used as the initial
// stack pointer.
//
// ============================================================================

const DOUBLE_FAULT_STACK_SIZE: usize = 4096 * 5;

/// Dedicated stack for future double-fault handling.
static mut DOUBLE_FAULT_STACK: [u8; DOUBLE_FAULT_STACK_SIZE] =
    [0; DOUBLE_FAULT_STACK_SIZE];

// ============================================================================
// STATIC TSS STORAGE
// ============================================================================
//
// The TSS needs to remain alive for the entire lifetime of the kernel.
//
// MaybeUninit allows us to reserve the storage statically and initialize it
// exactly once during gdt::init().
//
// ============================================================================

static mut TSS: MaybeUninit<TaskStateSegment> =
    MaybeUninit::uninit();

// ============================================================================
// GLOBAL DESCRIPTOR TABLE
// ============================================================================
//
// The CPU continues using the GDT after init() returns, so it must remain
// alive permanently.
//
// ============================================================================

static mut GDT: Option<GlobalDescriptorTable> = None;

// ============================================================================
// SEGMENT SELECTORS
// ============================================================================

static mut CODE_SELECTOR: Option<SegmentSelector> = None;
static mut DATA_SELECTOR: Option<SegmentSelector> = None;
static mut TSS_SELECTOR: Option<SegmentSelector> = None;

// ============================================================================
// INITIALIZATION
// ============================================================================

/// Initialize the Oxide Global Descriptor Table.
///
/// The initialization sequence is:
///
/// 1. Create the TSS.
/// 2. Configure the double-fault stack.
/// 3. Create the GDT.
/// 4. Add the kernel code segment.
/// 5. Add the kernel data segment.
/// 6. Add the TSS descriptor.
/// 7. Store the GDT and selectors.
/// 8. Load the GDT.
/// 9. Reload CS and SS.
/// 10. Load the TSS.
///
/// This function must only be called once during kernel initialization.
// ============================================================================

pub fn init() {
    // ========================================================================
    // GET RAW POINTER TO TSS STORAGE
    // ========================================================================
    //
    // Rust 2024 forbids creating normal references to static mut variables.
    //
    // `&raw mut` creates a raw pointer directly without creating an intermediate
    // mutable reference.
    //
    // ========================================================================

    let tss_ptr: *mut TaskStateSegment = unsafe {
        (&raw mut TSS).cast::<TaskStateSegment>()
    };

    // ========================================================================
    // INITIALIZE THE TSS
    // ========================================================================
    //
    // SAFETY:
    //
    // tss_ptr points to the statically allocated TSS storage.
    // The storage is currently uninitialized and is written exactly once.
    //
    // ========================================================================

    unsafe {
        tss_ptr.write(TaskStateSegment::new());
    }

    // ========================================================================
    // CONFIGURE DOUBLE-FAULT STACK
    // ========================================================================
    //
    // The TSS contains seven interrupt stack table entries:
    //
    //     0 through 6
    //
    // Entry 0 is reserved for the double-fault handler.
    //
    // ========================================================================

    let stack_start: VirtAddr = unsafe {
        // `&raw const` creates a raw pointer without creating a shared
        // reference to the mutable static.
        let stack_ptr: *const [u8; DOUBLE_FAULT_STACK_SIZE] =
            &raw const DOUBLE_FAULT_STACK;

        VirtAddr::from_ptr(stack_ptr)
    };

    // The stack grows downward, so the initial stack pointer is immediately
    // after the end of the stack allocation.
    let stack_end =
        stack_start + DOUBLE_FAULT_STACK_SIZE as u64;

    // ========================================================================
    // WRITE DOUBLE-FAULT STACK INTO TSS
    // ========================================================================

    unsafe {
        (*tss_ptr).interrupt_stack_table[0] = stack_end;
    }

    // ========================================================================
    // CREATE GLOBAL DESCRIPTOR TABLE
    // ========================================================================

    let mut gdt = GlobalDescriptorTable::new();

    // ========================================================================
    // KERNEL CODE SEGMENT
    // ========================================================================

    let code_selector =
        gdt.append(Descriptor::kernel_code_segment());

    // ========================================================================
    // KERNEL DATA SEGMENT
    // ========================================================================

    let data_selector =
        gdt.append(Descriptor::kernel_data_segment());

    // ========================================================================
    // TASK STATE SEGMENT
    // ========================================================================
    //
    // The GDT needs a reference to the TSS.
    //
    // We create the reference from the raw pointer only after initialization.
    //
    // The TSS lives in static storage and will never be moved or destroyed,
    // so this reference remains valid for the lifetime of the kernel.
    //
    // ========================================================================

    let tss_selector = unsafe {
        let tss_ref: &'static TaskStateSegment =
            &*tss_ptr;

        gdt.append(Descriptor::tss_segment(tss_ref))
    };

    // ========================================================================
    // STORE GDT
    // ========================================================================
    //
    // The GDT must remain alive after this function returns.
    //
    // ========================================================================

    unsafe {
        (&raw mut GDT).write(Some(gdt));

        (&raw mut CODE_SELECTOR).write(Some(code_selector));
        (&raw mut DATA_SELECTOR).write(Some(data_selector));
        (&raw mut TSS_SELECTOR).write(Some(tss_selector));
    }

    // ========================================================================
    // LOAD GDT
    // ========================================================================
    //
    // Get a raw pointer to the global GDT and access it without creating a
    // reference directly from the static mut declaration.
    //
    // ========================================================================

    unsafe {
        let gdt_ptr: *const Option<GlobalDescriptorTable> =
            &raw const GDT;

        if let Some(gdt) = &*gdt_ptr {
            gdt.load();
        }
    }

    // ========================================================================
    // RELOAD CODE SEGMENT
    // ========================================================================
    //
    // x86_64 crate 0.15.x uses CS::set_reg().
    //
    // ========================================================================

    unsafe {
        let selector_ptr: *const Option<SegmentSelector> =
            &raw const CODE_SELECTOR;

        if let Some(selector) = *selector_ptr {
            CS::set_reg(selector);
        }
    }

    // ========================================================================
    // RELOAD DATA SEGMENT
    // ========================================================================

    unsafe {
        let selector_ptr: *const Option<SegmentSelector> =
            &raw const DATA_SELECTOR;

        if let Some(selector) = *selector_ptr {
            SS::set_reg(selector);
        }
    }

    // ========================================================================
    // LOAD TASK STATE SEGMENT
    // ========================================================================

    unsafe {
        let selector_ptr: *const Option<SegmentSelector> =
            &raw const TSS_SELECTOR;

        if let Some(selector) = *selector_ptr {
            load_tss(selector);
        }
    }
}