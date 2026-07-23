fn main() {
    if std::env::args().any(|argument| argument == "--bukan-mcp-stdio") {
        if let Err(error) = bukan_lib::mcp::run_stdio_from_environment() {
            eprintln!("Bukan MCP error: {error}");
            std::process::exit(1);
        }
        return;
    }
    bukan_lib::run();
}
