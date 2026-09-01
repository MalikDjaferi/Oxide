//! ============================================================================
//! OXIDE PS/2 KEYBOARD DRIVER
//! ============================================================================
//!
//! This module implements the first keyboard input driver for Oxide.
//!
//! The driver communicates with the traditional PS/2 controller found on
//! PC-compatible systems. QEMU's normal x86 PC machine provides this hardware
//! interface, which makes it useful for early kernel development.
//!
//! Hardware ports used:
//!
//!     0x60 - PS/2 data port
//!     0x64 - PS/2 controller status/command port
//!
//! The current implementation uses POLLING rather than interrupts.
//!
//! The kernel repeatedly calls:
//!
//!     keyboard::poll()
//!
//! If the controller has keyboard data waiting, the driver reads the raw
//! scancode, decodes it, and returns an ASCII/control character.
//!
//! Current supported input includes:
//!
//!     Letters
//!     Number row
//!     Symbols
//!     Space
//!     Enter
//!     Backspace
//!     Tab
//!     Shift
//!     Caps Lock
//!     Escape
//!
//! The driver currently understands PS/2 Scan Code Set 1, which is the
//! traditional scan-code format used by PC-compatible firmware and commonly
//! exposed by QEMU's emulated PS/2 keyboard.
//!
//! ============================================================================

#![allow(dead_code)]

// ============================================================================
// PS/2 CONTROLLER PORTS
// ============================================================================

/// PS/2 controller data port.
///
/// Reading from port 0x60 retrieves a byte from the controller's output
/// buffer. For keyboard input, this byte is normally a keyboard scancode.
const DATA_PORT: u16 = 0x60;

/// PS/2 controller status/command port.
///
/// Reading port 0x64 returns the controller status register.
const STATUS_PORT: u16 = 0x64;

// ============================================================================
// PS/2 STATUS FLAGS
// ============================================================================

/// Bit 0 of the PS/2 controller status register.
///
/// When this bit is set, the controller's output buffer contains data that
/// can be read from port 0x60.
const STATUS_OUTPUT_BUFFER_FULL: u8 = 1 << 0;

/// Bit 5 of the PS/2 controller status register.
///
/// When this bit is set, the byte in the output buffer came from the
/// auxiliary PS/2 device, normally the mouse.
///
/// We do not support mouse input yet, so such bytes are discarded.
const STATUS_AUXILIARY_DATA: u8 = 1 << 5;

// ============================================================================
// KEYBOARD STATE
// ============================================================================

/// Tracks whether the LEFT Shift key is currently held.
///
/// PS/2 Set 1:
///
///     Left Shift pressed  -> 0x2A
///     Left Shift released -> 0xAA
static mut LEFT_SHIFT_PRESSED: bool = false;

/// Tracks whether the RIGHT Shift key is currently held.
///
/// PS/2 Set 1:
///
///     Right Shift pressed  -> 0x36
///     Right Shift released -> 0xB6
static mut RIGHT_SHIFT_PRESSED: bool = false;

/// Tracks whether Caps Lock is currently enabled.
///
/// Caps Lock behaves as a toggle rather than a held modifier.
static mut CAPS_LOCK: bool = false;

/// Tracks whether the previous scancode was the 0xE0 extended-key prefix.
///
/// Some keyboard keys are represented by two-byte sequences beginning with
/// 0xE0. We do not expose those keys as ASCII yet, but we must consume the
/// complete sequence so that it does not confuse the decoder.
static mut EXTENDED_SCANCODE: bool = false;

/// Tracks whether Ctrl is currently held.
///
/// Ctrl does not currently produce special terminal characters, but keeping
/// track of its state now makes it possible to add proper control-key support
/// later without redesigning the keyboard state system.
static mut CTRL_PRESSED: bool = false;

/// Tracks whether Alt is currently held.
///
/// Alt does not currently produce special terminal characters, but its state
/// is preserved for future shell shortcuts and Alt-based key combinations.
static mut ALT_PRESSED: bool = false;

// ============================================================================
// DRIVER INITIALIZATION
// ============================================================================

/// Initialize the software state of the PS/2 keyboard driver.
///
/// QEMU provides the PS/2 controller for us, so the first version of the
/// driver does not need to perform a full hardware-controller initialization.
///
/// This function simply resets all internal keyboard state.
pub fn init() {
    // SAFETY:
    //
    // Oxide is currently running as a single-threaded kernel and keyboard
    // polling happens from the main execution loop. Therefore these state
    // variables cannot currently be accessed concurrently.
    unsafe {
        LEFT_SHIFT_PRESSED = false;
        RIGHT_SHIFT_PRESSED = false;
        CAPS_LOCK = false;
        EXTENDED_SCANCODE = false;
        CTRL_PRESSED = false;
        ALT_PRESSED = false;
    }
}

// ============================================================================
// LOW-LEVEL PORT ACCESS
// ============================================================================

/// Read one byte from an x86 I/O port.
///
/// The x86 `in` instruction reads data from an I/O port into a CPU register.
///
/// The port number is supplied through DX and the resulting byte is returned
/// through AL.
///
/// This is one of the fundamental hardware-access operations required by an
/// x86 kernel.
#[inline(always)]
unsafe fn read_port(port: u16) -> u8 {
    let value: u8;

    // SAFETY:
    //
    // The caller explicitly requested a raw x86 I/O operation.
    //
    // Port I/O is required here because the PS/2 controller is exposed through
    // the x86 I/O-port address space rather than normal memory.
    unsafe {
        core::arch::asm!(
            "in al, dx",

            // DX contains the I/O port number.
            in("dx") port,

            // AL receives the byte returned by the hardware.
            out("al") value,

            // The instruction does not access normal memory.
            options(nomem, nostack, preserves_flags)
        );
    }

    value
}

// ============================================================================
// READ CONTROLLER STATUS
// ============================================================================

/// Read the current PS/2 controller status register.
#[inline(always)]
fn read_status() -> u8 {
    // SAFETY:
    //
    // Port 0x64 is the standard PS/2 controller status port on x86 PCs.
    unsafe { read_port(STATUS_PORT) }
}

// ============================================================================
// CHECK FOR AVAILABLE DATA
// ============================================================================

/// Return `true` if the PS/2 controller has data waiting.
///
/// Bit 0 of the status register is the Output Buffer Status flag.
#[inline(always)]
fn data_available() -> bool {
    let status = read_status();

    (status & STATUS_OUTPUT_BUFFER_FULL) != 0
}

// ============================================================================
// READ DATA PORT
// ============================================================================

/// Read one byte from the PS/2 data port.
///
/// This function should only be called after `data_available()` reports that
/// the controller has something waiting in its output buffer.
#[inline(always)]
fn read_data() -> u8 {
    // SAFETY:
    //
    // Port 0x60 is the standard PS/2 controller data port.
    unsafe { read_port(DATA_PORT) }
}

// ============================================================================
// PUBLIC POLLING INTERFACE
// ============================================================================

/// Poll the PS/2 controller for keyboard input.
///
/// This function is intentionally NON-BLOCKING.
///
/// If there is no keyboard data available, it immediately returns `None`.
///
/// If a keyboard scancode produces a character, it returns `Some(character)`.
///
/// The shell can therefore repeatedly call this function without becoming
/// stuck waiting for the user to press a key.
pub fn poll() -> Option<char> {
    // Nothing is waiting in the controller output buffer.
    if !data_available() {
        return None;
    }

    // Read the status before consuming the data byte.
    //
    // Bit 5 tells us whether the byte belongs to the auxiliary device
    // (normally a PS/2 mouse) rather than the keyboard.
    let status = read_status();

    // Read the byte regardless of whether it is keyboard or auxiliary data.
    //
    // If we leave an unwanted byte in the output buffer, it can prevent
    // subsequent keyboard bytes from being processed correctly.
    let scancode = read_data();

    // Ignore mouse/auxiliary-device data.
    if (status & STATUS_AUXILIARY_DATA) != 0 {
        return None;
    }

    // Decode the keyboard scancode into an ASCII/control character.
    decode_scancode(scancode)
}

// ============================================================================
// SCANCODE DECODER
// ============================================================================
//
// PS/2 Set 1 uses:
//
//     MAKE CODE  -> key pressed
//     BREAK CODE -> key released
//
// Break codes normally have bit 7 set.
//
// Example:
//
//     A pressed  -> 0x1E
//     A released -> 0x9E
//
// The driver only produces characters for make codes.
//
// Releases are used to update modifier state such as Shift.

// ============================================================================

/// Decode one PS/2 Set 1 scancode.
///
/// Returns `Some(character)` if the scancode represents an ASCII/control
/// character, otherwise returns `None`.
fn decode_scancode(scancode: u8) -> Option<char> {
    // ========================================================================
    // EXTENDED SCANCODE PREFIX
    // ========================================================================
    //
    // 0xE0 indicates that the following byte belongs to an extended key.
    //
    // Examples:
    //
    //     Arrow keys
    //     Insert
    //     Delete
    //     Home
    //     End
    //
    // We do not expose these as terminal characters yet.

    if scancode == 0xE0 {
        // SAFETY:
        //
        // Keyboard input is currently processed synchronously by the main
        // kernel loop.
        unsafe {
            EXTENDED_SCANCODE = true;
        }

        return None;
    }

    // If the previous byte was 0xE0, this byte completes the extended
    // sequence. Consume it for now.
    unsafe {
        if EXTENDED_SCANCODE {
            EXTENDED_SCANCODE = false;
            return None;
        }
    }

    // ========================================================================
    // DETERMINE WHETHER THIS IS A KEY RELEASE
    // ========================================================================

    // In Scan Code Set 1, bit 7 is set on key-release/break codes.
    let released = (scancode & 0x80) != 0;

    // Remove the release bit to recover the original make-code value.
    let key = scancode & 0x7F;

    // ========================================================================
    // LEFT SHIFT
    // ========================================================================
    //
    // Left Shift:
    //
    //     press   -> 0x2A
    //     release -> 0xAA

    if key == 0x2A {
        // SAFETY:
        //
        // The early kernel is currently single-threaded.
        unsafe {
            LEFT_SHIFT_PRESSED = !released;
        }

        return None;
    }

    // ========================================================================
    // RIGHT SHIFT
    // ========================================================================
    //
    // Right Shift:
    //
    //     press   -> 0x36
    //     release -> 0xB6

    if key == 0x36 {
        // SAFETY:
        //
        // The early kernel is currently single-threaded.
        unsafe {
            RIGHT_SHIFT_PRESSED = !released;
        }

        return None;
    }

    // ========================================================================
    // CTRL
    // ========================================================================
    //
    // Left Ctrl:
    //
    //     press   -> 0x1D
    //     release -> 0x9D
    //
    // Ctrl does not currently transform characters. We simply maintain its
    // state so future terminal shortcuts can use it.

    if key == 0x1D {
        // SAFETY:
        //
        // Keyboard state is only accessed from the single-threaded polling
        // loop.
        unsafe {
            CTRL_PRESSED = !released;
        }

        return None;
    }

    // ========================================================================
    // ALT
    // ========================================================================
    //
    // Left Alt:
    //
    //     press   -> 0x38
    //     release -> 0xB8
    //
    // Alt does not currently transform characters.

    if key == 0x38 {
        // SAFETY:
        //
        // Keyboard state is only accessed from the single-threaded polling
        // loop.
        unsafe {
            ALT_PRESSED = !released;
        }

        return None;
    }

    // ========================================================================
    // CAPS LOCK
    // ========================================================================
    //
    // Caps Lock has make code 0x3A.
    //
    // It toggles state only when pressed. The release code must not toggle it
    // back immediately.

    if key == 0x3A {
        if !released {
            // SAFETY:
            //
            // See the single-threaded-state explanation above.
            unsafe {
                CAPS_LOCK = !CAPS_LOCK;
            }
        }

        return None;
    }

    // ========================================================================
    // IGNORE OTHER KEY RELEASES
    // ========================================================================
    //
    // We only create terminal characters when a key is pressed.

    if released {
        return None;
    }

    // ========================================================================
    // READ CURRENT MODIFIER STATE
    // ========================================================================

    let shift = unsafe { LEFT_SHIFT_PRESSED || RIGHT_SHIFT_PRESSED };
    let caps = unsafe { CAPS_LOCK };

    // ========================================================================
    // CHARACTER LOOKUP
    // ========================================================================

    match key {
        // --------------------------------------------------------------------
        // LETTERS
        // --------------------------------------------------------------------
        //
        // Shift XOR Caps Lock determines the final case.
        //
        // Example:
        //
        //     Shift OFF + Caps OFF -> lowercase
        //     Shift ON  + Caps OFF -> uppercase
        //     Shift OFF + Caps ON  -> uppercase
        //     Shift ON  + Caps ON  -> lowercase

        0x1E => Some(letter_case('a', shift, caps)),
        0x30 => Some(letter_case('b', shift, caps)),
        0x2E => Some(letter_case('c', shift, caps)),
        0x20 => Some(letter_case('d', shift, caps)),
        0x12 => Some(letter_case('e', shift, caps)),
        0x21 => Some(letter_case('f', shift, caps)),
        0x22 => Some(letter_case('g', shift, caps)),
        0x23 => Some(letter_case('h', shift, caps)),
        0x17 => Some(letter_case('i', shift, caps)),
        0x24 => Some(letter_case('j', shift, caps)),
        0x25 => Some(letter_case('k', shift, caps)),
        0x26 => Some(letter_case('l', shift, caps)),
        0x32 => Some(letter_case('m', shift, caps)),
        0x31 => Some(letter_case('n', shift, caps)),
        0x18 => Some(letter_case('o', shift, caps)),
        0x19 => Some(letter_case('p', shift, caps)),
        0x10 => Some(letter_case('q', shift, caps)),
        0x13 => Some(letter_case('r', shift, caps)),
        0x1F => Some(letter_case('s', shift, caps)),
        0x14 => Some(letter_case('t', shift, caps)),
        0x16 => Some(letter_case('u', shift, caps)),
        0x2F => Some(letter_case('v', shift, caps)),
        0x11 => Some(letter_case('w', shift, caps)),
        0x2D => Some(letter_case('x', shift, caps)),
        0x15 => Some(letter_case('y', shift, caps)),
        0x2C => Some(letter_case('z', shift, caps)),

        // --------------------------------------------------------------------
        // NUMBER ROW
        // --------------------------------------------------------------------

        0x02 => Some(if shift { '!' } else { '1' }),
        0x03 => Some(if shift { '@' } else { '2' }),
        0x04 => Some(if shift { '#' } else { '3' }),
        0x05 => Some(if shift { '$' } else { '4' }),
        0x06 => Some(if shift { '%' } else { '5' }),
        0x07 => Some(if shift { '^' } else { '6' }),
        0x08 => Some(if shift { '&' } else { '7' }),
        0x09 => Some(if shift { '*' } else { '8' }),
        0x0A => Some(if shift { '(' } else { '9' }),
        0x0B => Some(if shift { ')' } else { '0' }),

        // --------------------------------------------------------------------
        // SYMBOL KEYS
        // --------------------------------------------------------------------

        // Hyphen / underscore.
        0x0C => Some(if shift { '_' } else { '-' }),

        // Equals / plus.
        0x0D => Some(if shift { '+' } else { '=' }),

        // Left bracket / left brace.
        0x1A => Some(if shift { '{' } else { '[' }),

        // Right bracket / right brace.
        0x1B => Some(if shift { '}' } else { ']' }),

        // Backslash / pipe.
        0x2B => Some(if shift { '|' } else { '\\' }),

        // Semicolon / colon.
        0x27 => Some(if shift { ':' } else { ';' }),

        // Apostrophe / quotation mark.
        0x28 => Some(if shift { '"' } else { '\'' }),

        // Comma / less-than.
        0x33 => Some(if shift { '<' } else { ',' }),

        // Period / greater-than.
        0x34 => Some(if shift { '>' } else { '.' }),

        // Slash / question mark.
        0x35 => Some(if shift { '?' } else { '/' }),

        // Backtick / tilde.
        0x29 => Some(if shift { '~' } else { '`' }),

        // --------------------------------------------------------------------
        // SPACE
        // --------------------------------------------------------------------

        0x39 => Some(' '),

        // --------------------------------------------------------------------
        // ENTER
        // --------------------------------------------------------------------

        0x1C => Some('\n'),

        // --------------------------------------------------------------------
        // BACKSPACE
        // --------------------------------------------------------------------

        0x0E => Some('\x08'),

        // --------------------------------------------------------------------
        // TAB
        // --------------------------------------------------------------------

        0x0F => Some('\t'),

        // --------------------------------------------------------------------
        // ESCAPE
        // --------------------------------------------------------------------
        //
        // Escape is represented by the ASCII escape character.
        //
        // The shell can use this for cancelling commands, clearing input,
        // switching modes, etc.

        0x01 => Some('\x1B'),

        // --------------------------------------------------------------------
        // EVERYTHING ELSE
        // --------------------------------------------------------------------
        //
        // Function keys, Alt combinations, arrow keys, etc. will eventually
        // become proper keyboard events instead of being discarded.

        _ => None,
    }
}

// ============================================================================
// LETTER CASE HANDLING
// ============================================================================

/// Apply Shift and Caps Lock state to an alphabetic character.
///
/// Shift and Caps Lock use XOR logic:
///
///     false ^ false = lowercase
///     true  ^ false = uppercase
///     false ^ true  = uppercase
///     true  ^ true  = lowercase
///
/// This matches normal PC keyboard behavior.
#[inline(always)]
fn letter_case(lowercase: char, shift: bool, caps: bool) -> char {
    let uppercase = shift ^ caps;

    if uppercase {
        // ASCII lowercase letters occupy a contiguous range from 'a' to 'z'.
        // Subtracting the ASCII offset converts the character to uppercase.
        ((lowercase as u8) - b'a' + b'A') as char
    } else {
        lowercase
    }
}