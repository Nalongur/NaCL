fn main() {
    let major = std::env::args()
        .nth(1)
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(21);
    match launcher_core::java::install_managed_runtime(major) {
        Ok(runtime) => {
            println!(
                "Java {} ready: {} ({})",
                runtime.version.as_deref().unwrap_or("unknown"),
                runtime.path,
                runtime.architecture.as_deref().unwrap_or("unknown")
            );
        }
        Err(error) => {
            eprintln!("Managed Java probe failed: {error}");
            std::process::exit(1);
        }
    }
}
