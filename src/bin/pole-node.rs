use std::env;

fn main() {
    if let Err(err) = pole_protocol_draft::cli_node::run(&env::args().collect::<Vec<_>>()) {
        eprintln!("pole-node error: {err}");
        std::process::exit(1);
    }
}
