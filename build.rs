fn main() {
    println!("cargo:rerun-if-changed=packaging/windows/pole.ico");

    if std::env::var("CARGO_CFG_WINDOWS").is_ok() {
        let mut res = winres::WindowsResource::new();
        res.set_icon("packaging/windows/pole.ico");
        if let Err(error) = res.compile() {
            panic!("failed to compile Windows resources: {error}");
        }
    }
}
