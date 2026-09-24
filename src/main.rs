//! ragtag process entry point.

fn main() -> std::process::ExitCode {
    env_logger::init();
    ragtag::run_process()
}
