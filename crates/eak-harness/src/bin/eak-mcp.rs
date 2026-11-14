//! `eak-mcp`: the MCP stdio server alone (same as `eak mcp`), for agent configs that want a
//! dedicated command.
fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    args.push("mcp".into());
    std::process::exit(eak_harness::cli::main_with(args));
}
