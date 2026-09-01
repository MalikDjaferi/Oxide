//! ============================================================================
//! OXIDE BUILD PACKAGE
//! ============================================================================
//!
//! The actual Oxide operating-system kernel is located in:
//!
//!     kernel/
//!
//! This root package exists primarily so Cargo can execute `build.rs`, which
//! assembles the final bootable Oxide ISO.
//!
//! The root package itself is NOT the kernel and therefore contains no
//! operating-system code.
//!
//! ============================================================================

// This library intentionally contains no code.
//
// The actual bare-metal executable is:
//     oxide-kernel
//
// The root package exists to coordinate the build process and create the
// final bootable ISO.