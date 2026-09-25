fn main() {
    let cli = omp_sbx::cli::Cli::parse_compat();
    if let Err(error) = omp_sbx::dispatch(cli) {
        eprintln!("omp-sbx: {error:#}");
        std::process::exit(1);
    }
}
