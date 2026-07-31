fn main() {
    let runtimes = launcher_core::java::detect_java_runtimes();

    if runtimes.is_empty() {
        println!("No Java runtime detected.");
        return;
    }

    for runtime in runtimes {
        println!(
            "{} | {} | {} | {}",
            runtime.version.as_deref().unwrap_or("unknown version"),
            runtime
                .architecture
                .as_deref()
                .unwrap_or("unknown architecture"),
            runtime.source,
            runtime.path
        );
    }
}
