After modifying Rust code:

1. Run cargo fmt --check
2. Run cargo check
3. Run cargo clippy --all-targets --all-features -- -D warnings
4. Run relevant tests
5. Fix all errors before considering the task complete.

Prefer idiomatic Rust.
Avoid unnecessary clones.
Avoid unwrap/expect in production code unless explicitly justified.
Do not introduce dependencies when the standard library or an existing dependency is sufficient.
