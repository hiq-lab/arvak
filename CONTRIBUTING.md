# Contributing to HIQ

Thank you for your interest in contributing to HIQ! This document provides guidelines
and information for contributors.

## Code of Conduct

This project adheres to a code of conduct. By participating, you are expected to
uphold respectful and inclusive behavior.

## Getting Started

### Prerequisites

- Rust 1.85+ (edition 2024)
- Git
- Optional: Python 3.8+ (for Python bindings)

### Setting Up Development Environment

```bash
# Clone the repository
git clone https://github.com/hiq-project/hiq.git
cd hiq

# Build all crates
cargo build

# Run tests
cargo test

# Build documentation
cargo doc --open
```

### Project Structure

```
HIQ/
├── crates/           # Core Rust crates
│   ├── hiq-ir/       # Circuit intermediate representation
│   ├── hiq-qasm3/    # OpenQASM 3.0 parser/emitter
│   ├── hiq-compile/  # Compilation framework
│   ├── hiq-hal/      # Hardware abstraction layer
│   ├── hiq-cli/      # Command-line interface
│   ├── hiq-sched/    # HPC scheduler
│   ├── hiq-types/    # Quantum types (QuantumInt, etc.)
│   ├── hiq-auto/     # Automatic uncomputation
│   └── hiq-python/   # Python bindings
├── adapters/         # Backend adapters
│   ├── hiq-adapter-sim/  # Statevector simulator
│   ├── hiq-adapter-iqm/  # IQM Quantum
│   └── hiq-adapter-ibm/  # IBM Quantum
├── demos/            # Demo applications
└── examples/         # Example QASM files
```

## How to Contribute

### Reporting Issues

Before creating an issue, please:

1. Search existing issues to avoid duplicates
2. Use the issue template if provided
3. Include:
   - HIQ version (`cargo --version`, `rustc --version`)
   - Operating system
   - Steps to reproduce
   - Expected vs actual behavior
   - Relevant error messages

### Submitting Pull Requests

1. **Fork the repository** and create a feature branch:
   ```bash
   git checkout -b feature/my-awesome-feature
   ```

2. **Make your changes** following our coding standards (see below)

3. **Add tests** for new functionality:
   ```bash
   cargo test -p <crate-name>
   ```

4. **Update documentation** if needed:
   - Add/update rustdoc comments
   - Update README if behavior changes
   - Update CHANGELOG.md

5. **Run the full test suite**:
   ```bash
   cargo test
   cargo clippy -- -D warnings
   cargo fmt --check
   ```

6. **Create a pull request** with:
   - Clear title describing the change
   - Description of what and why
   - Link to related issues

### Coding Standards

#### Rust Style

- Follow Rust idioms and best practices
- Use `cargo fmt` for formatting
- Use `cargo clippy` for linting
- Write documentation for public APIs

```rust
/// Brief description of the function.
///
/// Longer description if needed.
///
/// # Arguments
///
/// * `param` - Description of parameter
///
/// # Returns
///
/// Description of return value
///
/// # Errors
///
/// When this function can fail
///
/// # Example
///
/// ```rust
/// let result = my_function(arg)?;
/// ```
pub fn my_function(param: Type) -> Result<Output, Error> {
    // Implementation
}
```

#### Testing

- Write unit tests in the same file as the code (`#[cfg(test)]`)
- Write integration tests in `tests/` directory
- Aim for good coverage of edge cases
- Use descriptive test names

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_feature_basic_case() {
        // Arrange
        let input = /* ... */;

        // Act
        let result = function(input);

        // Assert
        assert_eq!(result, expected);
    }

    #[test]
    fn test_feature_edge_case() {
        // Test edge cases
    }
}
```

#### Commit Messages

Use clear, descriptive commit messages:

```
<type>(<scope>): <short description>

<longer description if needed>

<footer with issue references>
```

Types: `feat`, `fix`, `docs`, `style`, `refactor`, `test`, `chore`

Examples:
```
feat(compile): add commutative gate cancellation pass

Implement optimization pass that merges adjacent rotation gates
of the same type (e.g., RZ(θ₁)·RZ(θ₂) → RZ(θ₁+θ₂)).

Closes #42
```

## Development Areas

### High-Priority Contributions

- **New optimization passes**: Gate cancellation, template matching
- **Backend adapters**: Additional quantum hardware support
- **Error mitigation**: Readout error correction, ZNE improvements
- **Documentation**: Tutorials, examples, API docs

### Good First Issues

Look for issues labeled `good first issue` for beginner-friendly tasks.

## Testing Hardware Backends

### Simulator (always available)

```bash
cargo test -p hiq-adapter-sim
```

### IQM (requires token)

```bash
export IQM_TOKEN="your-token"
cargo test -p hiq-adapter-iqm -- --ignored
```

### IBM (requires token)

```bash
export IBM_QUANTUM_TOKEN="your-token"
cargo test -p hiq-adapter-ibm -- --ignored
```

## Building Documentation

```bash
# Build all documentation
cargo doc --all --no-deps

# Build with private items (for development)
cargo doc --all --no-deps --document-private-items

# Open in browser
cargo doc --open
```

## Release Process

Releases are managed by maintainers:

1. Update version in all `Cargo.toml` files
2. Update `CHANGELOG.md`
3. Create a git tag
4. Push to trigger CI/CD

## Questions?

- Open a GitHub issue for technical questions
- Check existing documentation and issues first

## License

By contributing, you agree that your contributions will be licensed under
the same license as the project (MIT OR Apache-2.0).

---

Thank you for contributing to HIQ!
