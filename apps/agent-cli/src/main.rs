mod app;
mod driver;
mod errors;
mod loop_driver;
mod model;
mod provider_setup;
mod theme;
mod tui;

use std::process::ExitCode;

fn main() -> ExitCode {
    match app::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
