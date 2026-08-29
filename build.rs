use std::fs;
use std::path::Path;
use std::process::Command;

fn main() {
    // Re-run this build script whenever any part of the kernel,
    // kernel manifest, or Limine configuration changes.
    println!("cargo:rerun-if-changed=kernel/src");
    println!("cargo:rerun-if-changed=kernel/Cargo.toml");
    println!("cargo:rerun-if-changed=iso_root/boot/limine.conf");

    // The kernel is built separately for the bare-metal x86_64 target.
    //
    // We intentionally do not try to use CARGO_BIN_FILE_OXIDE_KERNEL here.
    // The kernel is a workspace member, not an artifact dependency of the
    // root package, so Cargo does not provide that environment variable.
    let kernel_path = Path::new(
        "target/x86_64-unknown-none/debug/oxide-kernel"
    );

    // Make sure the kernel was actually built before assembling the ISO.
    if !kernel_path.exists() {
        panic!(
            "Oxide kernel was not found at {}. \
             Build it first with: \
             cargo build -p oxide-kernel --target x86_64-unknown-none",
            kernel_path.display()
        );
    }

    // Limine expects the kernel at /boot/oxide-kernel according to
    // iso_root/boot/limine.conf.
    fs::create_dir_all("iso_root/boot")
        .expect("failed to create iso_root/boot");

    // Copy the freshly built kernel into the ISO filesystem.
    fs::copy(
        kernel_path,
        "iso_root/boot/oxide-kernel",
    )
    .expect("failed to copy Oxide kernel into ISO root");

    // Install the Limine UEFI executable into the standard EFI path.
    //
    // This allows the resulting ISO to boot through UEFI firmware.
    fs::create_dir_all("iso_root/EFI/BOOT")
        .expect("failed to create iso_root/EFI/BOOT");

    fs::copy(
        "limine/bin/BOOTX64.EFI",
        "iso_root/EFI/BOOT/BOOTX64.EFI",
    )
    .expect("failed to copy Limine UEFI executable");

    // Create the ISO filesystem.
    //
    // xorriso takes the contents of iso_root/ and creates oxide.iso.
    let status = Command::new("xorriso")
        .args([
            "-as",
            "mkisofs",
            "-b",
            "limine-bios-cd.bin",
            "-no-emul-boot",
            "-boot-load-size",
            "4",
            "-boot-info-table",
            "--efi-boot",
            "EFI/BOOT/BOOTX64.EFI",
            "-efi-boot-part",
            "--efi-boot-image",
            "-o",
            "oxide.iso",
            "iso_root",
        ])
        .status()
        .expect(
            "failed to execute xorriso. \
             Make sure xorriso is installed."
        );

    if !status.success() {
        panic!("xorriso failed while creating oxide.iso");
    }

    // Install the Limine BIOS bootloader into the ISO.
    let status = Command::new("./limine/bin/limine")
        .args([
            "bios-install",
            "oxide.iso",
        ])
        .status()
        .expect("failed to execute Limine");

    if !status.success() {
        panic!("Limine failed while installing the BIOS bootloader");
    }

    println!("cargo:warning=Oxide ISO created successfully: oxide.iso");
}
