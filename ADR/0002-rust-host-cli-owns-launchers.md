# ADR-0002: Use One Rust Host CLI for Sandbox Operations

- Status: Accepted
- Date: 2026-09-21

## Context

The repository historically exposed separate Bash launchers for persistent sessions, parallel worktrees, declarative environments, configuration, image builds, and MCP import. Duplicated lifecycle logic made safety changes difficult to apply consistently.

## Decision

`omp-sbx` is the canonical host CLI. Its Rust subcommands own:

- `run`, `parallel`, and `env` sandbox lifecycle.
- `configure`, including staged model discovery and configuration publication.
- `build` and `build-configure` image build/load/smoke orchestration.
- `mcp-import` host integration.

The CLI also accepts legacy direct launcher flags such as `omp-sbx --new` and `omp-sbx --configure`, translating them to the corresponding Rust subcommands.

Host shell entrypoints are removed rather than maintained as parallel implementations. Guest scripts remain image payload components when they perform guest-local initialization or policy setup.

## Consequences

- Lifecycle and per-sandbox state policy has one implementation language and command surface.
- Rust owns typed error handling, locking, staging, migration, and exit-status propagation.
- The Rust CLI preserves supported behavior previously provided by the removed launchers.
- Docker/sbx remain external runtime dependencies; Rust invokes them through direct argument arrays, never shell command strings.
- Guest-local shell scripts remain part of the image boundary and are not host launcher alternatives.
