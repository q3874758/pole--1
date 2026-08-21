use std::process::ExitCode;

fn main() -> ExitCode {
    match pole_protocol_draft::cli_sbom::run(&std::env::args().collect::<Vec<_>>()) {
        Ok(0) => ExitCode::SUCCESS,
        Ok(code) => ExitCode::from(code as u8),
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
