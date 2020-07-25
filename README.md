# async-smtp-lite

* [Cargo package](https://crates.io/crates/async-smtp-lite)

## Examples

### smol 

* [aws_workmail](demos/smol/src/aws_workmail.rs)
* [gmail](demos/smol/src/gmail.rs)

## Dev

```
cargo test --all-features --all -- --nocapture && \
cargo clippy --all -- -D clippy::all && \
cargo fmt --all -- --check
```

```
cargo build-all-features
cargo test-all-features --all
```
