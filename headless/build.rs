use std::env;

fn main() {
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        set_windows_resource();
    }
}

#[cfg(windows)]
fn set_windows_resource() {
    let mut res = winresource::WindowsResource::new();
    res.set_language(0x0409);
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
    let icon_path = format!("{manifest_dir}/../app/assets/grimlite-ri.ico");
    println!("cargo:rerun-if-changed={icon_path}");
    res.set_icon(&icon_path);
    res.compile().expect("Failed to compile Windows icon resource");
}

#[cfg(not(windows))]
fn set_windows_resource() {}
