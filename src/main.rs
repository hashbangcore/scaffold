//! scaffold: materialize templates from a local repository (`$SCAFFOLD`).

mod cli;
mod discovery;
mod error;

fn main() -> std::process::ExitCode {
    match cli::run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err}");
            std::process::ExitCode::FAILURE
        }
    }
}
