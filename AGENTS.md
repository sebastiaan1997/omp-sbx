# Repository Guidelines

## Project Overview

`omp-sbx` packages Oh My Pi (OMP) into Docker Sandboxes (`sbx`) microVMs. Root Bash commands build the sandbox image, create or recover per-project sandboxes, forward selected host state, and launch OMP. The repository is infrastructure-oriented: there is no application package manifest, conventional `src/` tree, or package-managed test suite.

## Architecture & Data Flow

1. Host launchers (`omp-sbx`, `omp-sbx-parallel`, `omp-sbxenv`) parse launcher-only flags and forward all remaining arguments to OMP.
2. `sbx-preflight.sh` locates/bootstraps `sbx`, normalizes sandbox names, and removes sandboxes created from a stale kit path.
3. Launchers create or attach to a sandbox defined by `sbx-kit/spec.yaml` and based on the image built from `sbx-kit/Dockerfile`.
4. Inside the sandbox, `sbx-kit/omp-init.sh` maps host mounts to stable guest paths, reads supported project `.env` keys, configures optional AWS/MCP/skills integration, then uses `exec omp ...`.
5. `sbx-kit/extensions/aws-sso-nudge.ts` is an event-driven OMP extension. It checks AWS credentials asynchronously, owns timer/login state in a closure, and cleans up its interval on session shutdown.

Keep the host/guest boundary explicit. Host scripts own filesystem paths, Git/worktrees, browser authentication, MCP registry operations, and sandbox lifecycle. `omp-init.sh` owns guest-local convergence before OMP starts. `spec.yaml` owns runtime policy, network access, environment, and startup mount wiring.

Changes to launch/recovery behavior usually require comparing both `omp-sbx` and `omp-sbx-parallel`; they intentionally differ in sandbox lifetime, mounts, naming, and cleanup. Preserve the shared `sandbox_name_for` contract or helpers will address different sandboxes.

## Key Directories

- `sbx-kit/`: image, sandbox policy, guest initialization, runtime verification, and OMP extensions.
- `sbx-kit/extensions/`: runtime TypeScript extensions loaded by `omp-init.sh`.
- Repository root: public host-side Bash commands and shared preflight logic.

There are no conventional source, test, CI, or examples directories.

## Development Commands

```bash
./build.sh                              # authoritative build, load, and smoke verification
OMP_VERSION=X.Y.Z ./build.sh            # build a selected OMP baseline
RELEASE_COOLDOWN_DAYS=0 OMP_VERSION=latest ./build.sh
./omp-sbx                               # create/resume an interactive sandbox
./omp-sbx --new                         # recreate before launch
./omp-sbx "fix the bug"                 # one-shot OMP invocation
./omp-sbx-parallel --new feature-name   # new branch/worktree session
./omp-sbx-parallel --branch feature-x   # existing branch/worktree session
./omp-sbx-mcp-import --dry-run          # preview MCP imports
./omp-sbx-aws-login --profile NAME      # host-side AWS SSO login
./omp-sbxenv --version                  # experimental sbx-env one-shot path
```

Inside a running sandbox, run the manual TLS/network diagnostic with:

```bash
bash sbx-kit/verify-tls-trust.sh
```

No repository-defined install, lint, format, typecheck, unit-test, or coverage command exists. Do not invent `npm`, `pnpm`, ShellCheck, or other project commands. Use `./build.sh` rather than a direct `docker build`; it also loads the template and exercises the built image.

## Code Conventions & Common Patterns

- Bash entry points use `#!/usr/bin/env bash` and `set -euo pipefail`.
- Resolve symlinked launcher paths before locating repository-relative files. Reuse `sbx-preflight.sh` instead of duplicating sbx discovery or sandbox naming.
- Store command arguments in quoted arrays (`MOUNTS`, `CREATE_OPTS`, `OMP_ARGS`); never assemble executable command strings.
- Shell functions use `snake_case`; environment/configuration constants use uppercase `OMP_SBX_*` names. TypeScript uses `camelCase` and uppercase constants.
- Send diagnostics through the existing `log` helpers to stderr. Enable colors only for a TTY and read interactive prompts from `/dev/tty`.
- Use `exec` at terminal process boundaries so signals and status reach `sbx run`, `sbx exec`, or `omp` directly.
- Keep strict failures for prerequisites and validation. Use `|| true` only for expected absence, idempotent cleanup, or a failure deliberately classified by the next operation.
- Preserve idempotence: exact sandbox-name matching, safe symlink replacement, inspect-before-add for MCP, stable sorted parser output, and temporary-file-plus-`mv` updates.
- `.env` is parsed as simple first-match `KEY=value` text, not sourced as shell. Do not add interpolation assumptions. Supported keys include `OMP_SBX_AWS_PROFILE`, `OMP_SBX_AWS_REGION`, and `OMP_SBX_MCP_GATEWAY`.
- Startup ordering matters: configuration OMP must see belongs in `omp-init.sh`; the `spec.yaml` startup hook runs concurrently.
- Interactive flows may pause or prompt. One-shot flows must remain non-blocking and still pass through `omp-init.sh`.
- The AWS extension must keep login single-flight, suppress duplicate warnings, stream device-code output before process exit, and clear lifecycle timers.
- If creating/removing worktrees outside `omp-sbx-parallel`, follow `INSTRUCTIONS.md`: update the gitignored `<repo>.code-workspace`, keep the main checkout first, and never commit that file.

## Important Files

- `omp-sbx`: canonical launcher, mounts, attach/recovery state machine, and one-shot path.
- `sbx-preflight.sh`: shared host prerequisites, sandbox naming, and stale-kit detection.
- `omp-sbx-parallel`: branch/worktree lifecycle and VS Code workspace updates.
- `omp-sbx-mcp-import`: Claude MCP JSON parsing and sbx MCP registration/auth/load flow.
- `omp-sbx-aws-login`: host AWS profile import, cache sharing, and browser login.
- `omp-sbxenv`: experimental scripted/CI path; requires sbx v0.39+.
- `build.sh`: authoritative image build, template load, and smoke checks.
- `sbx-kit/Dockerfile`: sandbox toolchain and version pins.
- `sbx-kit/spec.yaml`: network allow-list, environment, generated sandbox instructions, and startup wiring.
- `sbx-kit/omp-init.sh`: guest entry point and final OMP argument construction.
- `sbx-kit/extensions/aws-sso-nudge.ts`: AWS SSO session extension.
- `sbx-kit/verify-tls-trust.sh`: manual aggregate TLS/proxy/network-policy verifier.
- `README.md`: operational runbook; verify claims against implementation when changing behavior.

## Runtime/Tooling Preferences

- Host runtime: Bash plus authenticated Docker `sbx`; image builds also require Docker. `~/.omp` must already exist for normal launch.
- Helper-specific host tools: Git for parallel sessions, Python 3 for MCP import; `fzf` and `jq` are optional enhancements.
- There is no repository package manager or lockfile. pnpm, npm/Corepack, Bun, uv, Go tooling, and RubyGems are sandbox payloads, not repository development managers.
- The sandbox is Linux (`amd64` or `arm64`), user `agent`, home `/home/agent`, workspace `/home/agent/workspace`. Its Docker socket is microVM-local; never mount the host Docker socket.
- Treat `~/.omp` as durable user data. It can contain sessions, skills, memories, MCP state, and AWS SSO cache.
- Network access is allow-listed in `sbx-kit/spec.yaml`. Adding an external service or AWS region requires corresponding policy entries.
- Keep downloaded tools/version checks architecture-aware and pinned where the existing implementation pins them. Playwright projects must match the image's exact `1.63.0` release and keep certificate verification enabled.
- Known drift: normal `./build.sh` currently supplies OMP `18.1.10`, while the Dockerfile default is `18.1.21`. `install-native-lsps.sh` is not invoked by the observed build path despite README claims. Do not describe either as resolved without changing and verifying the implementation.

## Testing & QA

There is no test framework, paired test-file convention, CI workflow, coverage configuration, or coverage threshold. QA is system-level:

- `./build.sh` creates disposable sandbox `omp-verify`, waits for its private Docker daemon, runs `omp-init.sh --version`, and checks headless Chromium output. This is expensive but is the repository's authoritative verification path.
- `sbx-kit/verify-tls-trust.sh` runs numbered certificate, proxy, allowed-host, and blocked-host checks inside a sandbox. It reports all checks through `_pass`/`_fail` and exits nonzero if any fail.

For image, startup, Docker, or browser changes, run `./build.sh`. For TLS, proxy, CA, or network-policy changes, additionally run the TLS verifier inside the built sandbox and keep its constants aligned with `sbx-kit/spec.yaml`. Prefer observable contracts—socket/executable presence, command status, certificate verification, policy outcome, and rendered browser output—over source-text assertions.
