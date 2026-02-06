# Contributing to HIQ

Thank you for your interest in contributing to HIQ! This document provides guidelines and information for contributors.

## Code of Conduct

We are committed to providing a welcoming and inclusive environment. Please be respectful and constructive in all interactions.

## Getting Started

### Prerequisites

- Rust 1.83 or later
- Python 3.11+ (for Python bindings)
- Git

### Development Setup

```bash
# Clone the repository
git clone https://github.com/hiq-project/hiq
cd hiq

# Build all crates
cargo build

# Run tests
cargo test

# Run clippy (linter)
cargo clippy --all-targets

# Format code
cargo fmt
```

### IDE Setup

**VS Code:**
```json
// .vscode/settings.json
{
    "rust-analyzer.cargo.features": "all",
    "rust-analyzer.checkOnSave.command": "clippy"
}
```

**RustRover/IntelliJ:**
- Install Rust plugin
- Enable clippy on save

## Project Structure

```
hiq/
├── crates/
│   ├── hiq-ir/          # Circuit IR
│   ├── hiq-qasm3/       # QASM3 parser
│   ├── hiq-compile/     # Compilation passes
│   ├── hiq-auto/        # Automatic uncomputation
│   ├── hiq-types/       # High-level types
│   ├── hiq-hal/         # Hardware abstraction
│   ├── hiq-sched/       # Scheduler integration
│   ├── hiq-core/        # Re-exports
│   ├── hiq-cli/         # CLI
│   └── hiq-python/      # Python bindings
├── adapters/
│   ├── hiq-adapter-iqm/
│   ├── hiq-adapter-ibm/
│   └── hiq-adapter-sim/
├── examples/
├── benches/
├── tests/
└── docs/
```

## Development Workflow

### Branching Strategy

- `main` — Stable release branch
- `develop` — Integration branch
- `feature/*` — Feature branches
- `fix/*` — Bug fix branches

### Making Changes

1. **Fork the repository** and clone your fork
2. **Create a feature branch:**
   ```bash
   git checkout -b feature/my-feature develop
   ```
3. **Make your changes** with clear, atomic commits
4. **Write tests** for new functionality
5. **Run the test suite:**
   ```bash
   cargo test --all
   ```
6. **Run lints:**
   ```bash
   cargo clippy --all-targets -- -D warnings
   cargo fmt --check
   ```
7. **Push and create a Pull Request**

### Commit Messages

Follow conventional commits:

```
type(scope): description

[optional body]

[optional footer]
```

Types:
- `feat` — New feature
- `fix` — Bug fix
- `docs` — Documentation
- `refactor` — Code refactoring
- `test` — Adding tests
- `chore` — Maintenance tasks

Examples:
```
feat(ir): add support for controlled-U gates
fix(compile): correct basis translation for IQM
docs(hal): add backend implementation guide
```

## Code Standards

### Rust Style

- Follow [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- Use `rustfmt` with project configuration
- Run `clippy` and address all warnings
- Prefer `thiserror` for error types
- Use `tracing` for logging

### Documentation

- All public items must have doc comments
- Include examples in doc comments where appropriate
- Use `///` for item docs, `//!` for module docs

```rust
/// A quantum gate operation.
///
/// # Examples
///
/// ```
/// use arvak_ir::gate::{Gate, StandardGate};
///
/// let h_gate = Gate::standard(StandardGate::H);
/// assert_eq!(h_gate.num_qubits(), 1);
/// ```
pub struct Gate {
    // ...
}
```

### Testing

- Unit tests in `#[cfg(test)]` modules
- Integration tests in `tests/` directory
- Property-based tests using `proptest` where appropriate
- Benchmarks in `benches/` using `criterion`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gate_creation() {
        let gate = Gate::standard(StandardGate::H);
        assert_eq!(gate.num_qubits(), 1);
    }

    #[test]
    fn test_circuit_depth() {
        let circuit = Circuit::bell().unwrap();
        assert_eq!(circuit.depth(), 2);
    }
}
```

### Error Handling

- Use `Result<T, E>` for fallible operations
- Define domain-specific error types with `thiserror`
- Provide context in error messages
- Avoid `unwrap()` in library code

```rust
#[derive(Debug, Error)]
pub enum CompileError {
    #[error("Gate '{0}' not in target basis")]
    GateNotInBasis(String),

    #[error("Routing failed: qubits {qubit1} and {qubit2} not connected")]
    RoutingFailed { qubit1: u32, qubit2: u32 },
}
```

## Adding New Features

### New Gate Type

1. Add to `StandardGate` enum in `hiq-ir/src/gate.rs`
2. Implement `num_qubits()` match arm
3. Add matrix representation if single-qubit
4. Add decomposition rules in `arvak-compile`
5. Add tests
6. Update documentation

### New Compilation Pass

1. Create new file in `hiq-compile/src/passes/`
2. Implement `Pass` trait
3. Add to `mod.rs` exports
4. Add to preset pass managers if appropriate
5. Write tests
6. Document in `docs/compilation.md`

### New Backend Adapter

1. Create new crate in `adapters/`
2. Implement `Backend` trait
3. Add configuration types
4. Write integration tests (may need mocking)
5. Document in `docs/hal-specification.md`
6. Add deployment guide if HPC-specific

## Testing

### Running Tests

```bash
# All tests
cargo test --all

# Specific crate
cargo test -p hiq-ir

# Specific test
cargo test -p hiq-ir test_circuit_depth

# With output
cargo test -- --nocapture
```

### Integration Tests

```bash
# Run integration tests (may require backend access)
cargo test --test integration -- --ignored
```

### Benchmarks

```bash
# Run all benchmarks
cargo bench

# Specific benchmark
cargo bench --bench transpile_bench
```

## Documentation

### Building Docs

```bash
# Build documentation
cargo doc --no-deps --open

# Include private items
cargo doc --no-deps --document-private-items
```

### Writing Docs

- Keep README.md up to date
- Update `docs/` for architectural changes
- Include code examples
- Document breaking changes in CHANGELOG.md

## Release Process

1. Update version in `Cargo.toml` files
2. Update CHANGELOG.md
3. Create release PR to `main`
4. After merge, tag release: `git tag v0.1.0`
5. Push tag: `git push origin v0.1.0`
6. CI builds and publishes release

## Getting Help

- **Questions:** Open a GitHub Discussion
- **Bugs:** Open a GitHub Issue with reproduction steps
- **Features:** Open a GitHub Issue with use case description

## Recognition

Contributors are recognized in:
- CONTRIBUTORS.md
- Release notes
- Project documentation

Thank you for contributing to HIQ!
