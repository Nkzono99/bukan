fn main() {
    match bukan_lib::cli::run() {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("error: {error}");
            std::process::exit(1);
        }
    }
}
