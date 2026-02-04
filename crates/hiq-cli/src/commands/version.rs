//! Version command implementation.

use console::style;

/// Execute the version command.
pub fn execute() {
    let version = env!("CARGO_PKG_VERSION");

    println!(
        "{} {} - Rust-native quantum compilation and orchestration",
        style("HIQ").cyan().bold(),
        style(format!("v{}", version)).yellow()
    );
    println!();
    println!("Components:");
    println!("  hiq-ir       Circuit intermediate representation");
    println!("  hiq-compile  Compilation and transpilation framework");
    println!("  hiq-hal      Hardware abstraction layer");
    println!("  hiq-cli      Command-line interface");
    println!();
    println!(
        "Repository: {}",
        style("https://github.com/hiq-project/hiq").underlined()
    );
    println!("License:    {}", style("MIT OR Apache-2.0").dim());
}
