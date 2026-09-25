# ADR-0004: Use Opt-In Integration Tests for sbx Workflows

- Status: Accepted
- Date: 2026-09-21

## Context

The Rust CLI owns workflows that require Docker Sandboxes: image loading, disposable configure sandboxes, model discovery, staged configuration, and guest startup. These workflows cannot be proven by unit tests alone. However, `sbx` is an external, authenticated, host-specific runtime and is not available in every development or CI environment.

A test suite that requires a live `sbx` daemon by default would make ordinary Rust test runs unreliable and would encourage weakening assertions or skipping failures after the fact.

## Decision

Keep sbx integration tests in `crates/omp-sbx/tests/sbx_integration.rs` and make them opt-in:

- Tests return successfully without running runtime operations when `sbx` or Docker is unavailable.
- Tests also require `OMP_SBX_RUN_INTEGRATION=1` to opt into potentially expensive image and microVM operations.
- With the opt-in enabled, tests invoke the compiled Rust binary and assert command success and observable host-state behavior.
- Temporary HOME/config directories isolate integration runs from the operator's real `~/.omp` state.
- Dry-run tests assert that host configuration remains byte-for-byte unchanged.
- A deterministic fake `sbx` runtime is permitted for local orchestration smoke tests, but it does not replace real authenticated `sbx` verification.

Run the real integration tests with:

```bash
OMP_SBX_RUN_INTEGRATION=1 cargo test -p omp-sbx --test sbx_integration -- --nocapture
```

## Consequences

- `cargo test -p omp-sbx` remains safe and deterministic on hosts without `sbx`.
- Real runtime parity is explicit and visible in CI or a developer environment with Docker, authenticated `sbx`, and the required sandbox capabilities.
- Skipped runtime tests must not be reported as real sandbox verification.
- Integration tests validate consumer-visible behavior rather than source text or implementation details.
