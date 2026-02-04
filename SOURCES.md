# Code Review Council — Reference Corpus

This document provides authoritative sources for Rust code review recommendations.
All findings in REVIEW_REPORT.md cite entries from this corpus.

---

## Canonical References

### RS-API — Rust API Guidelines
- **URL**: https://rust-lang.github.io/api-guidelines/
- **Description**: Official checklist for idiomatic Rust API design covering naming, interoperability, macros, documentation, and more.
- **Key Sections**:
  - [Naming](https://rust-lang.github.io/api-guidelines/naming.html) — C-CASE, C-CONV, C-ITER, C-GETTER
  - [Interoperability](https://rust-lang.github.io/api-guidelines/interoperability.html) — C-COMMON-TRAITS, C-CONV-TRAITS
  - [Documentation](https://rust-lang.github.io/api-guidelines/documentation.html) — C-CRATE-DOC, C-EXAMPLE
  - [Macros](https://rust-lang.github.io/api-guidelines/macros.html) — C-EVOCATIVE
  - [Future Proofing](https://rust-lang.github.io/api-guidelines/future-proofing.html) — C-STRUCT-PRIVATE, C-SEALED

### RS-REF — The Rust Reference
- **URL**: https://doc.rust-lang.org/reference/
- **Description**: Authoritative specification of Rust syntax and semantics.
- **Key Sections**:
  - [Unsafe Code](https://doc.rust-lang.org/reference/unsafety.html)
  - [Type Layout](https://doc.rust-lang.org/reference/type-layout.html)
  - [Visibility and Privacy](https://doc.rust-lang.org/reference/visibility-and-privacy.html)

### RS-BOOK — The Rust Programming Language
- **URL**: https://doc.rust-lang.org/book/
- **Description**: The official introductory book, containing idiomatic patterns.
- **Key Sections**:
  - [Error Handling](https://doc.rust-lang.org/book/ch09-00-error-handling.html)
  - [Generic Types, Traits, and Lifetimes](https://doc.rust-lang.org/book/ch10-00-generics.html)
  - [Fearless Concurrency](https://doc.rust-lang.org/book/ch16-00-concurrency.html)

### RS-CARGO — The Cargo Book
- **URL**: https://doc.rust-lang.org/cargo/
- **Description**: Official documentation for Cargo, Rust's package manager.
- **Key Sections**:
  - [Workspaces](https://doc.rust-lang.org/cargo/reference/workspaces.html)
  - [Features](https://doc.rust-lang.org/cargo/reference/features.html)
  - [Manifest Format](https://doc.rust-lang.org/cargo/reference/manifest.html)
  - [Publishing](https://doc.rust-lang.org/cargo/reference/publishing.html)

### RS-CLIPPY — Clippy Lints
- **URL**: https://rust-lang.github.io/rust-clippy/master/
- **Description**: Documentation for all Clippy lints with rationale and examples.
- **Categories**: correctness, suspicious, style, complexity, perf, pedantic, nursery

### RS-NOMICON — The Rustonomicon
- **URL**: https://doc.rust-lang.org/nomicon/
- **Description**: Guide to unsafe Rust and low-level programming.
- **Key Sections**:
  - [Data Representation](https://doc.rust-lang.org/nomicon/data.html)
  - [Ownership and Lifetimes](https://doc.rust-lang.org/nomicon/ownership.html)
  - [Concurrency](https://doc.rust-lang.org/nomicon/concurrency.html)

---

## Secondary References

### RS-PERF — The Rust Performance Book
- **URL**: https://nnethercote.github.io/perf-book/
- **Description**: Practical guide to optimizing Rust code performance.
- **Key Sections**:
  - [Benchmarking](https://nnethercote.github.io/perf-book/benchmarking.html)
  - [Build Configuration](https://nnethercote.github.io/perf-book/build-configuration.html)
  - [Heap Allocations](https://nnethercote.github.io/perf-book/heap-allocations.html)
  - [Type Sizes](https://nnethercote.github.io/perf-book/type-sizes.html)

### RS-ASYNC — Asynchronous Programming in Rust
- **URL**: https://rust-lang.github.io/async-book/
- **Description**: Official guide to async/await in Rust.
- **Key Sections**:
  - [Why Async?](https://rust-lang.github.io/async-book/01_getting_started/01_chapter.html)
  - [Pinning](https://rust-lang.github.io/async-book/04_pinning/01_chapter.html)
  - [Streams](https://rust-lang.github.io/async-book/05_streams/01_chapter.html)

### RS-EMBEDDED — The Embedded Rust Book
- **URL**: https://docs.rust-embedded.org/book/
- **Description**: Guide for embedded/no_std Rust development.
- **Relevance**: Patterns for resource-constrained environments.

### RS-EDITION — Rust Edition Guide
- **URL**: https://doc.rust-lang.org/edition-guide/
- **Description**: Guide to Rust editions and migration.
- **Key Sections**:
  - [Rust 2024](https://doc.rust-lang.org/edition-guide/rust-2024/index.html)

---

## Ecosystem Documentation

### SERDE — Serde Documentation
- **URL**: https://serde.rs/
- **Description**: Serialization framework best practices.
- **Key Sections**:
  - [Derive](https://serde.rs/derive.html)
  - [Attributes](https://serde.rs/attributes.html)
  - [Custom Serialization](https://serde.rs/custom-serialization.html)

### TOKIO — Tokio Documentation
- **URL**: https://tokio.rs/tokio/tutorial
- **Description**: Async runtime patterns and best practices.
- **Key Sections**:
  - [Spawning](https://tokio.rs/tokio/tutorial/spawning)
  - [Shared State](https://tokio.rs/tokio/tutorial/shared-state)
  - [Select](https://tokio.rs/tokio/tutorial/select)

### THISERROR — thiserror Documentation
- **URL**: https://docs.rs/thiserror/latest/thiserror/
- **Description**: Derive macros for error types.

### ANYHOW — anyhow Documentation
- **URL**: https://docs.rs/anyhow/latest/anyhow/
- **Description**: Flexible error handling for applications.

---

## Standards and RFCs

### SEMVER — Semantic Versioning
- **URL**: https://semver.org/
- **Description**: Versioning standard for software releases.
- **Relevance**: API stability commitments.

### RUST-RFC — Rust RFCs
- **URL**: https://rust-lang.github.io/rfcs/
- **Description**: Rust language and ecosystem design decisions.
- **Notable RFCs**:
  - RFC 1105: API Evolution
  - RFC 1946: Intra-doc Links
  - RFC 2008: Non-Exhaustive Enums

---

## Citation Format

When citing sources in findings, use the following format:

```
[SOURCE-ID § Section] — Brief description
```

**Examples**:
- `[RS-API § C-COMMON-TRAITS]` — Types should implement common traits where appropriate
- `[RS-CLIPPY § unnecessary_wraps]` — Functions should not wrap return values unnecessarily
- `[RS-PERF § Heap Allocations]` — Avoid unnecessary allocations in hot paths
- `[RS-CARGO § Features]` — Features should be additive

---

## Corpus Version

- **Generated**: 2026-02-04
- **Rust Toolchain**: 1.93.0
- **Edition**: 2024
