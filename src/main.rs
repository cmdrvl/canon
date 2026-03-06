#![forbid(unsafe_code)]

use canon::cli::Cli;
use clap::Parser;
use std::process;

fn main() {
    if let Some(display_mode) = canon::detect_display_mode(std::env::args_os()) {
        match canon::run_display_mode(display_mode) {
            Ok(exit_code) => process::exit(exit_code as i32),
            Err(e) => {
                eprintln!("Error: {}", e);
                process::exit(2);
            }
        }
    }

    let cli = Cli::parse();
    match canon::run(cli) {
        Ok(exit_code) => process::exit(exit_code as i32),
        Err(e) => {
            eprintln!("Error: {}", e);
            process::exit(2);
        }
    }
}
