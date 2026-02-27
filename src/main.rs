#![forbid(unsafe_code)]

use canon::cli::Cli;
use clap::Parser;
use std::process;

fn main() {
    let cli = Cli::parse();
    match canon::run(cli) {
        Ok(exit_code) => process::exit(exit_code as i32),
        Err(e) => {
            eprintln!("Error: {}", e);
            process::exit(2);
        }
    }
}
