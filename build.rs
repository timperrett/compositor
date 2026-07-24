fn main() {
    println!(
        "cargo:rustc-env=COMPOSITOR_BUILD_VERSION={}",
        env!("CARGO_PKG_VERSION")
    );
}
