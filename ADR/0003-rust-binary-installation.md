# ADR-0003: Install the Rust CLI as the Only Host Launcher

- Status: Accepted
- Date: 2026-09-21

## Context

Installing a Rust binary beside legacy shell launchers leaves two competing command implementations. The shell scripts previously delegated image builds and model configuration to additional scripts, which made installation incomplete and allowed behavior to drift.

## Decision

`install-rust.sh` builds the workspace `omp-sbx` binary and installs it atomically as `~/.local/bin/omp-sbx` by default. It supports `--debug`, `--dest`, and `--name` for controlled development installations.

The installer does not install or link host backing scripts. Image build, configure, launcher, MCP, environment, parallel, and private-state behavior lives in the Rust binary. Repository guest scripts are embedded/materialized as image assets where required.

## Consequences

- `omp-sbx` on `PATH` resolves to the Rust implementation.
- Installation does not depend on repository-relative shell script paths.
- A temporary executable and atomic rename prevent a partially copied binary from being exposed.
- `cargo` and the pinned Rust toolchain are installation prerequisites.
- Runtime operations still require the external Docker and `sbx` tools.
