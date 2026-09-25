# Repository Guidelines

## Project Overview

`omp-sbx` packages Oh My Pi (OMP) into Docker Sandboxes (`sbx`) microVMs. A native Rust CLI owns host launch, worktree, configuration, build, MCP, and persistent-state workflows; shell scripts in `sbx-kit/` run only inside the guest image. The repository is infrastructure-oriented: there is no application package manifest or conventional `src/` tree.

## Architecture & Data Flow

1. The Rust CLI (`crates/omp-sbx`) parses launcher-only flags and forwards remaining arguments to OMP.
2. `state.rs` assigns each exact sandbox name a private persistent `.omp` tree and safely migrates eligible legacy file-backed state.
3. Launch commands materialize the embedded kits, prepare Docker image policy, recreate incompatible old mounts, and create or attach to the named sandbox.
4. Inside the sandbox, `sbx-kit/omp-init.sh` maps host mounts to stable guest paths, configures optional MCP and shared skills, then uses `exec omp ...`.
5. Schema-v2 credentials in each `spec.yaml` keep API keys and OpenAI Codex OAuth tokens host-side behind the sbx proxy.

Keep the host/guest boundary explicit. Rust owns host filesystem paths, Git/worktrees, MCP registry operations, persistent OMP state, and sandbox lifecycle. `omp-init.sh` owns guest-local convergence before OMP starts. `spec.yaml` owns runtime policy, network access, credential injection, environment, and startup mount wiring.

Changes to launch/recovery behavior require comparing `commands/run.rs`, `commands/parallel.rs`, and `commands/env.rs`; they intentionally differ in sandbox lifetime, mounts, naming, and cleanup. Preserve the shared `sandbox_name_for` contract because it is also the persistent-state identity.

## Key Directories

- `crates/omp-sbx/`: native host CLI, lifecycle policy, state migration, and tests.
- `sbx-kit/`: interactive image, sandbox policy, guest initialization, runtime verification, and OMP plugins.
- `sbx-configure-kit/`: minimal image and policy used by model configuration.

There is no package-managed application test framework, CI workflow, or coverage threshold.

## Development Commands

```bash
cargo test -p omp-sbx
cargo check -p omp-sbx
cargo run -p omp-sbx -- run
cargo run -p omp-sbx -- run --new
cargo run -p omp-sbx -- parallel --new feature-name
cargo run -p omp-sbx -- parallel --branch feature-x
cargo run -p omp-sbx -- mcp-import --dry-run
cargo run -p omp-sbx -- env -- --version
cargo run -p omp-sbx -- build
```

Inside a running sandbox, run the manual TLS/network diagnostic with:

```bash
bash sbx-kit/verify-tls-trust.sh
```

Do not invent npm, pnpm, ShellCheck, or other project commands. `omp-sbx build` is the authoritative image build, template load, and smoke flow; do not substitute a direct `docker build`.

## Code Conventions & Common Patterns

- Rust functions and modules use `snake_case`; types use `UpperCamelCase`; environment/configuration constants use uppercase `OMP_SBX_*` names.
- Pass external command arguments as typed arrays through `CommandSpec`; never assemble executable shell command strings.
- Diagnostics go to stderr. Interactive prompts must be gated on a TTY; one-shot paths must remain non-blocking.
- Keep strict failures for prerequisites and validation. Ignore status only for expected absence or idempotent cleanup.
- Preserve idempotence: exact sandbox-name matching, safe symlink replacement, inspect-before-add for MCP, stable sorted parser output, and temporary-file-plus-rename publication.
- Legacy `.env` parsing is simple first-match `KEY=value` text, not sourced shell. `OMP_SBX_MCP_GATEWAY` is the supported guest startup key.
- Startup ordering matters: configuration OMP must see belongs in `omp-init.sh`; the `spec.yaml` startup hook runs concurrently.
- Per-sandbox OMP state is keyed by exact `sandbox_name_for` output. Never remount legacy `~/.omp` into a sandbox or copy SQLite databases, sidecars, repair artifacts, symlinks, special files, or retired AWS state into a private tree.
- Configuration seeding is digest-based: unchanged legacy `agent/config.yml` must preserve private edits; a changed seed applies once.
- All sbx command paths require 0.43.0 or newer. Declarative environment operations must use the same `--name`, `sbxenv.yaml` path, and complete `--env-arg` set.
- If creating/removing worktrees outside `commands/parallel.rs`, follow `INSTRUCTIONS.md`: update the gitignored `<repo>.code-workspace`, keep the main checkout first, and never commit that file.

## Important Files

- `crates/omp-sbx/src/commands/run.rs`: canonical launcher and attach/recovery state machine.
- `crates/omp-sbx/src/commands/parallel.rs`: branch/worktree lifecycle and disposable VM cleanup.
- `crates/omp-sbx/src/commands/env.rs`: declarative `sbx env` path.
- `crates/omp-sbx/src/state.rs`: private state location, migration exclusions, locking, and seed convergence.
- `crates/omp-sbx/src/preflight.rs`: sbx discovery, naming, stale-kit, and incompatible-mount checks.
- `crates/omp-sbx/src/commands/mcp_import.rs`: Claude MCP parsing and sbx MCP registration/auth/load flow.
- `crates/omp-sbx/src/commands/build.rs`: authoritative image build, load, and smoke checks.
- `sbx-kit/Dockerfile`: interactive sandbox toolchain and version pins.
- `sbx-kit/spec.yaml`: network allow-list, schema-v2 credentials, environment, generated instructions, and startup wiring.
- `sbx-kit/omp-init.sh`: guest entry point and final OMP argument construction.
- `sbx-kit/sbxenv.yaml`: declarative environment definition.
- `sbx-kit/verify-tls-trust.sh`: manual aggregate TLS/proxy/network-policy verifier.
- `README.md`: operational runbook; verify claims against implementation when changing behavior.

## Runtime/Tooling Preferences

- Host runtime: the Rust binary plus authenticated Docker `sbx` 0.43.0 or newer; image builds also require Docker.
- Helper-specific host tools: Git for parallel sessions and Python 3 for MCP import.
- There is no repository package manager or lockfile for guest payload tooling. pnpm, npm/Corepack, Bun, uv, Go tooling, and RubyGems are sandbox payloads, not repository development managers.
- The sandbox is Linux (`amd64` or `arm64`), user `agent`, home `/home/agent`, workspace `/home/agent/workspace`. Its Docker socket is microVM-local; never mount the host Docker socket.
- Durable state lives under `${XDG_STATE_HOME:-$HOME/.local/state}/omp-sbx/sandboxes/<name>/.omp`; legacy `~/.omp` is only a migration source and global configuration seed.
- Network access is allow-listed in `sbx-kit/spec.yaml`. OpenAI OAuth needs `auth.openai.com` and `chatgpt.com`; generic Bedrock provider reachability retains only the Bedrock API hosts.
- Keep downloads architecture-aware and pinned where the implementation pins them. Playwright projects must match the image's exact `1.63.0` release and keep certificate verification enabled.
- Known drift: the Rust build command currently supplies OMP `18.1.10`, while the interactive Dockerfile default is `18.1.21`. Do not describe this as resolved without changing and verifying both.

## Testing & QA

The Rust unit suite is deterministic and does not require sbx. Runtime verification is system-level:

- `cargo test -p omp-sbx` covers parsing, state migration/exclusions, seed convergence, and host logic.
- `omp-sbx build` creates disposable smoke sandboxes, runs the guest entrypoint, verifies `omp config path`, and checks the configure image boundary. This is expensive and requires Docker plus authenticated sbx.
- `sbx-kit/verify-tls-trust.sh` runs numbered certificate, proxy, allowed-host, and blocked-host checks inside a sandbox.

For image, startup, Docker, browser, credential, or network changes, run `omp-sbx build`. For TLS, proxy, CA, or network-policy changes, additionally run the TLS verifier inside the built sandbox and keep its constants aligned with `sbx-kit/spec.yaml`. Prefer observable contracts—mount paths, socket/executable presence, command status, certificate verification, policy outcome, and rendered browser output—over source-text assertions.
