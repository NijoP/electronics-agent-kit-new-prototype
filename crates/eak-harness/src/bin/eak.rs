fn main() {
    std::process::exit(eak_harness::cli::main_with(
        std::env::args().skip(1).collect(),
    ));
}
