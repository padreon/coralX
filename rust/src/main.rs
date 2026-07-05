mod core;
mod models;

fn main() {
    core::logger::setup_logging();
    core::logger::install_panic_hook();
    log::info!("coralX starting");
    println!("coralX (Rust) — scaffold placeholder, UI not wired up yet");
}
