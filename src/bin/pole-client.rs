use std::env;

fn main() {
    if let Err(err) = pole_protocol_draft::cli_client::run(&env::args().collect::<Vec<_>>()) {
        eprintln!("pole-client error: {err}");
        std::process::exit(1);
    }
}
