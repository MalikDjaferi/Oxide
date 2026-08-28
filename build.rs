use std::{env, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=kernel/src");
    println!("cargo:rerun-if-changed=kernel/Cargo.toml");

    let kernel_path = PathBuf::from(
        env::var_os("CARGO_BIN_FILE_OXIDE_KERNEL")
            .expect("Oxide kernel binary artifact was not provided by Cargo"),
    );

    let out_dir = PathBuf::from(
        env::var_os("OUT_DIR")
            .expect("OUT_DIR was not provided by Cargo"),
    );

    let image_path = out_dir.join("oxide-bios.img");

    bootloader::BiosBoot::new(&kernel_path)
        .create_disk_image(&image_path)
        .expect("failed to create Oxide BIOS disk image");

    println!(
        "cargo:warning=Oxide BIOS image: {}",
        image_path.display()
    );
}