//! Oxide build package.
//!
//! The actual operating-system kernel lives in `kernel/`.
//! This package exists to build the bootable Oxide disk image.

fn main() {
    // The bootable image is created by build.rs.
    // The root binary itself does not need to do anything.
}