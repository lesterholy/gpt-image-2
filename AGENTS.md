# Repository Guidelines

## Project Structure & Module Organization

This repository contains a Rust-only Codex skill and CLI for GPT-Image-2 compatible image APIs.

- `SKILL.md` contains the skill instructions loaded by Codex.
- `README.md` provides usage, installation, and operational notes.
- `Cargo.toml` defines the Rust CLI package.
- `src/main.rs` parses CLI arguments and dispatches commands.
- `src/lib.rs` contains API URL handling, payloads, multipart upload, retries, response decoding, and tests.
- `references/` contains prompt gallery, prompt craft, and local API/model reference notes.
- `.env.example` lists local configuration keys; copy it to `.env` for local use only.

## Build, Test, and Development Commands

```bash
cargo build --release
```

Builds `target/release/gpt-image-2`.

```bash
cargo install --path .
```

Installs `gpt-image-2` into Cargo's bin directory, usually `~/.cargo/bin`.

```bash
cargo run -- generate --prompt "A cyberpunk orange cat" --out output/imagegen/cat.png
```

Calls the configured image API and writes the output file.

```bash
cargo test
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
```

Runs Rust tests, formatting checks, and lint checks.

## Coding Style & Naming Conventions

Use Rust 2021 edition. Keep helpers small, return `Result<T>` from fallible library functions, and keep CLI defaults aligned with `.env.example`. Follow `rustfmt` output and prefer explicit validation for user/env input.

Prefer small helper functions with focused tests. Keep documentation examples aligned with actual CLI flags and environment variable names.

## Testing Guidelines

Rust tests live next to the Rust modules. Add or update tests when changing payload construction, environment handling, retry behavior, multipart upload logic, or response decoding. Use local HTTP handlers for API behavior; do not require live credentials in tests.

Before submitting changes, run:

```bash
cargo test
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo build --release
```

## Skill Installation Notes

To install this skill into another Codex environment, copy the repository to `${CODEX_HOME:-$HOME/.codex}/skills/gpt-image-2-api`, run `cargo install --path .` from that directory, then restart Codex. Make sure `~/.cargo/bin` is on `PATH`.

## Commit & Pull Request Guidelines

Recent history uses short imperative or scoped messages, for example `Add executable gpt-image-2 scripts` and `docs: add bilingual README`. Keep commits focused and describe the user-facing reason for the change.

Pull requests should include a concise summary, test results, and any documentation updates. Link related issues when available. Include screenshots or generated sample paths only when image behavior changes.

## Security & Configuration Tips

Never commit real API keys, generated secrets, local `.env` files, or generated images unless they are intentional documentation assets. Use `GPT_IMAGE_2_BASE_URL` and `BASE_URL_API_KEY` from the environment.
