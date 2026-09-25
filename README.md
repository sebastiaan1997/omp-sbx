<p align="center">
  <img width="250" alt="Image" src="https://github.com/user-attachments/assets/c91d67e4-c6a6-46c0-a7dd-4f682cc67193" />
</p>

# omp-sbx Oh My Pi Sandbox

Run the [omp coding agent](https://omp.sh) (oh-my-pi) inside a [Docker sbx](https://docs.docker.com/ai/sandboxes/) sandbox with persistent per-sandbox state.

## What it does

- Launches omp inside a Docker sbx microVM — sbx handles all security (non-root user, network policies, secret proxy, resource limits)
- Provides a private Docker Engine inside the sbx microVM, so `/var/run/docker.sock`, `docker ps`, `docker build`, and Docker Compose target sandbox-local containers/images instead of the host daemon
- Mounts a distinct host-backed `~/.omp` tree for every sandbox, so SQLite databases have only one microVM writer while settings, logins, memories, and sessions survive recreation
- Sandboxes are per-directory: running from the same cwd reconnects to the same sandbox and state tree
- Recovers automatically after a force-quit: if the sandbox is left stopped (or its agent wedged), the launcher re-attaches, then stops+restarts, and as a last resort recreates the sandbox without deleting its OMP state
- `--new` forces a fresh sandbox while retaining that sandbox's OMP state

## Prerequisites

```bash
brew install docker/tap/sbx
sbx login
sbx policy set-default balanced
```

Docker `sbx` 0.43.0 or newer is required.

## Install

```bash
git clone https://github.com/mikeatlas/omp-sbx.git ~/src/github.com/mikeatlas/omp-sbx
cd ~/src/github.com/mikeatlas/omp-sbx

# Build and install the fully native Rust launcher as `omp-sbx`.
./install-rust.sh

# Build and load both templates with the Rust launcher.
omp-sbx build

# Alias omp to always use the sandbox (in ~/.zshrc or ~/.bashrc)
echo "alias omp='omp-sbx'" >> ~/.zshrc
```

## Usage

```bash
omp                    # interactive TUI (cwd = workspace, private state persists)
omp --new              # recreate the VM, retaining its OMP state
omp --yes              # skip the pre-launch "press any key" pause
omp --version          # passthrough flags to omp
omp "fix the bug"      # one-shot prompt
omp --configure          # choose global fast / standard / deep model defaults
omp --configure --dry-run
omp --configure --fast openai-codex/gpt-5.6-luna \
  --standard openai-codex/gpt-5.6-sol \
  --deep openai-codex/gpt-6-astra
```

## How it works


| Component | File | Purpose |
|---|---|---|
| Interactive template | `sbx-kit/Dockerfile` | Extends `shell-docker-nightly` with the nested Docker Engine, OMP binary, browser, and development tools |
| Interactive kit | `sbx-kit/spec.yaml` | Defines the OMP entrypoint, network allow-list, environment, and agent context |
| Configure template | `sbx-configure-kit/Dockerfile` | Extends the lightweight non-Docker `shell` template with only the OMP binary |
| Configure kit | `sbx-configure-kit/spec.yaml` | Defines the parked entrypoint and provider-only network policy used by `omp --configure` |
| Native Rust CLI | `crates/omp-sbx` | Owns launcher, build, configure, environment, parallel, MCP, and per-sandbox state lifecycle |
| Per-sandbox state | `crates/omp-sbx/src/state.rs` | Migrates safe legacy files and converges the global configuration seed into isolated persistent state |
| Native host binary | `install-rust.sh` | Builds and installs `omp-sbx` to `~/.local/bin` |
| MCP gateway | `sbx-kit/omp-init.sh` | Opt-in wiring that connects omp to the sandbox's MCP gateway |
| Browser | `sbx-kit/Dockerfile` | Installs architecture-native Chromium for OMP's built-in browser API |
### Per-sandbox OMP state

The launcher assigns each sandbox this persistent host directory:

```text
${XDG_STATE_HOME:-$HOME/.local/state}/omp-sbx/sandboxes/<sandbox-name>/.omp
```

Docker sbx mounts that directory at its host absolute path inside the microVM.
`omp-init.sh` symlinks it to `/home/agent/.omp`, so OMP's default agent
directory remains `/home/agent/.omp/agent`. Removing or recreating a VM does not
remove the host state directory.

The first launch copies regular file-backed data from legacy `~/.omp`, including
YAML settings, session JSONL, blobs, memories, plugins, and native assets. It
does not copy SQLite files (by extension or header), WAL/SHM/journal sidecars,
database quarantine/repair artifacts, symlinks, special files, `aws-config`, or
`aws-sso-cache`. The legacy tree remains untouched. This intentionally avoids
copying `agent.db`, `history.db`, `stats.db`, GitHub cache databases, and local
Mnemopi databases into every sandbox.

`omp-sbx configure` publishes the global seed at `~/.omp/agent/config.yml`.
Each private state tree applies that seed once per content digest. Repeated
launches preserve settings changed inside that sandbox; changing the seed
converges the private `config.yml` on its next launch. Named OMP profiles remain
inside the same private `.omp` root. User-supplied `PI_CODING_AGENT_DIR`,
`PI_CODING_AGENT_SESSION_DIR`, or XDG overrides are outside this persistence
guarantee.

### Model provider credentials

Docker Sandboxes keeps provider credentials on the host. API-key providers
receive only a proxy-managed sentinel variable; the HTTPS proxy supplies the
stored key only for the matching API host. OpenAI Codex OAuth uses the same
model: the sandbox sees `oai-oat01-proxy-managed`, while the real access and
refresh tokens remain host-side.

| Docker service | Mechanism | OMP environment variable | Proxy hosts |
|---|---|---|---|
| `anthropic` | API key | `ANTHROPIC_API_KEY` | `api.anthropic.com` |
| `openai` | OAuth | `OPENAI_CODEX_OAUTH_TOKEN` | `auth.openai.com`, `chatgpt.com` |
| `google` | API key | `GEMINI_API_KEY` | `generativelanguage.googleapis.com` |
| `groq` | API key | `GROQ_API_KEY` | `api.groq.com` |
| `mistral` | API key | `MISTRAL_API_KEY` | `api.mistral.ai` |
| `openrouter` | API key | `OPENROUTER_API_KEY` | `openrouter.ai` |
| `xai` | API key | `XAI_API_KEY` | `api.x.ai` |

On the host, run `sbx secret set <service>` for API keys. For Codex OAuth, run:

```bash
sbx secret set openai --oauth
```

Confirm configured services with `sbx secret ls`. The first interactive launch
asks to approve each schema-v2 credential binding for its provider and domains;
a non-interactive launch without that approval starts with the credential
withheld. Interactive `run` and `parallel` creation and recovery preserve the
terminal so sbx can display and receive that approval. `--yes` skips only the
launcher's pause, not sbx credential approval. Service-secret changes apply to
existing local sandboxes in current sbx releases; changing the kit requires
recreating the sandbox.

Both kits declare OpenAI OAuth only. Select `openai-codex/<model>` in OMP.
Inside OMP, a runtime `--api-key`, configured model key, or
stored private `/login` credential can take precedence over the injected
environment sentinel. Codex web search separately requires OMP-stored OAuth in
the sandbox's private `agent.db`; it is not an OAuth-injection verification.

After updating the host launcher, rebuild and install it before recreating the
sandbox. An image build alone does not update the launcher:

```bash
bash install-rust.sh
omp-sbx run --new
```

Accept the OpenAI OAuth/domain approval in the host terminal. To check proxy
authentication independently of OMP, run inside the sandbox:

```bash
curl -sS --max-time 30 -o /dev/null -w 'HTTP %{http_code}\n' \
  -H 'Authorization: Bearer oai-oat01-proxy-managed' \
  https://chatgpt.com/backend-api/wham/usage
```

A `200` verifies that this request authenticates; a `401` requires investigating
the sandbox's credential binding/injection, not deleting OMP session state.

### Model defaults

`omp --configure` creates a fresh disposable sandbox from the lightweight
configure image, which has OMP but no nested Docker daemon, browser, language
servers, or development toolchain. It discovers the model catalog visible with
the host's approved sbx provider credentials and a private staged OMP agent
directory, then opens a Ratatui selector for each missing fast, standard, or deep tier.
Use the arrow keys or `j`/`k` to move, `Enter` to select, `d` to choose the
suggested default, and `q` or `Escape` to cancel.
The configure image uses a small argument-tolerant entrypoint that ignores
sandbox-injected agent or MCP arguments and parks instead of launching an
interactive OMP session. The helper uses noninteractive `sbx exec` commands,
removes the sandbox on every exit path, and then returns control to the host.
The native Rust command builds and loads the configure image when needed. Custom
`OMP_SBX_CONFIGURE_TEMPLATE` values are never built implicitly.

Thinking choices start with **Inherit** (no role-specific override), then **Off**
(a native disable request), followed by that model's advertised concrete efforts:
`minimal`, `low`, `medium`, `high`, `xhigh`, or `max`. Unsupported efforts are not
offered. Models without configurable efforts still offer Inherit and Off;
Off does not guarantee that a provider disables mandatory reasoning.
The initial thinking selection is always Inherit.

By default, the selected tiers update these global mappings:

| Tier | Roles | Bundled agents |
|---|---|---|
| Fast | `smol` | `scout`, `sonic` |
| Standard | `default`, `plan` | `task` via `@task` → `@default`, unless a task role is already saved |
| Deep | `slow`, `advisor` | `reviewer`, `security-reviewer` |

Interactive configuration then asks `Do you want to configure the optional models?`.
Answer Yes to select overrides for `plan`, `vision`, `designer`, `commit`, `tiny`,
`task`, and `advisor`, in that order. These menus use the same navigation and
cancellation keys; press `s` to skip a role. Vision requires an image-capable
model and is optional when the main model handles images.
Skipping `plan` or `advisor` retains the standard or deep tier assignment,
including its thinking level. Skipping `task` preserves its exact saved role
(including any thinking suffix or alias), or assigns `@default` if none exists.
Skipping any other optional role preserves its saved mapping, or leaves it
unassigned. Skipped roles do not open a thinking menu. Explicitly selecting a
role replaces its whole model/thinking assignment; choosing Inherit clears that
assignment's previous thinking suffix. Menu cursor defaults are suggestions,
not assignments.

The bundled task agent always routes through `@task`, so a project's task role
can override the global fallback. Selecting `designer` routes its agent through
`@designer`, replacing any saved override for that agent; skipping it preserves
the saved agent choice. Intentional agent-specific model overrides can take
precedence over roles. Projects can override those choices with native
`task.agentModelOverrides` settings.

Non-interactive invocations skip the optional question. The resolved and success
summaries show intended role and agent assignments, global scope, and the
`modelRoleStorage: project` preference. Unrelated saved mappings remain unchanged.
Dry-run reads the current global configuration to preserve saved choices, but
does not write configuration.

Use `omp --configure --dry-run` to discover and validate selections without
writing configuration. For non-interactive use, supply all three selectors:

```bash
omp --configure \
  --fast openai-codex/gpt-5.6-luna \
  --standard openai-codex/gpt-5.6-sol \
  --deep openai-codex/gpt-6-astra
```

Append `:LEVEL` to any CLI selector to set its role-specific thinking, for example
`--deep provider/model:high`, using a model from the discovered catalog that
supports `high`. `:inherit` normalizes to the bare selector; `:off` is distinct.
CLI-supplied tiers never open model or thinking menus, even on a terminal.
Unknown or unsupported levels fail before configuration writes. Exact catalog
identities take precedence over suffix parsing, including IDs containing colons;
the interactive menu rejects a thinking assignment that would collide with
another exact model ID.

Thinking is stored in native `modelRoles` selector strings, not a separate map.
A bare selector uses native inheritance. Project role overrides replace the
complete global selector, including its thinking suffix. The helper requires
catalog `reasoning` and `thinking` metadata and fails on incompatible output
rather than guessing capabilities from model names.

The helper preserves unrelated mappings in `modelRoles` and
`task.agentModelOverrides`, then publishes them atomically to the host seed at
`~/.omp/agent/config.yml`. Model discovery and serialization both use the same
private mounted staging directory; the configure sandbox never mounts legacy
`~/.omp`. A failed update or readback leaves the host seed untouched.

“Global” means the seed applied across this user's `omp-sbx` state trees, not
all OS users. On each sandbox's next launch, the state preparer compares the
seed digest with that sandbox's marker. A changed seed replaces the private
`agent/config.yml` once; an unchanged seed preserves settings changed with
`/settings` or `omp config set` inside that sandbox.

A successful configuration also sets global `modelRoleStorage: project`, enabling
global/project save choices in `/model` → Roles. To override a role for one
project, open Roles in that project's session and save the assignment to Project.
Native `.omp/config.yml` role keys override matching global roles; other roles
continue to inherit global defaults. The helper never changes project files.
The storage preference controls hub save choices, not the helper's write scope.

An OMP process reads these settings at startup. Exit and relaunch it after
configuration. `omp --new` recreates the VM while retaining that sandbox's
private sessions and other state.

The helper does not need a separate PATH symlink. Catalog visibility confirms
that OMP can discover a selector, but actual provider requests remain subject
to the network policy in `sbx-kit/spec.yaml`. The native Roles hub remains the
advanced surface for adaptive thinking (`auto`), aliases, ordered selectors, and
clearing assignments. The helper does not change `defaultThinkingLevel`.
Ordered selectors select an available model; they are not request retry fallback
chains.

### Docker inside the sandbox

The template extends Docker's `shell-docker-nightly` sbx base. That starts a private Docker Engine inside the sbx microVM and exposes the normal socket at `/var/run/docker.sock`. The socket is **not** the host Docker socket: containers, images, volumes, and `docker ps` output belong to the sandbox and are removed with the sandbox.

Projects may propose an external-image allowlist in
`.omp-sbx-docker-images.yaml`:

```yaml
schemaVersion: 1
allowedImages:
  - docker.io/library/alpine:3.22
  - ghcr.io/example/tool@sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef
```

The file is optional. Without it, the sandbox denies every external image pull.
On first launch, the host launcher copies the project file—or a generated empty
allowlist—into host state outside the writable project. Only that snapshot
directory is mounted read-only and consumed by `omp-sbx-image-policy`; the
plugin never reads the project copy. Short Docker Hub names are normalized
(`alpine` becomes `docker.io/library/alpine:latest`), and malformed policies
stop startup. Tags are mutable; use a digest when image content must be fixed.

The policy applies to Docker pulls, container creation, tags, and supported
builds. Locally built images remain usable when tagged with an unqualified name
such as `my-app:dev` or under `local/`. `docker build` and
`docker buildx build|bake` automatically receive the generated Buildx policy.
Compose pulls and `up` without `--build` work normally; use
`docker buildx bake -f compose.yaml` instead of `docker compose build` or
`docker compose up --build`.

Project edits do not change an approved snapshot, including after a normal
reattach, restart, or `--new`. Apply a host-reviewed change explicitly:
```bash
omp-sbx --refresh-image-policy
omp-sbx parallel --branch feature-x --refresh-image-policy
omp-sbx env --refresh-image-policy
```

The refresh recreates the affected sandbox/environment so the new snapshot is
compiled and activated. Deleting the project YAML and refreshing switches the
policy to deny-all. Sandboxes created before snapshot mounting was added also
default to deny-all until refreshed once.

This is a guardrail, not a hard security boundary. Passwordless `sudo` can
remove the plugin or reconfigure the daemon. Docker AuthZ also does not cover
native/upgraded gRPC, and non-JSON build contexts are enforced by the CLI
wrapper plus Buildx policy rather than inspected by AuthZ.

`omp-sbx build` verifies this contract by creating throwaway sandboxes and running
the Docker socket, Docker CLI, and browser smoke checks.

### GitHub auth forwarding

The sandbox forwards your host `gh` CLI session so that `gh` commands and `git push` over HTTPS work without a separate token or SSH setup. This is a **convenience**, not a security boundary — sbx's microVM isolation is the real security control (see [Security](#security)).

**What gets mounted.** If `~/.config/gh` exists on your host, `omp-sbx` bind-mounts it into the sandbox read-write. The kit's startup command symlinks it to `/home/agent/.config/gh` (via `GH_CONFIG_DIR`), then runs `gh auth setup-git` when `hosts.yml` is present — this configures `git`'s credential helper to call `gh auth git-credential`, which supplies the OAuth token for HTTPS remotes.

**Why the token is "insecure."** `~/.config/gh/hosts.yml` contains an OAuth token that can authenticate to GitHub as your user. The mount is read-write, so anything running inside the sandbox can read it. This is acceptable because the sandbox is a short-lived, isolated microVM with a network allow-list — it is not a multi-tenant or untrusted environment. If you need a hard boundary, do **not** mount `~/.config/gh`: remove the `MOUNTS+=("$HOME/.config/gh")` block in the `omp-sbx` launcher (around line 68) and use an HTTPS remote with a separate credential helper or SSH instead.

**Prerequisites.**

1. On the host: `gh auth login` (creates `~/.config/gh/hosts.yml`).
2. Store the GitHub secret so the sbx proxy can substitute a real token: `sbx secret set github --command 'gh auth token'`. Without it, `gh` and `git push` fail with `401 Unauthorized` (the proxy injects only a placeholder `GH_TOKEN`).
3. `github.com:443` must be in the network allow-list (it is by default — see `sbx-kit/spec.yaml`).
**Verifying.** Inside the sandbox:

```bash
gh auth status      # should show your logged-in account
gh status           # dashboard of assigned issues/PRs/mentions
git push            # uses gh credential helper, no separate token needed
```

If `gh auth status` fails with `401`, ensure the GitHub secret is stored (`sbx secret ls`). If it fails because the mount is missing, recreate the sandbox with `omp --new` — mounting is decided once, at launch. On macOS, `hosts.yml` contains no token (it lives in the keychain), so `sbx secret set github` is required.

### MCP servers from Claude Code

omp-sbx mcp-import --dry-run   # read ~/.claude.json, print the plan
omp-sbx mcp-import --auth      # register, then authorize the remote ones
omp-sbx mcp-import --load      # attach them to this directory's sandbox

sbx keeps its own MCP registry and serves it to a sandbox through a gateway.
`omp-sbx mcp-import` copies the servers out of `~/.claude.json` (plus a project
`.mcp.json`, or any `--file`) into that registry. It is idempotent: a second run
reports what already exists and changes nothing.

**Why not just mount the Claude config.** Registering is a host-side act, and
that is the point. A local stdio server runs on the host, and a remote server's
OAuth flow opens the user's host browser, so the credential never enters the
sandbox or depends on a host keychain/browser session there.

**Two ways to reach a sandbox, and they do not mix:**

| Route | Command | Trade |
|---|---|---|
| Live attach | `omp-sbx mcp-import --load` | No restart; the agent also gets the gateway's `mcp-add` / `mcp-find` |
| Fixed at create | `OMP_SBX_STATIC_MCP=notion,searxng omp-sbx --new` | Set cannot change without `--new`; the agent cannot register more |

Loading into a sandbox created with `--static-mcp` misbehaves - the second load
appears to replace the first rather than add to it. Pick one route.

**What does not carry over.** `sbx mcp add` has no `--env`, so a server that
reads its config from the environment gets wrapped in `env VAR=value <command>`.
The values stay on the host, and the script prints them as `VAR=...` rather than
echoing a secret. A server whose own binary fails to start on the host fails
here too, and the attach step reports the reason.

#### Letting omp see them

Registering and attaching gets the servers onto the sandbox's gateway. omp still
has to connect to it, which a project turns on with one line in its `.env`:

```bash
OMP_SBX_MCP_GATEWAY=1
```

Off by default. A gateway with nothing attached still hands omp the meta-tools
that register more servers, and that is not a choice to make for every project.

The tools arrive named `mcp__sbx_gateway_<tool>`, so `searxng_web_search` becomes
`mcp__sbx_gateway_searxng_web_search`. Check what mounted with `/mcp` in a
session.

`omp-init.sh` writes the server definition into a `--plugin-dir` root under
`~/.cache` inside the sandbox, not an `mcp.json`. OMP's config directories are
either persistent private state or project files, so writing the sandbox-only
gateway URL there would outlive the endpoint that serves it.

Changing `OMP_SBX_MCP_GATEWAY` takes effect on the next launch. Changing
`omp-init.sh` needs `omp-sbx build` and `omp-sbx --new`.

### LSP servers

The image ships 51 of the 55 enabled language-server commands in OMP
`v18.1.21`. The two Nix servers (`nil` and `nixd`), `ocamllsp`, and
`tlapm_lsp` are intentionally omitted. The Nix package manager, profiles, and
runtime are not installed. The Docker build fails if this 51-installed/4-omitted
contract changes or any required command is missing.

**Installation is split by distribution mechanism:**

| Package-manager servers | `sbx-kit/Dockerfile` | Pinned npm, Go, uv, Ruby, Kotlin, Erlang, and Swift installs |
| Native/toolchain servers | `sbx-kit/install-native-lsps.sh` | Architecture-specific, pinned, checksum-verified releases and source builds |
| Image-owned project registrations | `sbx-kit/omp-lsp/lsp.json` | Baked into the interactive image and loaded with `--plugin-dir`; currently Azure Pipelines |
| User registration overrides | `/home/agent/.omp/agent/lsp.yml` | Live in that sandbox's private persistent state; no image rebuild needed |
Downloaded tool artifacts live under `/home/agent/.local/share/<tool>`.
Stable command entry points live in `/home/agent/.local/bin`; NVM-managed
Node 20/22/24/26, Cargo, GHCup, Swift, Go, Bun, Ruby, and pnpm paths remain
available for their native toolchains. Node 24 is the default runtime. The
installer supports Linux `x86_64`/`amd64` and `aarch64`/`arm64` and rejects
other architectures.

#### Lazy loading and overrides

omp starts LSP servers **lazily**, keyed on `fileTypes` matching actual files
in the open workspace. A server activates only when a workspace contains a
matching file. The message *“No language servers configured for this project”*
from `lsp status` means no workspace file matched; it does not mean the
configuration is missing.

`rootMarkers` such as `.git`, `go.mod`, and `package.json` select the project
root but do not start a server without a matching file type. The image-owned
Azure Pipelines registration uses the root marker `pipelines`, YAML extensions
`.yaml` and `.yml`, and the command
`azure-pipelines-language-server --stdio`. It therefore activates for YAML
files in a workspace whose root contains a `pipelines/` directory. OMP routes
by file type and root marker; it cannot restrict the registration to only
recursive `pipelines/**/*.{yaml,yml}` paths within that workspace.

The Azure registration is immutable image content under
`/opt/omp-sbx/lsp/`; do not copy it into private persistent
`/home/agent/.omp/agent/lsp.yml`. Entries in the private file can replace or
extend OMP's defaults and the image registration for the next session.

#### Adding or changing a server

1. Pin and install its command in `sbx-kit/Dockerfile` or
   `sbx-kit/install-native-lsps.sh`. Verify upstream checksums or signatures
   when published; otherwise pin a reviewed checksum or source commit.
2. For an image-owned custom registration, add its manifest under
   `sbx-kit/omp-lsp/` and keep the corresponding command in the image. Load
   that directory with the guest startup's `--plugin-dir` rather than writing
   to the sandbox's private durable `~/.omp/agent/lsp.yml`.
3. Use `~/.omp/agent/lsp.yml` only for sandbox-specific overrides and
   extensions that should live outside the image.
4. Rebuild and load the image with `omp-sbx build`, then start a fresh sandbox
   with `omp-sbx --new`. Registration-only changes need a fresh session, and
   an existing sandbox does not receive a rebuilt image until it is recreated.


### Browser automation

OMP's Eval `browser` API is enabled by default. The template installs
Playwright's architecture-native Chromium and sets
`PUPPETEER_EXECUTABLE_PATH=/usr/local/bin/chromium`, preventing OMP from
downloading a Chrome-for-Testing binary that may be incompatible with the sbx
microVM architecture.

Use the browser from an Eval JavaScript cell:

```javascript
const tab = await browser.open({ name: "docs", url: "https://example.com" });
const observed = await tab.observe();
const title = await tab.title();
await tab.close();
```

The API also supports `click`, `fill`, `press`, `screenshot`, and custom
Puppeteer work through `tab.run`. Use the `read` tool instead for static URLs
that do not require JavaScript or interaction.

The image pins Chromium through `playwright@1.63.0`. Playwright-driven project
tests must use the same Playwright release because each release requires its
matching browser revision:

```bash
pnpm add --save-dev --save-exact playwright@1.63.0
```

The sbx TLS proxy injects its CA into the container trust store, so Chromium
keeps certificate validation enabled. Do not set
`PUPPETEER_PROXY_IGNORE_CERT_ERRORS`.

Changing the Playwright Chromium version requires `omp-sbx build` and a fresh
sandbox (`omp-sbx --new`).

### Security

All security is handled by the sbx microVM — no manual `cap_drop`, `gosu`, `umask`, or read-only rootfs configuration needed:

| Control | sbx |
|---|---|
| Isolation | MicroVM with separate kernel |
| Non-root user | Built-in `agent` UID 1000 |
| Network | Policy-based allow-list |
| Docker images | Project allowlist enforced by daemon AuthZ and Buildx policy |
| Secrets | Proxy injects keys (never enter sandbox) |
| Resource limits | `sbx run --memory --cpus` |

## Parallel sessions (git worktrees)

`omp-sbx parallel` creates a git worktree on a separate branch and launches a dedicated sandbox. Run it multiple times to work on multiple tasks in parallel.

```bash
omp-sbx parallel                          # interactive branch selection
omp-sbx parallel --new fix-auth-bug       # create new branch and worktree
omp-sbx parallel --branch feature-x       # use existing branch
```

On exit (interactive mode), you're offered cleanup:
1. Merge the branch into your current branch and remove the worktree
2. Remove the worktree only (keep the branch)
3. Keep the worktree as-is

Worktrees are created as siblings of the repo root: `~/src/myproject@fix-auth-bug`

### VS Code worktree integration

`omp-sbx parallel` maintains a multi-root `.code-workspace` file at the repo root (`<repo-name>.code-workspace`) so VS Code can display all active worktrees as named roots in one window. The file is gitignored (`*.code-workspace`) — it is machine-local and never committed.

**What happens automatically:**

| Event | `.code-workspace` action |
|---|---|
| Worktree created/reused | Worktree added as a named root |
| Cleanup: merge + remove | Root removed |
| Cleanup: remove only | Root removed |
| Cleanup: keep as-is | Root left in file |

**Folder naming:** main checkout is `<repo-name>`; each worktree is `<repo-name> <branch>`. This lets VS Code tasks pin cwd via `${workspaceFolder:<name>}`:

```json
{
  "label": "agent: feature-x",
  "type": "shell",
  "command": "omp-sbx parallel --branch feature-x",
  "options": { "cwd": "${workspaceFolder:myrepo feature-x}" }
}
```

For full agent instructions, see [`INSTRUCTIONS.md`](INSTRUCTIONS.md).

**VS Code settings:** enable `git.detectWorktrees` to auto-list all worktrees in Source Control, even ones created outside VS Code.

## Scripted / CI use (experimental)

`omp-sbx env` is the declarative launcher built on Docker sbx's `sbx env`
commands (sbx v0.43+). It fits headless automation better than the interactive
`omp-sbx run` flow:

```bash
omp-sbx env --version
omp-sbx env --new
```

The Rust command supplies `kitDir`, `workspace`, `ompState`, and `dockerPolicy`
to `sbx-kit/sbxenv.yaml` with repeatable `--env-arg` flags on every create,
run, exec, and remove operation. It also supplies the same explicit sandbox
name and environment-file path every time. A changed kit or incompatible
legacy `~/.omp` mount causes documented `sbx env rm --force` recreation while
the private host state directory remains intact.

**Known gaps vs `omp-sbx run`:**
- No force-quit recovery cascade (re-attach → restart → recreate) — just
  create-or-reuse.
- No `~/.config/gh` forwarding. A static env file cannot conditionally mount a
  path that may not exist on every host without risking an interactive prompt
  in CI.

For day-to-day interactive use, use `omp-sbx run`.

## Rebuild after omp upgrade

```bash
omp-sbx build
```
