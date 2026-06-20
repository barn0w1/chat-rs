# chat-rs

Simple, reliable, self-hosted chat in Rust.

This project is in its initial development stage.

## Workspace

- `crates/chat`: application logic and the interfaces it requires
- `crates/chat-server`: executable server and runtime integrations

## Development

```sh
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```