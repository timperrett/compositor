fn main() {
    if let Err(error) = compositor::cli::run() {
        eprintln!("compositor: {error}");
        std::process::exit(error.exit_code());
    }
}
