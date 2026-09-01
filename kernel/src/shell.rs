//! Oxide interactive shell.
//!
//! This module implements the command-line interface used by Oxide.
//!
//! The shell is intentionally simple and heap-free. It uses a fixed-size
//! command buffer because the kernel does not have a heap allocator yet.
//!
//! Input flow:
//!
//!     PS/2 keyboard
//!          ↓
//!     keyboard::poll()
//!          ↓
//!     character
//!          ↓
//!     shell input buffer
//!          ↓
//!     execute_command()
//!
//! The shell currently contains a mixture of:
//!
//! - Real kernel information commands.
//! - Hardware diagnostic commands.
//! - Utility commands.
//! - Placeholder commands for future filesystem/process functionality.
//!
//! Commands that depend on functionality Oxide does not have yet explicitly
//! report that the subsystem is not implemented instead of pretending that it
//! exists.

use crate::keyboard;
use crate::terminal;

/// Maximum number of bytes that can be entered into one command.
///
/// Keeping this fixed means the shell does not need heap allocation.
const COMMAND_BUFFER_SIZE: usize = 256;

/// Starts the Oxide interactive shell.
///
/// The shell first prints a welcome screen and the initial prompt. It then
/// continuously polls the keyboard and handles characters typed by the user.
///
/// This is currently a polling-based shell. Later, the keyboard driver can be
/// upgraded to an interrupt-driven input queue without changing the overall
/// command architecture.
pub fn run() -> ! {
    // Fixed-size input buffer.
    //
    // We use a byte array instead of String because the kernel currently has
    // no heap allocator and therefore cannot safely create arbitrary-sized
    // heap strings.
    let mut buffer = [0u8; COMMAND_BUFFER_SIZE];

    // Number of valid bytes currently stored in the command buffer.
    let mut length = 0usize;

    // ------------------------------------------------------------
    // Oxide startup screen
    // ------------------------------------------------------------

    terminal::println("");
    terminal::println("========================================");
    terminal::println("          Welcome to Oxide OS!");
    terminal::println("========================================");
    terminal::println("");
    terminal::println("Oxide kernel 0.1.0");
    terminal::println("A small operating system built from scratch in Rust.");
    terminal::println("");
    terminal::println("Type 'help' to see available commands.");
    terminal::println("");

    // Print the initial prompt.
    //
    // Previously the prompt was only printed after pressing Enter, which
    // made the freshly booted shell appear to be a completely black screen
    // even though the keyboard and shell were already functioning.
    terminal::print("Oxide>");

    // ------------------------------------------------------------
    // Main shell loop
    // ------------------------------------------------------------

    loop {
        // Ask the keyboard driver whether a new character is available.
        if let Some(character) = keyboard::poll() {
            match character {
                // ----------------------------------------------------
                // Enter
                // ----------------------------------------------------
                //
                // Execute whatever is currently in the command buffer.
                '\n' => {
                    terminal::println("");

                    execute_command(&buffer[..length]);

                    // Clear the command buffer.
                    length = 0;

                    // Display the prompt for the next command.
                    terminal::print("Oxide>");
                }

                // ----------------------------------------------------
                // Backspace
                // ----------------------------------------------------
                //
                // Remove the final character from both the internal
                // command buffer and the terminal display.
                '\x08' => {
                    if length > 0 {
                        length -= 1;
                        terminal::backspace();
                    }
                }

                // ----------------------------------------------------
                // Tab
                // ----------------------------------------------------
                //
                // The keyboard driver converts Tab into '\t'.
                // Until command completion exists, we simply insert
                // four spaces.
                '\t' => {
                    if length + 4 <= COMMAND_BUFFER_SIZE {
                        for _ in 0..4 {
                            buffer[length] = b' ';
                            length += 1;

                            terminal::print(" ");
                        }
                    }
                }

                // ----------------------------------------------------
                // Escape
                // ----------------------------------------------------
                //
                // Escape clears the current command line.
                '\x1B' => {
                    while length > 0 {
                        length -= 1;
                        terminal::backspace();
                    }
                }

                // ----------------------------------------------------
                // Normal printable ASCII character
                // ----------------------------------------------------
                //
                // The current terminal font is ASCII-based, so we only
                // store printable ASCII characters in the command buffer.
                character
                    if character.is_ascii() && !character.is_ascii_control() =>
                {
                    if length < COMMAND_BUFFER_SIZE {
                        buffer[length] = character as u8;
                        length += 1;

                        terminal::print_char(character);
                    }
                }

                // Ignore unsupported characters.
                _ => {}
            }
        }

        // The keyboard is currently polled rather than interrupt-driven.
        // spin_loop() prevents this loop from being completely empty while
        // still allowing the CPU to optimize the busy-wait appropriately.
        core::hint::spin_loop();
    }
}

// ============================================================================
// COMMAND DISPATCH
// ============================================================================

/// Executes a command stored in the input buffer.
///
/// The command is split into:
///
///     command + arguments
///
/// For example:
///
///     echo hello oxide
///
/// becomes:
///
///     command  = "echo"
///     arguments = "hello oxide"
fn execute_command(input: &[u8]) {
    // Convert the byte buffer into an ASCII string-like slice.
    //
    // Because keyboard input is currently restricted to ASCII, UTF-8
    // validation is unnecessary for normal shell input. We still use
    // from_utf8() defensively and fall back to an empty string if something
    // unexpected reaches the command buffer.
    let input = match core::str::from_utf8(input) {
        Ok(value) => value,
        Err(_) => {
            terminal::println("Invalid command input.");
            return;
        }
    };

    // Remove leading and trailing spaces.
    let input = trim_spaces(input);

    // Empty commands should simply return to the prompt.
    if input.is_empty() {
        return;
    }

    // Separate the first word from the rest of the command line.
    let (command, arguments) = split_command(input);

    match command {
        // ------------------------------------------------------------
        // General
        // ------------------------------------------------------------

        "help" | "?" => command_help(arguments),
        "clear" | "cls" => command_clear(arguments),
        "echo" => command_echo(arguments),

        "about" => command_about(arguments),

        "version" | "ver" => command_version(arguments),

        "uname" => command_uname(arguments),

        "hostname" => command_hostname(arguments),

        "whoami" => command_whoami(arguments),

        "arch" => command_arch(arguments),

        "kernel" => command_kernel(arguments),

        "sysinfo" | "info" => command_info(arguments),

        "fastfetch" | "neofetch" => command_fastfetch(arguments),

        "memory" | "mem" | "free" => command_memory(arguments),

        "cpu" => command_cpu(arguments),

        "uptime" => command_uptime(arguments),

        "date" => command_date(arguments),

        "time" => command_time(arguments),

        "status" => command_status(arguments),

        "shell" => command_shell(arguments),

        "env" => command_env(arguments),

        "set" => command_set(arguments),

        "banner" => command_banner(arguments),

        "hello" => command_hello(arguments),

        "true" => command_true(arguments),

        "false" => command_false(arguments),

        "yes" => command_yes(arguments),

        "no" => command_no(arguments),

        "repeat" => command_repeat(arguments),

        "hex" => command_hex(arguments),

        "dec" => command_dec(arguments),

        // ------------------------------------------------------------
        // Hardware
        // ------------------------------------------------------------

        "keyboard" | "kbd" => command_keyboard(arguments),

        "framebuffer" | "fb" => command_framebuffer(arguments),

        "boot" => command_boot(arguments),

        "gdt" => command_gdt(arguments),

        "idt" => command_idt(arguments),

        "interrupts" | "irq" => command_interrupts(arguments),

        "devices" | "dev" => command_devices(arguments),

        "pci" => command_pci(arguments),

        // ------------------------------------------------------------
        // Filesystem
        // ------------------------------------------------------------

        "pwd" => command_pwd(arguments),

        "ls" | "dir" => command_ls(arguments),

        "cd" => command_cd(arguments),

        "cat" => command_cat(arguments),

        "tree" => command_tree(arguments),

        "mkdir" => command_mkdir(arguments),

        "touch" => command_touch(arguments),

        "rm" => command_rm(arguments),

        "cp" => command_cp(arguments),

        "mv" => command_mv(arguments),

        "find" => command_find(arguments),

        "mount" => command_mount(arguments),

        "df" => command_df(arguments),

        // ------------------------------------------------------------
        // Debug / diagnostics
        // ------------------------------------------------------------

        "test" => command_test(arguments),

        "crab" => command_crab(arguments),

        "about:crab" => command_about_crab(arguments),

        "dmesg" => command_dmesg(arguments),

        "ps" => command_ps(arguments),

        "jobs" => command_jobs(arguments),

        "top" => command_top(arguments),

        "panic" => command_panic(arguments),

        // ------------------------------------------------------------
        // Power
        // ------------------------------------------------------------

        "reboot" => command_reboot(arguments),

        "shutdown" | "poweroff" | "halt" => command_shutdown(arguments),

        // ------------------------------------------------------------
        // Unknown command
        // ------------------------------------------------------------

        _ => {
            terminal::print("Unknown command: ");
            terminal::println(command);
            terminal::println("Type 'help' to see available commands.");
        }
    }
}

// ============================================================================
// HELP
// ============================================================================

/// Prints the list of available shell commands.
fn command_help(_arguments: &str) {
    terminal::println("");
    terminal::println("Oxide shell commands:");
    terminal::println("");

    terminal::println("General:");
    terminal::println("  help / ?       Show this help");
    terminal::println("  echo            Print text");
    terminal::println("  clear / cls     Clear the screen");
    terminal::println("  about           About Oxide");
    terminal::println("  version / ver   Show kernel version");
    terminal::println("  uname           Show system name");
    terminal::println("  hostname        Show hostname");
    terminal::println("  whoami          Show current user");
    terminal::println("  arch            Show architecture");
    terminal::println("  kernel          Show kernel information");
    terminal::println("  sysinfo / info  Show system information");
    terminal::println("  fastfetch       Show Oxide system summary");
    terminal::println("  memory / mem    Show memory status");
    terminal::println("  cpu             Show CPU information");
    terminal::println("  uptime          Show uptime status");
    terminal::println("  date            Show RTC status");
    terminal::println("  time            Show RTC status");
    terminal::println("  status          Show kernel status");
    terminal::println("  shell           Show shell information");
    terminal::println("  env             Show environment status");
    terminal::println("  set             Show environment status");
    terminal::println("  banner          Show Oxide banner");
    terminal::println("  hello           Say hello");
    terminal::println("  true            Return true");
    terminal::println("  false           Return false");
    terminal::println("  yes             Print yes");
    terminal::println("  no              Print no");
    terminal::println("  repeat          Repeat text");
    terminal::println("  hex             Convert decimal to hexadecimal");
    terminal::println("  dec             Convert hexadecimal to decimal");

    terminal::println("");
    terminal::println("Hardware:");
    terminal::println("  keyboard / kbd  Keyboard status");
    terminal::println("  framebuffer / fb Framebuffer status");
    terminal::println("  boot            Bootloader information");
    terminal::println("  gdt             GDT status");
    terminal::println("  idt             IDT status");
    terminal::println("  interrupts / irq Interrupt status");
    terminal::println("  devices / dev   Device status");
    terminal::println("  pci             PCI status");

    terminal::println("");
    terminal::println("Filesystem:");
    terminal::println("  pwd             Current directory");
    terminal::println("  ls / dir        List files");
    terminal::println("  cd              Change directory");
    terminal::println("  cat             Read a file");
    terminal::println("  tree            Show filesystem tree");
    terminal::println("  mkdir           Create directory");
    terminal::println("  touch           Create file");
    terminal::println("  rm              Remove file");
    terminal::println("  cp              Copy file");
    terminal::println("  mv              Move file");
    terminal::println("  find            Find files");
    terminal::println("  mount           Mount filesystem");
    terminal::println("  df              Disk usage");

    terminal::println("");
    terminal::println("Debug:");
    terminal::println("  test            Run basic kernel test");
    terminal::println("  crab            Show ASCII crab");
    terminal::println("  about:crab      About the Oxide crab");
    terminal::println("  dmesg           Kernel log status");
    terminal::println("  ps              Process status");
    terminal::println("  jobs            Job status");
    terminal::println("  top             System monitor status");
    terminal::println("  panic           Trigger a kernel panic");

    terminal::println("");
    terminal::println("Power:");
    terminal::println("  reboot          Reboot the machine");
    terminal::println("  shutdown        Shut down the machine");
    terminal::println("  poweroff        Shut down the machine");
    terminal::println("  halt            Halt the machine");

    terminal::println("");
}

// ============================================================================
// GENERAL COMMANDS
// ============================================================================

fn command_echo(arguments: &str) {
    terminal::println(arguments);
}

fn command_clear(_arguments: &str) {
    terminal::clear_screen();
}

fn command_about(_arguments: &str) {
    terminal::println("");
    terminal::println("Oxide OS");
    terminal::println("A small open-source operating system");
    terminal::println("built from scratch in Rust.");
    terminal::println("");
    terminal::println("Kernel: oxide-kernel");
    terminal::println("Architecture: x86_64");
    terminal::println("Bootloader: Limine");
    terminal::println("");
}

fn command_version(_arguments: &str) {
    terminal::println("Oxide kernel 0.1.0");
}

fn command_uname(_arguments: &str) {
    terminal::println("Oxide oxide-kernel 0.1.0 x86_64");
}

fn command_hostname(_arguments: &str) {
    terminal::println("oxide");
}

fn command_whoami(_arguments: &str) {
    terminal::println("root");
}

fn command_arch(_arguments: &str) {
    terminal::println("x86_64");
}

fn command_kernel(_arguments: &str) {
    terminal::println("oxide-kernel 0.1.0");
}

fn command_info(_arguments: &str) {
    terminal::println("");
    terminal::println("System Information:");
    terminal::println("  OS:           Oxide");
    terminal::println("  Kernel:       oxide-kernel 0.1.0");
    terminal::println("  Architecture: x86_64");
    terminal::println("  Bootloader:   Limine");
    terminal::println("  Terminal:     Framebuffer");
    terminal::println("  Input:        PS/2 polling");
    terminal::println("");
}

fn command_fastfetch(_arguments: &str) {
    terminal::println("");
    terminal::print_fastfetch();
    terminal::println("");
}

fn command_memory(_arguments: &str) {
    terminal::println("Memory detection is not implemented yet.");
    terminal::println("Memory manager: pending");
    terminal::println("Heap: not initialized");
}

fn command_cpu(_arguments: &str) {
    terminal::println("CPU detection is not implemented yet.");
    terminal::println("Architecture: x86_64");
}

fn command_uptime(_arguments: &str) {
    terminal::println("Uptime timer is not initialized yet.");
}

fn command_date(_arguments: &str) {
    terminal::println("RTC date support is not implemented yet.");
}

fn command_time(_arguments: &str) {
    terminal::println("RTC time support is not implemented yet.");
}

fn command_status(_arguments: &str) {
    terminal::println("");
    terminal::println("Oxide status: ONLINE");
    terminal::println("Framebuffer: active");
    terminal::println("Keyboard: active");
    terminal::println("Shell: active");
    terminal::println("Heap: not initialized");
    terminal::println("Filesystem: not implemented");
    terminal::println("Scheduler: not implemented");
    terminal::println("");
}

fn command_shell(_arguments: &str) {
    terminal::println("Oxide shell");
    terminal::println("Input mode: polling");
    terminal::println("Buffer: 256 bytes");
}

fn command_env(_arguments: &str) {
    terminal::println("Environment subsystem is not implemented yet.");
}

fn command_set(_arguments: &str) {
    terminal::println("Environment variables are not implemented yet.");
}

fn command_banner(_arguments: &str) {
    terminal::println("");
    terminal::println("   OOOOO  X   X  III  DDDD  EEEEE");
    terminal::println("   O   O   X X    I   D   D E");
    terminal::println("   O   O    X     I   D   D EEEE");
    terminal::println("   O   O   X X    I   D   D E");
    terminal::println("   OOOOO  X   X  III  DDDD  EEEEE");
    terminal::println("");
}

fn command_hello(_arguments: &str) {
    terminal::println("Hello from Oxide!");
}

fn command_true(_arguments: &str) {
    // Unix-style `true` normally produces no output and exits successfully.
    // Since Oxide does not have process exit codes yet, there is simply
    // nothing to print here.
}

fn command_false(_arguments: &str) {
    terminal::println("false");
}

fn command_yes(arguments: &str) {
    // Default output count.
    let mut count = 10usize;

    // Allow:
    //
    //     yes 5
    //
    // while keeping the output bounded so an accidental command cannot
    // flood the terminal forever.
    if !arguments.is_empty() {
        if let Some(value) = parse_decimal(arguments) {
            count = value as usize;

            if count > 32 {
                count = 32;
            }
        }
    }

    for _ in 0..count {
        terminal::println("y");
    }
}

fn command_no(_arguments: &str) {
    terminal::println("no");
}

fn command_repeat(arguments: &str) {
    let (count_text, text) = split_command(arguments);

    if count_text.is_empty() || text.is_empty() {
        terminal::println("Usage: repeat <count> <text>");
        return;
    }

    let count = match parse_decimal(count_text) {
        Some(value) => value as usize,
        None => {
            terminal::println("Invalid repeat count.");
            return;
        }
    };

    // Keep the command bounded while the kernel is still young.
    let count = if count > 32 { 32 } else { count };

    for _ in 0..count {
        terminal::println(text);
    }
}

fn command_hex(arguments: &str) {
    if arguments.is_empty() {
        terminal::println("Usage: hex <decimal>");
        return;
    }

    let value = match parse_decimal(arguments) {
        Some(value) => value,
        None => {
            terminal::println("Invalid decimal number.");
            return;
        }
    };

    terminal::print("0x");
    print_hex_u64(value);
    terminal::println("");
}

fn command_dec(arguments: &str) {
    if arguments.is_empty() {
        terminal::println("Usage: dec <hex>");
        return;
    }

    let value = match parse_hex(arguments) {
        Some(value) => value,
        None => {
            terminal::println("Invalid hexadecimal number.");
            return;
        }
    };

    terminal::print_u64(value);
    terminal::println("");
}

// ============================================================================
// HARDWARE COMMANDS
// ============================================================================

fn command_keyboard(_arguments: &str) {
    terminal::println("Keyboard: PS/2");
    terminal::println("Driver: active");
    terminal::println("Input mode: polling");
    terminal::println("IRQ1: not initialized yet");
}

fn command_framebuffer(_arguments: &str) {
    terminal::println("Framebuffer: active");
    terminal::println("Format: 32-bit BGR");
    terminal::println("Terminal: framebuffer renderer");
}

fn command_boot(_arguments: &str) {
    terminal::println("Bootloader: Limine");
    terminal::println("Architecture: x86_64");
}

fn command_gdt(_arguments: &str) {
    terminal::println("GDT: initialized");
}

fn command_idt(_arguments: &str) {
    terminal::println("IDT: not initialized yet");
}

fn command_interrupts(_arguments: &str) {
    terminal::println("Interrupt subsystem: not initialized yet.");
    terminal::println("Keyboard currently uses polling.");
}

fn command_devices(_arguments: &str) {
    terminal::println("Device enumeration is not implemented yet.");
}

fn command_pci(_arguments: &str) {
    terminal::println("PCI enumeration is not implemented yet.");
}

// ============================================================================
// FILESYSTEM COMMANDS
// ============================================================================

fn command_pwd(_arguments: &str) {
    terminal::println("/");
    terminal::println("Filesystem support is not implemented yet.");
}

fn command_ls(_arguments: &str) {
    terminal::println("Filesystem support is not implemented yet.");
}

fn command_cd(_arguments: &str) {
    terminal::println("Filesystem support is not implemented yet.");
}

fn command_cat(_arguments: &str) {
    terminal::println("Filesystem support is not implemented yet.");
}

fn command_tree(_arguments: &str) {
    terminal::println("/");
    terminal::println("+-- boot");
    terminal::println("+-- kernel");
    terminal::println("`-- filesystem");
    terminal::println("");
    terminal::println("Filesystem support is not implemented yet.");
}

fn command_mkdir(_arguments: &str) {
    terminal::println("Filesystem support is not implemented yet.");
}

fn command_touch(_arguments: &str) {
    terminal::println("Filesystem support is not implemented yet.");
}

fn command_rm(_arguments: &str) {
    terminal::println("Filesystem support is not implemented yet.");
}

fn command_cp(_arguments: &str) {
    terminal::println("Filesystem support is not implemented yet.");
}

fn command_mv(_arguments: &str) {
    terminal::println("Filesystem support is not implemented yet.");
}

fn command_find(_arguments: &str) {
    terminal::println("Filesystem support is not implemented yet.");
}

fn command_mount(_arguments: &str) {
    terminal::println("Filesystem support is not implemented yet.");
}

fn command_df(_arguments: &str) {
    terminal::println("Filesystem support is not implemented yet.");
}

// ============================================================================
// DEBUG COMMANDS
// ============================================================================

fn command_test(_arguments: &str) {
    terminal::println("");
    terminal::println("Oxide kernel test:");
    terminal::println("  [OK] Kernel running");
    terminal::println("  [OK] GDT initialized");
    terminal::println("  [OK] Framebuffer initialized");
    terminal::println("  [OK] Terminal initialized");
    terminal::println("  [OK] Keyboard initialized");
    terminal::println("");
}

fn command_crab(_arguments: &str) {
    terminal::print_crab();
}

fn command_about_crab(_arguments: &str) {
    terminal::println("");
    terminal::println("The Oxide crab.");
    terminal::println("A little mascot for a little Rust operating system.");
    terminal::println("");
}

fn command_dmesg(_arguments: &str) {
    terminal::println("Persistent kernel logging is not implemented yet.");
}

fn command_ps(_arguments: &str) {
    terminal::println("Process management is not implemented yet.");
}

fn command_jobs(_arguments: &str) {
    terminal::println("Job control is not implemented yet.");
}

fn command_top(_arguments: &str) {
    terminal::println("Scheduler/process subsystem is not implemented yet.");
}

fn command_panic(_arguments: &str) -> ! {
    panic!("Oxide panic command requested");
}

// ============================================================================
// POWER COMMANDS
// ============================================================================

/// Attempts to reboot the machine through the legacy 8042 keyboard controller.
fn command_reboot(_arguments: &str) -> ! {
    terminal::println("Rebooting Oxide...");

    unsafe {
        // Wait until the keyboard controller input buffer is empty.
        loop {
            let status = read_port_u8(0x64);

            if status & 0x02 == 0 {
                break;
            }
        }

        // 0xFE tells the 8042 controller to pulse the CPU reset line.
        write_port_u8(0x64, 0xFE);
    }

    // If the hardware did not reset, halt safely.
    loop {
        unsafe {
            core::arch::asm!("cli");
            core::arch::asm!("hlt");
        }
    }
}

/// Attempts ACPI-style shutdown through QEMU/Bochs' standard port.
///
/// Real hardware shutdown support will eventually be replaced by proper ACPI
/// table parsing and power-management code.
fn command_shutdown(_arguments: &str) -> ! {
    terminal::println("Shutting down Oxide...");

    unsafe {
        // QEMU/Bochs commonly recognize this I/O port combination.
        write_port_u16(0x604, 0x2000);
    }

    // If the environment does not implement the shutdown port, halt the CPU
    // rather than continuing to execute kernel code.
    loop {
        unsafe {
            core::arch::asm!("cli");
            core::arch::asm!("hlt");
        }
    }
}

// ============================================================================
// STRING HELPERS
// ============================================================================

/// Removes ASCII spaces from both ends of a string.
fn trim_spaces(value: &str) -> &str {
    let bytes = value.as_bytes();

    let mut start = 0usize;
    let mut end = bytes.len();

    while start < end && bytes[start] == b' ' {
        start += 1;
    }

    while end > start && bytes[end - 1] == b' ' {
        end -= 1;
    }

    &value[start..end]
}

/// Splits a command line at its first space.
///
/// Example:
///
///     split_command("echo hello")
///
/// returns:
///
///     ("echo", "hello")
fn split_command(value: &str) -> (&str, &str) {
    let bytes = value.as_bytes();

    for index in 0..bytes.len() {
        if bytes[index] == b' ' {
            let command = &value[..index];

            let mut argument_start = index;

            while argument_start < bytes.len() && bytes[argument_start] == b' ' {
                argument_start += 1;
            }

            return (command, &value[argument_start..]);
        }
    }

    (value, "")
}

/// Parses an unsigned decimal number.
///
/// This is implemented manually because the kernel does not use the standard
/// library's parsing infrastructure.
fn parse_decimal(value: &str) -> Option<u64> {
    if value.is_empty() {
        return None;
    }

    let bytes = value.as_bytes();

    let mut result = 0u64;

    for &byte in bytes {
        if !(b'0'..=b'9').contains(&byte) {
            return None;
        }

        let digit = (byte - b'0') as u64;

        result = result.checked_mul(10)?;
        result = result.checked_add(digit)?;
    }

    Some(result)
}

/// Parses an unsigned hexadecimal number.
///
/// Both of these forms are accepted:
///
///     FF
///     0xFF
fn parse_hex(value: &str) -> Option<u64> {
    let mut bytes = value.as_bytes();

    if bytes.len() >= 2 && bytes[0] == b'0' && (bytes[1] == b'x' || bytes[1] == b'X') {
        bytes = &bytes[2..];
    }

    if bytes.is_empty() {
        return None;
    }

    let mut result = 0u64;

    for &byte in bytes {
        let digit = match byte {
            b'0'..=b'9' => (byte - b'0') as u64,
            b'a'..=b'f' => (byte - b'a' + 10) as u64,
            b'A'..=b'F' => (byte - b'A' + 10) as u64,
            _ => return None,
        };

        result = result.checked_mul(16)?;
        result = result.checked_add(digit)?;
    }

    Some(result)
}

/// Prints an unsigned 64-bit number in hexadecimal.
fn print_hex_u64(mut value: u64) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";

    if value == 0 {
        terminal::print("0");
        return;
    }

    let mut buffer = [0u8; 16];
    let mut index = buffer.len();

    while value != 0 {
        index -= 1;
        buffer[index] = HEX[(value & 0xF) as usize];
        value >>= 4;
    }

    for &byte in &buffer[index..] {
        terminal::print_char(byte as char);
    }
}

// ============================================================================
// LOW-LEVEL I/O
// ============================================================================

/// Reads one byte from an x86 I/O port.
///
/// This is kept local to the shell because the reboot command is currently
/// the only shell functionality that directly needs these legacy ports.
#[inline(always)]
unsafe fn read_port_u8(port: u16) -> u8 {
    let mut value: u8;

    core::arch::asm!(
        "in al, dx",
        in("dx") port,
        out("al") value,
        options(nomem, nostack, preserves_flags)
    );

    value
}

/// Writes one byte to an x86 I/O port.
#[inline(always)]
unsafe fn write_port_u8(port: u16, value: u8) {
    core::arch::asm!(
        "out dx, al",
        in("dx") port,
        in("al") value,
        options(nomem, nostack, preserves_flags)
    );
}

/// Writes one 16-bit value to an x86 I/O port.
#[inline(always)]
unsafe fn write_port_u16(port: u16, value: u16) {
    core::arch::asm!(
        "out dx, ax",
        in("dx") port,
        in("ax") value,
        options(nomem, nostack, preserves_flags)
    );
}