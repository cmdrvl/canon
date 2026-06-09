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

    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            // Enrich an unknown-flag error with a did-you-mean naming the exact
            // corrected flag; otherwise defer to clap's own handling (which also
            // covers --help/--version and correct exit codes).
            if let Some(message) = canon::unknown_flag_suggestion(&error) {
                eprintln!("{message}");
                process::exit(2);
            }
            error.exit();
        }
    };
    match canon::run(cli) {
        Ok(exit_code) => process::exit(exit_code as i32),
        Err(e) => {
            eprintln!("Error: {}", e);
            process::exit(2);
        }
    }
}
