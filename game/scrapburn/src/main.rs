#![forbid(unsafe_code)]
//! Scrapburn: drive the gray-box loop.

mod app;

use std::process::ExitCode;

fn main() -> ExitCode {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info,wgpu_core=warn,wgpu_hal=warn,naga=warn"),
    )
    .init();
    let args = match scrapburn::args::parse(
        std::env::args().skip(1),
        std::env::var("RIVET_ADAPTER").ok(),
    ) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };
    match app::run(args) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}
