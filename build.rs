//! This build script is responsible for embedding the application icon
//! into the executable on Windows. It uses the `winres` crate to achieve this.

fn main() {
    // We only need to run this build script on Windows.
    // The `CARGO_CFG_TARGET_OS` environment variable allows us to check the target OS.
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap() == "windows" {
        println!("cargo:rerun-if-changed=build.rs");
        println!("cargo:rerun-if-changed=icon.ico");

        // The `winres` crate helps in embedding Windows resources.
        // It requires a dependency to be added to Cargo.toml.
        let mut res = winres::WindowsResource::new();

        // The path to the icon file is relative to the project root.
        res.set_icon("icon.ico");

        // The `compile` method performs the embedding. We use `expect` here
        // to provide a more informative error message if something goes wrong.
        res.compile().expect("Failed to compile Windows resources with winres. Make sure you have the 'winres' dependency in your Cargo.toml and that the 'icon.ico' file exists.");
    }
}
