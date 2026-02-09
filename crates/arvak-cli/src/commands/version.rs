//! Version command implementation.

use console::style;

/// Execute the version command.
pub fn execute() {
    let version = env!("CARGO_PKG_VERSION");

    println!(
        "{} {} - Rust-native quantum compilation and orchestration",
        style("Arvak").cyan().bold(),
        style(format!("v{version}")).yellow()
    );
    println!();
    println!("Components:");
    println!("  arvak-ir       Circuit intermediate representation");
    println!("  arvak-compile  Compilation and transpilation framework");
    println!("  arvak-hal      Hardware abstraction layer");
    println!("  arvak-cli      Command-line interface");
    println!();
    println!(
        "Repository: {}",
        style("https://github.com/hiq-lab/arvak").underlined()
    );
    println!("License:    {}", style("Apache-2.0").dim());
}
