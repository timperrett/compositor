use clap::Parser;
use compositor::migration::{self, MigrationOptions};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "compositor-migrate",
    about = "One-time import of legacy .compositor production state"
)]
struct Cli {
    #[arg(long, default_value = ".")]
    root: PathBuf,
    #[arg(long)]
    apply: bool,
}

fn main() {
    let cli = Cli::parse();
    match migration::run(&cli.root, MigrationOptions { apply: cli.apply }) {
        Ok(report) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&report).expect("report serializes")
            );
            if !report.blockers.is_empty() {
                std::process::exit(2);
            }
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
