//! ============================================================================
//! OXIDE TERMINAL
//! ============================================================================
//!
//! Early framebuffer terminal for the Oxide kernel.
//!
//! This terminal renders text directly into the framebuffer supplied by Limine.
//! It intentionally avoids VGA text mode because Oxide is designed around a
//! modern framebuffer-based boot environment.
//!
//! Current responsibilities:
//!
//! - Initialize from a Limine framebuffer.
//! - Clear the framebuffer.
//! - Draw individual pixels.
//! - Draw ASCII characters.
//! - Draw the Oxide crab mascot.
//! - Draw the large fastfetch crab.
//! - Draw fastfetch system information beside the crab.
//! - Track the terminal cursor.
//! - Handle newlines and carriage returns.
//! - Automatically wrap lines.
//! - Scroll the screen when necessary.
//! - Print strings.
//! - Print unsigned integers.
//! - Print individual characters.
//! - Handle backspace.
//! - Clear the screen.
//!
//! The normal terminal remains ASCII based. The large Unicode-style crab used
//! by fastfetch is rendered directly as framebuffer pixel art rather than
//! being passed through the ASCII font.
//!
//! ============================================================================

use limine::framebuffer::Framebuffer;

// -----------------------------------------------------------------------------
// TERMINAL CONFIGURATION
// -----------------------------------------------------------------------------

/// Width of a single terminal character in pixels.
///
/// The current built-in font uses an 8x8 bitmap.
const CHAR_WIDTH: usize = 8;

/// Height of a single terminal character in pixels.
///
/// The current built-in font uses an 8x8 bitmap.
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
// FASTFETCH CONFIGURATION
// -----------------------------------------------------------------------------

/// Pixel size used for each character in the large fastfetch crab.
///
/// The original crab art uses Unicode block characters. We render each art
/// character as a small square grid directly into the framebuffer.
///
/// A scale of four makes the crab large enough to act as a proper fastfetch
/// logo without taking over the entire screen.
const FASTFETCH_CRAB_SCALE: usize = 4;

/// Horizontal position of the fastfetch information column relative to the
/// crab.
///
/// The value is intentionally based on terminal character cells so the text
/// remains aligned with the existing 8x8 font.
const FASTFETCH_INFO_X_CELLS: usize = 34;

// -----------------------------------------------------------------------------
// TERMINAL STATE
// -----------------------------------------------------------------------------

/// Raw framebuffer address.
///
/// This is initialized by [`init()`].
///
/// Oxide is currently single-core/single-threaded, so this simple global
/// framebuffer state is sufficient for the early terminal implementation.
///
/// This will eventually be replaced by a proper framebuffer manager.
static mut FRAMEBUFFER_ADDRESS: *mut u8 = core::ptr::null_mut();

/// Number of bytes between the beginning of two framebuffer rows.
static mut FRAMEBUFFER_PITCH: usize = 0;

/// Framebuffer width in pixels.
static mut FRAMEBUFFER_WIDTH: usize = 0;

/// Framebuffer height in pixels.
static mut FRAMEBUFFER_HEIGHT: usize = 0;

/// Number of bytes occupied by one pixel.
///
/// Oxide currently supports 32-bit framebuffers, therefore this is 4.
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
/// The framebuffer must be valid and remain mapped for the lifetime of the
/// terminal.
pub fn init(framebuffer: &Framebuffer) {
    // Oxide currently supports only 32-bit framebuffer formats.
    //
    // Supporting arbitrary framebuffer formats will be handled later using
    // Limine's color mask information.
    if framebuffer.bpp != 32 {
        return;
    }

    unsafe {
        // Store the framebuffer's base address.
        FRAMEBUFFER_ADDRESS = framebuffer.address() as *mut u8;

        // Store the number of bytes between framebuffer rows.
        FRAMEBUFFER_PITCH = framebuffer.pitch as usize;

        // Store the framebuffer dimensions.
        FRAMEBUFFER_WIDTH = framebuffer.width as usize;
        FRAMEBUFFER_HEIGHT = framebuffer.height as usize;

        // A 32-bit framebuffer contains four bytes per pixel.
        BYTES_PER_PIXEL = 4;

        // Start the terminal cursor at the top-left corner.
        CURSOR_X = 0;
        CURSOR_Y = 0;
    }

    // Clear the framebuffer before printing anything.
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

    // Fill every pixel with the configured background color.
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

/// Clears the screen and resets the terminal cursor to the top-left corner.
///
/// This is exposed to the shell so commands such as `clear` can reset the
/// terminal without having to know anything about framebuffer internals.
pub fn clear_screen() {
    unsafe {
        CURSOR_X = 0;
        CURSOR_Y = 0;
    }

    clear();
}

// -----------------------------------------------------------------------------
// STRING OUTPUT
// -----------------------------------------------------------------------------

/// Prints a string to the terminal.
///
/// Currently supported control characters:
///
/// - `\n` — newline
/// - `\r` — carriage return
///
/// Printable ASCII characters are rendered using the built-in bitmap font.
///
/// UTF-8 characters outside ASCII are intentionally ignored by the normal
/// terminal renderer. Special graphical elements such as the fastfetch crab
/// use their own framebuffer renderer.
pub fn print(text: &str) {
    for byte in text.bytes() {
        match byte {
            // Move to the next line.
            b'\n' => newline(),

            // Move to the beginning of the current line.
            b'\r' => carriage_return(),

            // Render printable ASCII.
            32..=126 => put_char(byte),

            // Ignore unsupported bytes for now.
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
/// This conversion is implemented manually because Oxide does not yet rely
/// on Rust's normal formatting machinery.
pub fn print_u64(mut value: u64) {
    // Zero requires special handling because the conversion loop below would
    // otherwise produce no digits.
    if value == 0 {
        print("0");
        return;
    }

    // A u64 contains at most 20 decimal digits.
    let mut buffer = [0u8; 20];

    // Start at the end of the buffer and build the number backwards.
    let mut index = buffer.len();

    while value > 0 {
        // Extract the final decimal digit.
        let digit = (value % 10) as u8;

        // Move backwards through the buffer.
        index -= 1;

        // Convert the digit into its ASCII representation.
        buffer[index] = b'0' + digit;

        // Remove the digit from the number.
        value /= 10;
    }

    // Print the resulting ASCII digits.
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
        // Start the next line at the left edge.
        CURSOR_X = 0;

        // Move down by one character cell.
        CURSOR_Y += CHAR_HEIGHT;

        // Scroll if the new cursor position would be below the framebuffer.
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

/// Ensures that another character fits horizontally.
///
/// If there is not enough space, the cursor automatically moves to the next
/// line.
fn ensure_horizontal_space() {
    unsafe {
        // Check whether the next character would extend past the framebuffer.
        if CURSOR_X + CHAR_WIDTH > FRAMEBUFFER_WIDTH {
            CURSOR_X = 0;
            CURSOR_Y += CHAR_HEIGHT;

            // Scroll if necessary.
            if CURSOR_Y + CHAR_HEIGHT > FRAMEBUFFER_HEIGHT {
                scroll();
            }
        }
    }
}

// -----------------------------------------------------------------------------
// OXIDE CRAB MASCOT
// -----------------------------------------------------------------------------

/// Draws the original small Oxide crab mascot at the current cursor position.
///
/// This function is intentionally preserved from the original terminal.
///
/// The new large fastfetch crab is separate so existing `crab` behavior does
/// not unexpectedly change.
pub fn print_crab() {
    // Each string represents one horizontal row of the crab.
    //
    // '#' = foreground pixel
    // ' ' = transparent pixel
    const CRAB: [&[u8]; 13] = [
        b"      ##      ##      ",
        b"      ##      ##      ",
        b"     ###      ###     ",
        b"   ###  ##  ##  ###   ",
        b"  ##    ######    ##  ",
        b" ##   ############   ##",
        b"###  ################  ",
        b" ## ################ ##",
        b"  #################### ",
        b" ##  ##############  ##",
        b"###  ##  ####  ##  ###",
        b" ## ##          ## ## ",
        b"  ###            ###   ",
    ];

    // Each ASCII-art pixel is rendered as a small filled square.
    //
    // Using two framebuffer pixels horizontally gives the mascot a more
    // natural terminal-art aspect ratio because normal terminal characters
    // are visually taller than they are wide.
    const PIXEL_WIDTH: usize = 2;
    const PIXEL_HEIGHT: usize = 2;

    // Determine how much horizontal space the mascot needs.
    let crab_width = 22 * PIXEL_WIDTH;
    let crab_height = 13 * PIXEL_HEIGHT;

    // If the crab does not fit on the current line, move to the next one.
    unsafe {
        if CURSOR_X + crab_width > FRAMEBUFFER_WIDTH {
            CURSOR_X = 0;
            CURSOR_Y += CHAR_HEIGHT;

            if CURSOR_Y + crab_height > FRAMEBUFFER_HEIGHT {
                scroll();
            }
        }
    }

    let (start_x, start_y) = unsafe {
        (CURSOR_X, CURSOR_Y)
    };

    // Render each character in the crab bitmap.
    for (row, line) in CRAB.iter().enumerate() {
        for (column, &pixel) in line.iter().enumerate() {
            if pixel == b'#' {
                // Draw a 2x2 pixel block so the crab is clearly visible.
                for py in 0..PIXEL_HEIGHT {
                    for px in 0..PIXEL_WIDTH {
                        put_pixel(
                            start_x + column * PIXEL_WIDTH + px,
                            start_y + row * PIXEL_HEIGHT + py,
                            FOREGROUND_RED,
                            FOREGROUND_GREEN,
                            FOREGROUND_BLUE,
                        );
                    }
                }
            }
        }
    }

    // Leave one normal character cell of spacing after the crab.
    unsafe {
        CURSOR_X += crab_width + CHAR_WIDTH;
    }
}

// -----------------------------------------------------------------------------
// LARGE FASTFETCH CRAB
// -----------------------------------------------------------------------------

/// Exact large crab artwork used by the Oxide fastfetch screen.
///
/// The Unicode block characters are NOT sent through the normal ASCII text
/// renderer. Instead, `print_fastfetch()` interprets them directly:
///
///     '▓' = shaded/dithered block
///     '█' = solid block
///     ' ' = transparent
///
/// This means the visual shape remains intact even though the normal terminal
/// font does not support Unicode.
const FASTFETCH_CRAB: [&str; 21] = [
    "              ▓▓▓▓▓▓▓▓▓▓▓▓                    ",
    "  ▓▓▓▓      ▓▓▓▓▓▓▓▓  ▓▓▓▓▓▓                  ",
    "  ▓▓      ▓▓▓▓    ▓▓██    ██                  ",
    "  ▓▓▓▓    ▓▓▓▓    ████              ▓▓▓▓      ",
    "    ▓▓    ▓▓▓▓                        ▓▓▓▓    ",
    "    ▓▓▓▓  ▓▓████        ▓▓        ▓▓    ██    ",
    "      ▓▓▓▓  ██▓▓▓▓▓▓▓▓▓▓          ▓▓▓▓▓▓██    ",
    "▓▓      ████▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓  ▓▓    ▓▓▓▓██    ",
    "▓▓▓▓▓▓    ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓        ▓▓██    ",
    "  ▓▓▓▓████▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓      ▓▓██    ",
    "          ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓    ████  ▓▓",
    "    ▓▓██████▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓████████    ██",
    "  ▓▓▓▓      ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓████████    ████",
    "  ▓▓  ▓▓▓▓██████▓▓▓▓▓▓▓▓▓▓████████  ▓▓▓▓▓▓██",
    "    ▓▓▓▓        ████▓▓▓▓██████████▓▓          ",
    "    ▓▓▓▓            ▓▓████████    ▓▓▓▓    ██  ",
    "      ▓▓▓▓              ▓▓  ▓▓▓▓    ▓▓▓▓▓▓██  ",
    "                        ▓▓    ▓▓▓▓              ",
    "                      ████      ██              ",
    "                  ▓▓████      ████              ",
    "                                                  ",
];

/// Draw the large fastfetch crab at a framebuffer position.
///
/// This function renders directly to pixels, bypassing the normal 8x8 ASCII
/// font. That is what allows the Unicode-looking `▓` and `█` art to retain
/// its intended appearance.
fn draw_fastfetch_crab(start_x: usize, start_y: usize) {
    for (row, line) in FASTFETCH_CRAB.iter().enumerate() {
        let mut column = 0usize;

        // `chars()` is used here rather than bytes because the art contains
        // multibyte UTF-8 characters such as `▓` and `█`.
        for character in line.chars() {
            match character {
                // Solid block: fill the entire scaled square.
                '█' => {
                    draw_scaled_block(
                        start_x + column * FASTFETCH_CRAB_SCALE,
                        start_y + row * FASTFETCH_CRAB_SCALE,
                        FASTFETCH_CRAB_SCALE,
                        false,
                    );
                }

                // Shaded block: render a checker/dither pattern.
                '▓' => {
                    draw_scaled_block(
                        start_x + column * FASTFETCH_CRAB_SCALE,
                        start_y + row * FASTFETCH_CRAB_SCALE,
                        FASTFETCH_CRAB_SCALE,
                        true,
                    );
                }

                // Spaces remain transparent.
                ' ' => {}

                // Any unexpected character is ignored.
                _ => {}
            }

            column += 1;
        }
    }
}

/// Draw one scaled crab-art block.
///
/// `shaded == false` produces a solid block.
///
/// `shaded == true` produces a simple checker/dither pattern. This gives the
/// `▓` character a visibly different appearance from `█` without requiring a
/// full Unicode font renderer.
fn draw_scaled_block(
    start_x: usize,
    start_y: usize,
    scale: usize,
    shaded: bool,
) {
    for y in 0..scale {
        for x in 0..scale {
            // A checker pattern gives the shaded block its visual texture.
            let visible = !shaded || ((x + y) % 2 == 0);

            if visible {
                put_pixel(
                    start_x + x,
                    start_y + y,
                    FOREGROUND_RED,
                    FOREGROUND_GREEN,
                    FOREGROUND_BLUE,
                );
            }
        }
    }
}

// -----------------------------------------------------------------------------
// FASTFETCH
// -----------------------------------------------------------------------------

/// Prints the Oxide fastfetch display.
///
/// The layout is intentionally similar to a traditional Linux fastfetch:
///
///     [large Oxide crab]     Oxide
///                            ----------------
///                            Kernel: ...
///                            Architecture: ...
///                            ...
///
/// The crab is rendered directly as framebuffer art while the information on
/// the right uses the normal Oxide 8x8 font.
///
/// This function does not print the crab during boot. It only runs when the
/// shell explicitly executes `fastfetch` or `neofetch`.
pub fn print_fastfetch() {
    // Determine the dimensions of the framebuffer before drawing.
    let (width, height) = unsafe {
        (FRAMEBUFFER_WIDTH, FRAMEBUFFER_HEIGHT)
    };

    // The crab artwork is roughly 52 characters wide and 21 rows tall.
    //
    // Each artwork character is four pixels wide, giving the crab a width of
    // roughly 208 pixels.
    let crab_width = FASTFETCH_CRAB
        .iter()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0)
        * FASTFETCH_CRAB_SCALE;

    let crab_height = FASTFETCH_CRAB.len() * FASTFETCH_CRAB_SCALE;

    // Keep the system information at a predictable terminal-cell position.
    let info_x = FASTFETCH_INFO_X_CELLS * CHAR_WIDTH;

    // Use the larger of the two positions if necessary so that the text never
    // overlaps the crab.
    let info_x = core::cmp::max(info_x, crab_width + 24);

    // The information block contains 12 lines. Leave enough space for it.
    let info_height = 12 * CHAR_HEIGHT;

    // If the fastfetch output would not fit vertically, start it from a clean
    // screen. This prevents drawing partially off-screen.
    let required_height = core::cmp::max(crab_height, info_height);

    unsafe {
        if CURSOR_Y + required_height + CHAR_HEIGHT > height {
            CURSOR_X = 0;
            CURSOR_Y = 0;
            clear();
        }
    }

    // Capture the starting Y position once so both columns share the same
    // vertical alignment.
    let start_y = unsafe { CURSOR_Y };

    // Draw the large crab.
    draw_fastfetch_crab(0, start_y);

    // Draw the system information using the existing ASCII renderer.
    //
    // `draw_fastfetch_text()` works directly at a framebuffer position instead
    // of changing the terminal cursor while rendering the right-hand column.
    let info = [
        "Oxide",
        "------------------------------",
        "OS:           Oxide",
        "Kernel:       oxide-kernel 0.1.0",
        "Architecture: x86_64",
        "Bootloader:   Limine",
        "Terminal:     framebuffer",
        "Input:        PS/2 polling",
        "Keyboard:     active",
        "Memory:       detection pending",
        "Heap:         not initialized",
        "Status:       ONLINE",
    ];

    for (line_index, line) in info.iter().enumerate() {
        draw_ascii_text_at(
            info_x,
            start_y + line_index * CHAR_HEIGHT,
            line,
        );
    }

    // Place the normal shell cursor underneath the entire fastfetch display.
    unsafe {
        CURSOR_X = 0;
        CURSOR_Y = start_y + required_height + CHAR_HEIGHT;

        // If the calculated cursor lands beyond the screen, let the normal
        // scrolling code handle it.
        if CURSOR_Y + CHAR_HEIGHT > FRAMEBUFFER_HEIGHT {
            scroll();
        }
    }
}

/// Draw an ASCII string at an arbitrary framebuffer position.
///
/// This is deliberately separate from `print()` because fastfetch has two
/// independent visual columns. Moving the global terminal cursor for every
/// right-column line would make normal shell output difficult to reason about.
fn draw_ascii_text_at(x: usize, y: usize, text: &str) {
    let mut current_x = x;

    for byte in text.bytes() {
        // The fastfetch information is ASCII only.
        if !(32..=126).contains(&byte) {
            continue;
        }

        draw_glyph(byte, current_x, y);

        current_x += CHAR_WIDTH;

        // Stop if the text would run beyond the framebuffer.
        if current_x + CHAR_WIDTH > unsafe { FRAMEBUFFER_WIDTH } {
            break;
        }
    }
}

// -----------------------------------------------------------------------------
// BACKSPACE
// -----------------------------------------------------------------------------

/// Removes the character immediately before the current cursor position.
///
/// Backspace is deliberately implemented at the terminal level rather than
/// inside the keyboard driver.
///
/// The keyboard driver only needs to report:
///
///     '\x08'
///
/// The terminal then decides what deleting a character actually means.
pub fn backspace() {
    unsafe {
        // There is nothing to erase when the cursor is already at the
        // beginning of the current line.
        //
        // We intentionally do not move to the previous line yet.
        if CURSOR_X == 0 {
            return;
        }

        // Move the cursor one character cell backwards.
        CURSOR_X -= CHAR_WIDTH;

        // Clear the entire character cell.
        //
        // We clear the whole 8x8 region rather than only the glyph pixels.
        // This is important because otherwise pixels from the deleted
        // character could remain visible.
        for y in CURSOR_Y..CURSOR_Y + CHAR_HEIGHT {
            for x in CURSOR_X..CURSOR_X + CHAR_WIDTH {
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
}

// -----------------------------------------------------------------------------
// SCREEN SCROLLING
// -----------------------------------------------------------------------------

/// Scrolls the framebuffer upward by one character row.
///
/// The newly available bottom row is cleared.
fn scroll() {
    unsafe {
        // If the framebuffer is smaller than one character cell vertically,
        // simply reset and clear it.
        if FRAMEBUFFER_HEIGHT <= CHAR_HEIGHT {
            CURSOR_Y = 0;
            clear();
            return;
        }

        let width = FRAMEBUFFER_WIDTH;
        let height = FRAMEBUFFER_HEIGHT;
        let pitch = FRAMEBUFFER_PITCH;
        let bytes_per_pixel = BYTES_PER_PIXEL;

        // Move every framebuffer row upward by CHAR_HEIGHT pixels.
        //
        // `ptr::copy` behaves like memmove and therefore correctly handles
        // overlapping source and destination regions.
        for y in CHAR_HEIGHT..height {
            let source_offset = y * pitch;
            let destination_offset = (y - CHAR_HEIGHT) * pitch;

            core::ptr::copy(
                FRAMEBUFFER_ADDRESS.add(source_offset),
                FRAMEBUFFER_ADDRESS.add(destination_offset),
                width * bytes_per_pixel,
            );
        }

        // Clear the newly created bottom character row.
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

        // Place the cursor on the newly created bottom line.
        CURSOR_Y = height - CHAR_HEIGHT;
    }
}

// -----------------------------------------------------------------------------
// CHARACTER OUTPUT
// -----------------------------------------------------------------------------

/// Prints one ASCII character to the terminal.
///
/// Control characters are handled here as well, making this function useful
/// for keyboard drivers and other kernel subsystems.
pub fn print_char(character: char) {
    match character {
        // Enter/newline.
        '\n' => newline(),

        // Carriage return.
        '\r' => carriage_return(),

        // Backspace.
        //
        // This allows other kernel subsystems to send the control character
        // directly to the terminal if desired.
        '\x08' => backspace(),

        // Printable ASCII character.
        character if character.is_ascii() && character >= ' ' => {
            put_char(character as u8);
        }

        // Ignore unsupported characters.
        _ => {}
    }
}

/// Draws one ASCII character at the current cursor position.
fn put_char(character: u8) {
    // Make sure the character fits on the current line.
    ensure_horizontal_space();

    // Read the current cursor position.
    let (x, y) = unsafe {
        (CURSOR_X, CURSOR_Y)
    };

    // Render the glyph.
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
/// Each byte represents one horizontal row of the glyph.
///
/// Each bit represents one pixel:
///
/// - Bit 7 = leftmost pixel
/// - Bit 0 = rightmost pixel
fn draw_glyph(character: u8, x: usize, y: usize) {
    let glyph = glyph_for(character);

    // Each glyph contains exactly eight rows.
    for row in 0..8 {
        let bits = glyph[row];

        // Each row contains exactly eight pixels.
        for column in 0..8 {
            // Determine whether this pixel belongs to the glyph.
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
/// Unsupported characters use a visible box so that missing glyphs are easy
/// to identify during kernel development.
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

        // Unsupported characters are displayed as a box.
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
/// The framebuffer must have been initialized by [`init()`].
///
/// Coordinates outside the framebuffer are ignored.
///
/// The framebuffer is expected to use a 32-bit BGR-style pixel layout.
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

        // Calculate the byte offset of the pixel.
        //
        // The pitch is used instead of simply multiplying the width because
        // framebuffer rows may contain padding.
        let offset =
            y * FRAMEBUFFER_PITCH +
            x * BYTES_PER_PIXEL;

        // Write the blue channel.
        FRAMEBUFFER_ADDRESS
            .add(offset)
            .write_volatile(blue);

        // Write the green channel.
        FRAMEBUFFER_ADDRESS
            .add(offset + 1)
            .write_volatile(green);

        // Write the red channel.
        FRAMEBUFFER_ADDRESS
            .add(offset + 2)
            .write_volatile(red);

        // The fourth byte is currently unused.
        FRAMEBUFFER_ADDRESS
            .add(offset + 3)
            .write_volatile(0);
    }
}

// ============================================================================
// END OF OXIDE TERMINAL
// ============================================================================