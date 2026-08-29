//! ============================================================================
//! OXIDE TERMINAL
//! ============================================================================
//!
//! Early framebuffer terminal for the Oxide kernel.
//!
//! This is intentionally a very small text renderer. It does not depend on
//! VGA text mode because modern machines are generally booted using a graphical
//! framebuffer supplied by the bootloader.
//!
//! Current responsibilities:
//!
//! - Clear the framebuffer.
//! - Draw individual pixels.
//! - Draw simple ASCII characters.
//! - Track the terminal cursor.
//! - Handle newlines.
//! - Automatically wrap lines.
//! - Scroll the screen when necessary.
//! - Print strings.
//! - Print unsigned integers.
//!
//! Future improvements can include:
//!
//! - Better fonts.
//! - ANSI escape sequences.
//! - Colored text.
//! - Backspace handling.
//! - Tab handling.
//! - Keyboard input.
//! - A proper `core::fmt::Write` implementation.
//! - A global kernel logger.
//! ============================================================================

use limine::framebuffer::Framebuffer;

// -----------------------------------------------------------------------------
// TERMINAL CONFIGURATION
// -----------------------------------------------------------------------------

/// Width of a single terminal character in pixels.
///
/// The current built-in font is an 8x8 bitmap font.
const CHAR_WIDTH: usize = 8;

/// Height of a single terminal character in pixels.
const CHAR_HEIGHT: usize = 8;

/// Background red channel.
const BACKGROUND_RED: u8 = 8;

/// Background green channel.
const BACKGROUND_GREEN: u8 = 10;

/// Background blue channel.
const BACKGROUND_BLUE: u8 = 16;

/// Foreground red channel.
const FOREGROUND_RED: u8 = 220;

/// Foreground green channel.
const FOREGROUND_GREEN: u8 = 220;

/// Foreground blue channel.
const FOREGROUND_BLUE: u8 = 220;

// -----------------------------------------------------------------------------
// TERMINAL STATE
// -----------------------------------------------------------------------------
//
// The terminal is currently implemented as a very small global state.
//
// Oxide is single-core/single-threaded at this stage, so a simple static
// pointer is sufficient for the initial implementation.
//
// This will eventually be replaced with a proper graphics/framebuffer manager.
// -----------------------------------------------------------------------------

/// Raw framebuffer address.
///
/// This is set during `init()`.
static mut FRAMEBUFFER_ADDRESS: *mut u8 = core::ptr::null_mut();

/// Number of bytes between the beginning of two framebuffer rows.
static mut FRAMEBUFFER_PITCH: usize = 0;

/// Framebuffer width in pixels.
static mut FRAMEBUFFER_WIDTH: usize = 0;

/// Framebuffer height in pixels.
static mut FRAMEBUFFER_HEIGHT: usize = 0;

/// Number of bytes occupied by one pixel.
static mut BYTES_PER_PIXEL: usize = 0;

/// Current terminal cursor X position in pixels.
static mut CURSOR_X: usize = 0;

/// Current terminal cursor Y position in pixels.
static mut CURSOR_Y: usize = 0;

// -----------------------------------------------------------------------------
// TERMINAL INITIALIZATION
// -----------------------------------------------------------------------------

/// Initializes the Oxide terminal using a Limine framebuffer.
///
/// # Safety
///
/// The framebuffer must be a valid framebuffer supplied by Limine.
///
/// The framebuffer memory must remain valid for the lifetime of the terminal.
pub fn init(framebuffer: &Framebuffer) {
    // We currently only support 32-bit framebuffers.
    //
    // A future graphics subsystem will support additional framebuffer
    // formats using the color mask information supplied by Limine.
    if framebuffer.bpp != 32 {
        return;
    }

    // Store the framebuffer properties used by the renderer.
    unsafe {
        FRAMEBUFFER_ADDRESS = framebuffer.address() as *mut u8;
        FRAMEBUFFER_PITCH = framebuffer.pitch as usize;
        FRAMEBUFFER_WIDTH = framebuffer.width as usize;
        FRAMEBUFFER_HEIGHT = framebuffer.height as usize;
        BYTES_PER_PIXEL = 4;

        // Start the terminal in the top-left corner.
        CURSOR_X = 0;
        CURSOR_Y = 0;
    }

    // Clear the entire framebuffer.
    clear();
}

// -----------------------------------------------------------------------------
// SCREEN CLEARING
// -----------------------------------------------------------------------------

/// Clears the entire framebuffer using the terminal background color.
fn clear() {
    let (width, height) = unsafe {
        (FRAMEBUFFER_WIDTH, FRAMEBUFFER_HEIGHT)
    };

    // Fill every pixel with the background color.
    for y in 0..height {
        for x in 0..width {
            put_pixel(
                x,
                y,
                BACKGROUND_RED,
                BACKGROUND_GREEN,
                BACKGROUND_BLUE,
            );
        }
    }
}

// -----------------------------------------------------------------------------
// CHARACTER OUTPUT
// -----------------------------------------------------------------------------

/// Prints a string to the terminal.
///
/// The string is processed character by character.
///
/// Currently supported control character:
///
/// - `\n` — newline
///
/// Printable ASCII characters are rendered using the built-in 8x8 font.
pub fn print(text: &str) {
    for byte in text.bytes() {
        match byte {
            // Newline moves the cursor to the beginning of the next row.
            b'\n' => newline(),

            // Carriage return moves the cursor to the beginning of the line.
            b'\r' => carriage_return(),

            // Printable ASCII characters are rendered normally.
            32..=126 => put_char(byte),

            // Unsupported characters are ignored for now.
            _ => {}
        }
    }
}

/// Prints a string followed by a newline.
pub fn println(text: &str) {
    print(text);
    newline();
}

// -----------------------------------------------------------------------------
// INTEGER OUTPUT
// -----------------------------------------------------------------------------

/// Prints an unsigned 64-bit integer.
///
/// We implement this ourselves because the kernel currently does not use
/// Rust's normal formatting machinery.
pub fn print_u64(mut value: u64) {
    // Zero requires special handling because the conversion loop below
    // would otherwise produce no digits.
    if value == 0 {
        print("0");
        return;
    }

    // A u64 can contain at most 20 decimal digits.
    let mut buffer = [0u8; 20];
    let mut index = buffer.len();

    // Convert the number into decimal digits from right to left.
    while value > 0 {
        let digit = (value % 10) as u8;

        index -= 1;
        buffer[index] = b'0' + digit;

        value /= 10;
    }

    // Print the generated digits.
    for &digit in &buffer[index..] {
        put_char(digit);
    }
}

// -----------------------------------------------------------------------------
// CURSOR MANAGEMENT
// -----------------------------------------------------------------------------

/// Moves the cursor to the beginning of the next text line.
fn newline() {
    unsafe {
        CURSOR_X = 0;
        CURSOR_Y += CHAR_HEIGHT;

        // If we moved below the screen, scroll the framebuffer.
        if CURSOR_Y + CHAR_HEIGHT > FRAMEBUFFER_HEIGHT {
            scroll();
        }
    }
}

/// Moves the cursor to the beginning of the current line.
fn carriage_return() {
    unsafe {
        CURSOR_X = 0;
    }
}

/// Ensures there is enough horizontal space for another character.
///
/// If there is not enough space, the terminal automatically starts a new line.
fn ensure_horizontal_space() {
    unsafe {
        if CURSOR_X + CHAR_WIDTH > FRAMEBUFFER_WIDTH {
            CURSOR_X = 0;
            CURSOR_Y += CHAR_HEIGHT;

            if CURSOR_Y + CHAR_HEIGHT > FRAMEBUFFER_HEIGHT {
                scroll();
            }
        }
    }
}

// -----------------------------------------------------------------------------
// SCREEN SCROLLING
// -----------------------------------------------------------------------------

/// Scrolls the framebuffer upward by one character row.
///
/// The newly created bottom row is cleared.
///
/// This is intentionally simple for now. Later, Oxide can replace this with
/// a more efficient memory-copy implementation.
fn scroll() {
    unsafe {
        // If the framebuffer is too small to contain even one character row,
        // there is nothing useful to scroll.
        if FRAMEBUFFER_HEIGHT <= CHAR_HEIGHT {
            CURSOR_Y = 0;
            clear();
            return;
        }

        let width = FRAMEBUFFER_WIDTH;
        let height = FRAMEBUFFER_HEIGHT;
        let pitch = FRAMEBUFFER_PITCH;

        // Move every row upward by CHAR_HEIGHT pixels.
        for y in CHAR_HEIGHT..height {
            let source_offset = y * pitch;
            let destination_offset = (y - CHAR_HEIGHT) * pitch;

            // Copy one framebuffer row.
            //
            // `copy` is safe here because the source and destination rows do
            // not overlap in a way that violates the operation.
            core::ptr::copy(
                FRAMEBUFFER_ADDRESS.add(source_offset),
                FRAMEBUFFER_ADDRESS.add(destination_offset),
                width * BYTES_PER_PIXEL,
            );
        }

        // Clear the bottom character row.
        for y in (height - CHAR_HEIGHT)..height {
            for x in 0..width {
                put_pixel(
                    x,
                    y,
                    BACKGROUND_RED,
                    BACKGROUND_GREEN,
                    BACKGROUND_BLUE,
                );
            }
        }

        // The cursor remains on the newly created bottom line.
        CURSOR_Y = height - CHAR_HEIGHT;
    }
}

// -----------------------------------------------------------------------------
// CHARACTER RENDERING
// -----------------------------------------------------------------------------

/// Draws one ASCII character at the current cursor position.
fn put_char(character: u8) {
    ensure_horizontal_space();

    // Read the current cursor location.
    let (x, y) = unsafe {
        (CURSOR_X, CURSOR_Y)
    };

    // Render the character using the built-in bitmap font.
    draw_glyph(character, x, y);

    // Advance the cursor by one character cell.
    unsafe {
        CURSOR_X += CHAR_WIDTH;
    }
}

// -----------------------------------------------------------------------------
// GLYPH RENDERING
// -----------------------------------------------------------------------------

/// Draws one 8x8 ASCII glyph.
///
/// The font is deliberately tiny and dependency-free.
///
/// Each byte represents one row of the glyph.
/// Each bit represents one pixel.
///
/// Bit 7 is the leftmost pixel and bit 0 is the rightmost pixel.
fn draw_glyph(character: u8, x: usize, y: usize) {
    let glyph = glyph_for(character);

    // Each glyph contains exactly eight rows.
    for row in 0..8 {
        let bits = glyph[row];

        // Each row contains exactly eight pixels.
        for column in 0..8 {
            // Check whether this pixel is part of the character.
            let pixel_is_set = (bits & (1 << (7 - column))) != 0;

            if pixel_is_set {
                put_pixel(
                    x + column,
                    y + row,
                    FOREGROUND_RED,
                    FOREGROUND_GREEN,
                    FOREGROUND_BLUE,
                );
            }
        }
    }
}

// -----------------------------------------------------------------------------
// BASIC 8x8 FONT
// -----------------------------------------------------------------------------

/// Returns the 8x8 bitmap for an ASCII character.
///
/// This first version intentionally contains the characters needed by the
/// initial kernel messages plus the digits and common punctuation.
///
/// Unsupported characters fall back to a visible box.
///
/// The font can be expanded later without changing the terminal architecture.
fn glyph_for(character: u8) -> [u8; 8] {
    match character {
        b' ' => [
            0b00000000,
            0b00000000,
            0b00000000,
            0b00000000,
            0b00000000,
            0b00000000,
            0b00000000,
            0b00000000,
        ],

        b'A' => [
            0b00111100,
            0b01100110,
            0b11000011,
            0b11000011,
            0b11111111,
            0b11000011,
            0b11000011,
            0b00000000,
        ],

        b'B' => [
            0b11111100,
            0b11000110,
            0b11000110,
            0b11111100,
            0b11000110,
            0b11000110,
            0b11111100,
            0b00000000,
        ],

        b'C' => [
            0b00111110,
            0b01100000,
            0b11000000,
            0b11000000,
            0b11000000,
            0b01100000,
            0b00111110,
            0b00000000,
        ],

        b'D' => [
            0b11111100,
            0b11000110,
            0b11000011,
            0b11000011,
            0b11000011,
            0b11000110,
            0b11111100,
            0b00000000,
        ],

        b'E' => [
            0b11111110,
            0b11000000,
            0b11000000,
            0b11111100,
            0b11000000,
            0b11000000,
            0b11111110,
            0b00000000,
        ],

        b'F' => [
            0b11111110,
            0b11000000,
            0b11000000,
            0b11111100,
            0b11000000,
            0b11000000,
            0b11000000,
            0b00000000,
        ],

        b'G' => [
            0b00111110,
            0b01100000,
            0b11000000,
            0b11001110,
            0b11000011,
            0b01100011,
            0b00111110,
            0b00000000,
        ],

        b'H' => [
            0b11000011,
            0b11000011,
            0b11000011,
            0b11111111,
            0b11000011,
            0b11000011,
            0b11000011,
            0b00000000,
        ],

        b'I' => [
            0b01111110,
            0b00011000,
            0b00011000,
            0b00011000,
            0b00011000,
            0b00011000,
            0b01111110,
            0b00000000,
        ],

        b'J' => [
            0b00011111,
            0b00000110,
            0b00000110,
            0b00000110,
            0b11000110,
            0b11000110,
            0b01111100,
            0b00000000,
        ],

        b'K' => [
            0b11000110,
            0b11001100,
            0b11011000,
            0b11110000,
            0b11011000,
            0b11001100,
            0b11000110,
            0b00000000,
        ],

        b'L' => [
            0b11000000,
            0b11000000,
            0b11000000,
            0b11000000,
            0b11000000,
            0b11000000,
            0b11111110,
            0b00000000,
        ],

        b'M' => [
            0b11000011,
            0b11100111,
            0b11111111,
            0b11011011,
            0b11000011,
            0b11000011,
            0b11000011,
            0b00000000,
        ],

        b'N' => [
            0b11000011,
            0b11100011,
            0b11110011,
            0b11011011,
            0b11001111,
            0b11000111,
            0b11000011,
            0b00000000,
        ],

        b'O' => [
            0b00111100,
            0b01100110,
            0b11000011,
            0b11000011,
            0b11000011,
            0b01100110,
            0b00111100,
            0b00000000,
        ],

        b'P' => [
            0b11111100,
            0b11000110,
            0b11000110,
            0b11111100,
            0b11000000,
            0b11000000,
            0b11000000,
            0b00000000,
        ],

        b'Q' => [
            0b00111100,
            0b01100110,
            0b11000011,
            0b11000011,
            0b11011011,
            0b01100110,
            0b00111101,
            0b00000000,
        ],

        b'R' => [
            0b11111100,
            0b11000110,
            0b11000110,
            0b11111100,
            0b11011000,
            0b11001100,
            0b11000110,
            0b00000000,
        ],

        b'S' => [
            0b00111110,
            0b01100000,
            0b01100000,
            0b00111100,
            0b00000110,
            0b00000110,
            0b11111100,
            0b00000000,
        ],

        b'T' => [
            0b11111111,
            0b00011000,
            0b00011000,
            0b00011000,
            0b00011000,
            0b00011000,
            0b00011000,
            0b00000000,
        ],

        b'U' => [
            0b11000011,
            0b11000011,
            0b11000011,
            0b11000011,
            0b11000011,
            0b01100110,
            0b00111100,
            0b00000000,
        ],

        b'V' => [
            0b11000011,
            0b11000011,
            0b11000011,
            0b11000011,
            0b01100110,
            0b01100110,
            0b00111100,
            0b00000000,
        ],

        b'W' => [
            0b11000011,
            0b11000011,
            0b11000011,
            0b11011011,
            0b11111111,
            0b11100111,
            0b11000011,
            0b00000000,
        ],

        b'X' => [
            0b11000011,
            0b01100110,
            0b00111100,
            0b00011000,
            0b00111100,
            0b01100110,
            0b11000011,
            0b00000000,
        ],

        b'Y' => [
            0b11000011,
            0b01100110,
            0b00111100,
            0b00011000,
            0b00011000,
            0b00011000,
            0b00011000,
            0b00000000,
        ],

        b'Z' => [
            0b11111111,
            0b00000110,
            0b00001100,
            0b00011000,
            0b00110000,
            0b01100000,
            0b11111111,
            0b00000000,
        ],

        b'a' => [
            0b00000000,
            0b00000000,
            0b00111100,
            0b00000110,
            0b00111110,
            0b01100110,
            0b00111110,
            0b00000000,
        ],

        b'b' => [
            0b11000000,
            0b11000000,
            0b11011100,
            0b11100110,
            0b11000110,
            0b11100110,
            0b11011100,
            0b00000000,
        ],

        b'c' => [
            0b00000000,
            0b00000000,
            0b00111110,
            0b01100000,
            0b01100000,
            0b01100000,
            0b00111110,
            0b00000000,
        ],

        b'd' => [
            0b00000110,
            0b00000110,
            0b00110110,
            0b01101110,
            0b11000110,
            0b11000110,
            0b00110110,
            0b00000000,
        ],

        b'e' => [
            0b00000000,
            0b00000000,
            0b00111100,
            0b01100110,
            0b11111110,
            0b01100000,
            0b00111110,
            0b00000000,
        ],

        b'f' => [
            0b00011100,
            0b00110000,
            0b01111100,
            0b00110000,
            0b00110000,
            0b00110000,
            0b00110000,
            0b00000000,
        ],

        b'g' => [
            0b00000000,
            0b00000000,
            0b00111110,
            0b01100110,
            0b01100110,
            0b00111110,
            0b00000110,
            0b01111100,
        ],

        b'h' => [
            0b11000000,
            0b11000000,
            0b11011100,
            0b11100110,
            0b11000110,
            0b11000110,
            0b11000110,
            0b00000000,
        ],

        b'i' => [
            0b00011000,
            0b00000000,
            0b00111000,
            0b00011000,
            0b00011000,
            0b00011000,
            0b00111100,
            0b00000000,
        ],

        b'j' => [
            0b00000110,
            0b00000000,
            0b00001110,
            0b00000110,
            0b00000110,
            0b11000110,
            0b01101100,
            0b00111000,
        ],

        b'k' => [
            0b11000000,
            0b11000000,
            0b11001100,
            0b11011000,
            0b11110000,
            0b11011000,
            0b11001100,
            0b00000000,
        ],

        b'l' => [
            0b00111000,
            0b00011000,
            0b00011000,
            0b00011000,
            0b00011000,
            0b00011000,
            0b00111100,
            0b00000000,
        ],

        b'm' => [
            0b00000000,
            0b00000000,
            0b11100110,
            0b11111111,
            0b11011011,
            0b11011011,
            0b11011011,
            0b00000000,
        ],

        b'n' => [
            0b00000000,
            0b00000000,
            0b11011100,
            0b11100110,
            0b11000110,
            0b11000110,
            0b11000110,
            0b00000000,
        ],

        b'o' => [
            0b00000000,
            0b00000000,
            0b00111100,
            0b01100110,
            0b11000110,
            0b01100110,
            0b00111100,
            0b00000000,
        ],

        b'p' => [
            0b00000000,
            0b00000000,
            0b11011100,
            0b11100110,
            0b11000110,
            0b11100110,
            0b11011100,
            0b11000000,
        ],

        b'q' => [
            0b00000000,
            0b00000000,
            0b00110110,
            0b01101110,
            0b11000110,
            0b01101110,
            0b00110110,
            0b00000110,
        ],

        b'r' => [
            0b00000000,
            0b00000000,
            0b11011100,
            0b11100110,
            0b11000000,
            0b11000000,
            0b11000000,
            0b00000000,
        ],

        b's' => [
            0b00000000,
            0b00000000,
            0b00111110,
            0b01100000,
            0b00111100,
            0b00000110,
            0b11111100,
            0b00000000,
        ],

        b't' => [
            0b00110000,
            0b00110000,
            0b11111100,
            0b00110000,
            0b00110000,
            0b00110110,
            0b00011100,
            0b00000000,
        ],

        b'u' => [
            0b00000000,
            0b00000000,
            0b11000110,
            0b11000110,
            0b11000110,
            0b01101110,
            0b00110110,
            0b00000000,
        ],

        b'v' => [
            0b00000000,
            0b00000000,
            0b11000011,
            0b11000011,
            0b01100110,
            0b01100110,
            0b00111100,
            0b00000000,
        ],

        b'w' => [
            0b00000000,
            0b00000000,
            0b11000011,
            0b11011011,
            0b11111111,
            0b11100111,
            0b11000011,
            0b00000000,
        ],

        b'x' => [
            0b00000000,
            0b00000000,
            0b11000110,
            0b01101100,
            0b00111000,
            0b01101100,
            0b11000110,
            0b00000000,
        ],

        b'y' => [
            0b00000000,
            0b00000000,
            0b11000110,
            0b11000110,
            0b01100110,
            0b00111110,
            0b00000110,
            0b01111100,
        ],

        b'z' => [
            0b00000000,
            0b00000000,
            0b01111110,
            0b00001100,
            0b00011000,
            0b00110000,
            0b01111110,
            0b00000000,
        ],

        b'0' => [
            0b00111100,
            0b01100110,
            0b01101110,
            0b01110110,
            0b01100110,
            0b01100110,
            0b00111100,
            0b00000000,
        ],

        b'1' => [
            0b00011000,
            0b00111000,
            0b01111000,
            0b00011000,
            0b00011000,
            0b00011000,
            0b01111110,
            0b00000000,
        ],

        b'2' => [
            0b00111100,
            0b01100110,
            0b00000110,
            0b00001100,
            0b00011000,
            0b00110000,
            0b01111110,
            0b00000000,
        ],

        b'3' => [
            0b00111100,
            0b01100110,
            0b00000110,
            0b00011100,
            0b00000110,
            0b01100110,
            0b00111100,
            0b00000000,
        ],

        b'4' => [
            0b00001100,
            0b00011100,
            0b00111100,
            0b01101100,
            0b01111110,
            0b00001100,
            0b00001100,
            0b00000000,
        ],

        b'5' => [
            0b01111110,
            0b01100000,
            0b01100000,
            0b01111100,
            0b00000110,
            0b01100110,
            0b00111100,
            0b00000000,
        ],

        b'6' => [
            0b00111100,
            0b01100000,
            0b11000000,
            0b11111100,
            0b11000110,
            0b11000110,
            0b00111100,
            0b00000000,
        ],

        b'7' => [
            0b01111110,
            0b00000110,
            0b00001100,
            0b00011000,
            0b00110000,
            0b00110000,
            0b00110000,
            0b00000000,
        ],

        b'8' => [
            0b00111100,
            0b01100110,
            0b01100110,
            0b00111100,
            0b01100110,
            0b01100110,
            0b00111100,
            0b00000000,
        ],

        b'9' => [
            0b00111100,
            0b01100110,
            0b01100110,
            0b00111110,
            0b00000110,
            0b00001100,
            0b00111000,
            0b00000000,
        ],

        b'.' => [
            0b00000000,
            0b00000000,
            0b00000000,
            0b00000000,
            0b00000000,
            0b00011000,
            0b00011000,
            0b00000000,
        ],

        b':' => [
            0b00000000,
            0b00011000,
            0b00011000,
            0b00000000,
            0b00000000,
            0b00011000,
            0b00011000,
            0b00000000,
        ],

        b'-' => [
            0b00000000,
            0b00000000,
            0b00000000,
            0b01111110,
            0b00000000,
            0b00000000,
            0b00000000,
            0b00000000,
        ],

        b'_' => [
            0b00000000,
            0b00000000,
            0b00000000,
            0b00000000,
            0b00000000,
            0b00000000,
            0b11111111,
            0b00000000,
        ],

        _ => [
            0b11111111,
            0b10000001,
            0b10111101,
            0b10100101,
            0b10111101,
            0b10000001,
            0b11111111,
            0b00000000,
        ],
    }
}

// -----------------------------------------------------------------------------
// PIXEL WRITER
// -----------------------------------------------------------------------------

/// Writes one pixel to the framebuffer.
///
/// # Safety
///
/// This function assumes that the framebuffer globals were initialized by
/// `init()` and that the supplied coordinates are inside the framebuffer.
///
/// The framebuffer is expected to be a 32-bit RGB framebuffer.
fn put_pixel(
    x: usize,
    y: usize,
    red: u8,
    green: u8,
    blue: u8,
) {
    unsafe {
        // Ignore pixels outside the framebuffer.
        if x >= FRAMEBUFFER_WIDTH || y >= FRAMEBUFFER_HEIGHT {
            return;
        }

        // Calculate the byte offset of this pixel.
        //
        // `pitch` accounts for any padding at the end of framebuffer rows.
        let offset =
            y * FRAMEBUFFER_PITCH +
            x * BYTES_PER_PIXEL;

        // Write the channels using the common BGR byte ordering used by
        // Limine's 32-bit framebuffer on x86 systems.
        //
        // The color mask information in `Framebuffer` can eventually be
        // used here to support arbitrary pixel formats.
        FRAMEBUFFER_ADDRESS
            .add(offset)
            .write_volatile(blue);

        FRAMEBUFFER_ADDRESS
            .add(offset + 1)
            .write_volatile(green);

        FRAMEBUFFER_ADDRESS
            .add(offset + 2)
            .write_volatile(red);

        // Fourth byte is currently unused/alpha.
        FRAMEBUFFER_ADDRESS
            .add(offset + 3)
            .write_volatile(0);
    }
}