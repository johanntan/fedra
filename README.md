# Fedra

Fedra is a lightweight, fast, and accessible Mastodon client for Windows. It is designed to be completely usable with screen readers and keyboard navigation, providing a seamless social media experience without the bloat.

## Documentation

For a comprehensive user guide, including a full list of features and hotkeys, please see the [User Manual](doc/readme.md).

## Building

To build, you'll need cargo, as well as CMake and Ninja for building wxDragon. In addition, you also need LLVM, from LLVM.org.

```batch
cargo build --release
```

This will generate the executable at `target/release/fedra.exe`.

### Toolchains

- Stable Rust `1.88.0` is the minimum supported version. CI builds, tests, and lints on the latest stable.
- Nightly Rust is only required for formatting with `cargo +nightly fmt`.
- Clippy runs with `clippy::all`, `clippy::pedantic`, and `clippy::nursery` enabled, and CI treats every one as an error.

### Installer

`cargo release` builds the Windows installer with Inno Setup. If Inno Setup isn't installed, the build downloads it to a per-user cache.

## Before Committing

Run the formatter before you commit changes:

```batch
cargo +nightly fmt
```

Set up the repository pre-commit hooks from the repo root:

```batch
cargo install prek
prek install
```

Run the same checks that CI expects:

```batch
cargo +nightly fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## License

This project is licensed under the [MIT License](LICENSE).
