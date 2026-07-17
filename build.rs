use std::process::Command;

fn command_output(program: &str, arguments: &[&str]) -> Option<String> {
    let output = Command::new(program).args(arguments).output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout)
        .ok()
        .map(|value| value.trim().to_owned())
}

fn main() {
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs");

    let date = command_output("date", &["+%y.%j"]).unwrap_or_else(|| "00.000".into());
    let revision = command_output("git", &["rev-parse", "--short", "HEAD"])
        .unwrap_or_else(|| "unknown".into());

    println!("cargo:rustc-env=COMPOSITOR_BUILD_VERSION={date}-{revision}");
}
