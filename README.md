<p align="center">
  <img width="250" alt="Image" src="https://github.com/user-attachments/assets/c91d67e4-c6a6-46c0-a7dd-4f682cc67193" />
</p>

# omp-sbx Oh My Pi Sandbox

Run the [omp coding agent](https://omp.sh) (oh-my-pi) inside a [Docker sbx](https://docs.docker.com/ai/sandboxes/) sandbox with host configs shared.

## What it does

- Launches omp inside a Docker sbx microVM — sbx handles all security (non-root user, network policies, secret proxy, resource limits)
- Provides a private Docker Engine inside the sbx microVM, so `/var/run/docker.sock`, `docker ps`, `docker build`, and Docker Compose target sandbox-local containers/images instead of the host daemon
- Bind-mounts your `~/.omp` (agent.db, managed-skills, memories, sessions) so state persists across sandbox restarts
- Sandboxes are per-directory: running from the same cwd reconnects to the same sandbox
- Recovers automatically after a force-quit: if the sandbox is left stopped (or its agent wedged), the launcher re-attaches, then stops+restarts, and as a last resort recreates the sandbox — your omp session resumes from the shared `~/.omp` either way
- `--new` flag forces a fresh sandbox

## Prerequisites

```bash
brew install docker/tap/sbx
sbx login
sbx policy set-default balanced
```

## Install

```bash
git clone https://github.com/mikeatlas/omp-sbx.git ~/src/github.com/mikeatlas/omp-sbx
cd ~/src/github.com/mikeatlas/omp-sbx

# Build + load the interactive and configure-only template images into sbx
./build.sh

# Rebuild only the minimal configure template when needed
./build-configure.sh

# Symlink the launcher onto your PATH
ln -sf "$PWD/omp-sbx" ~/.local/bin/omp-sbx

# Alias omp to always use the sandbox (in ~/.zshrc or ~/.bashrc)
echo "alias omp='omp-sbx'" >> ~/.zshrc
```

## Usage

```bash
omp                    # interactive TUI (cwd = workspace, ~/.omp shared)
omp --new              # destroy + create fresh sandbox
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
| Configure builder | `build-configure.sh` | Builds, tags, and loads only the local configure template |
| Env (experimental) | `sbx-kit/.sbxenv.yaml` + `omp-sbxenv` | Declarative alternative launcher for scripted/CI use — see [Scripted / CI use](#scripted--ci-use-experimental) |
| Launcher | `omp-sbx` | Wrapper handling banner, sandbox lifecycle, resume vs new |
| Model configuration | `omp-sbx-configure-models` | Discovers available models and updates global OMP role and bundled-agent routing |
| Parallel | `omp-sbx-parallel` | Git worktree-based parallel sandbox launcher |
| MCP import | `omp-sbx-mcp-import` | Registers Claude Code's MCP servers with sbx - see [MCP servers](#mcp-servers-from-claude-code) |
| MCP gateway | `sbx-kit/omp-init.sh` | Opt-in wiring that connects omp to the sandbox's MCP gateway |
| Browser | `sbx-kit/Dockerfile` | Installs architecture-native Chromium for OMP's built-in browser API |
| Bedrock auth | `sbx-kit/omp-init.sh` | Opt-in AWS SSO profile with browserless renewal - see [Amazon Bedrock](#amazon-bedrock-aws-sso) |
| Bedrock login | `omp-sbx-aws-login` | Host-side SSO login, shared with every sandbox via `~/.omp/aws-sso-cache` |
| SSO nudge | `sbx-kit/extensions/aws-sso-nudge.ts` | omp extension: warns before the SSO login lapses, adds `/aws-login` |

### Config sharing

sbx mounts additional workspaces at their **host path** inside the microVM (e.g. `/Users/<user>/.omp`). The kit's startup command symlinks this to `/home/agent/.omp` so omp's `PI_CONFIG_DIR=.omp` resolves correctly.

### Model defaults

`omp --configure` creates a fresh disposable sandbox from the lightweight
configure image, which has OMP but no nested Docker daemon, browser, language
servers, or development toolchain. It discovers the model catalog visible with
the current host-wide sbx provider credentials and shared OMP authentication,
then opens an Up/Down menu for each missing fast, standard, or deep tier. Each
model selection is followed by a thinking-level menu.
The configure image uses a small argument-tolerant entrypoint that ignores
sandbox-injected agent or MCP arguments and parks instead of launching an
interactive OMP session. The helper uses noninteractive `sbx exec` commands,
removes the sandbox on every exit path, and then returns control to the host.
If the local `omp-sbx-configure:latest` template is missing or exits immediately
because it is stale, the helper invokes `build-configure.sh` once. The saved
Docker image carries that exact tag, so the one-argument `sbx template load`
supported by the installed CLI restores the local name before creation is
retried. Custom `OMP_SBX_CONFIGURE_TEMPLATE` values are never built implicitly.

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
`task.agentModelOverrides`, then publishes them to the native host configuration,
normally the physical target of
`~/.omp/agent/config.yml`. The helper resolves the guest `~/.omp` mount and
OMP's active directory separately, requires the active directory to stay within
that guest mount, then maps its relative path onto the physical host `~/.omp`.
This handles sbx exposing the host mount at a guest-only path such as
`/home/agent/.omp`. The resolved host file is printed in the dry-run summary and
after a successful write. `/var/tmp/omp-configure-models-*` is only a
project-free guest working directory, so project overrides are never copied
into global defaults.
“Global” means shared across this user's `omp-sbx` sandboxes, not all OS users.

A successful configuration also sets global `modelRoleStorage: project`, enabling
global/project save choices in `/model` → Roles. To override a role for one
project, open Roles in that project's session and save the assignment to Project.
Native `.omp/config.yml` role keys override matching global roles; other roles
continue to inherit global defaults. The helper never changes project files.
The storage preference controls hub save choices, not the helper's write scope.

The helper copies the current native host file into a private mounted staging
directory, lets OMP serialize and verify all three settings there, and only then
atomically replaces the host `config.yml`. A failed update or readback leaves the
host file untouched; semantic rollback is confined to the disposable staging
copy.
An OMP process reads these settings at startup. Exit and relaunch it after
configuration; if the launcher reattaches to the already-running process, use
`omp --new` to restart the sandbox process while retaining shared session data.

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
omp-sbx-parallel --branch feature-x --refresh-image-policy
omp-sbxenv --refresh-image-policy
```

The refresh recreates the affected sandbox/environment so the new snapshot is
compiled and activated. Deleting the project YAML and refreshing switches the
policy to deny-all. Sandboxes created before snapshot mounting was added also
default to deny-all until refreshed once.

This is a guardrail, not a hard security boundary. Passwordless `sudo` can
remove the plugin or reconfigure the daemon. Docker AuthZ also does not cover
native/upgraded gRPC, and non-JSON build contexts are enforced by the CLI
wrapper plus Buildx policy rather than inspected by AuthZ.

`./build.sh` verifies this contract by creating a throwaway sandbox and running:

```bash
test -S /var/run/docker.sock
docker version
docker ps
```

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

```bash
omp-sbx-mcp-import --dry-run   # read ~/.claude.json, print the plan
omp-sbx-mcp-import --auth      # register, then authorize the remote ones
omp-sbx-mcp-import --load      # attach them to this directory's sandbox
```

sbx keeps its own MCP registry and serves it to a sandbox through a gateway.
`omp-sbx-mcp-import` copies the servers out of `~/.claude.json` (plus a project
`.mcp.json`, or any `--file`) into that registry. It is idempotent: a second run
reports what already exists and changes nothing.

**Why not just mount the Claude config.** Registering is a host-side act, and
that is the point. A local stdio server runs on the host, and a remote server's
OAuth flow opens the user's host browser, so the credential never enters the
sandbox or depends on a host keychain/browser session there.

**Two ways to reach a sandbox, and they do not mix:**

| Route | Command | Trade |
|---|---|---|
| Live attach | `omp-sbx-mcp-import --load` | No restart; the agent also gets the gateway's `mcp-add` / `mcp-find` |
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
`~/.cache` inside the sandbox, not an `mcp.json`. Every config dir omp looks in
for `mcp.json` is a host mount here, so writing one would leave a URL in the
shared host config that resolves only inside a sandbox.

Changing `OMP_SBX_MCP_GATEWAY` takes effect on the next launch. Changing
`omp-init.sh` needs `./build.sh` and `omp --new`.

### Amazon Bedrock (AWS SSO)

Off by default. A project turns it on with one line in its `.env`:

```bash
OMP_SBX_AWS_PROFILE=infra-dev-bedrock
OMP_SBX_AWS_REGION=us-east-1          # optional, defaults to us-east-1
```

Define that profile once in `~/.omp/aws-config` on the host. The file uses AWS
CLI config syntax and holds no secrets:

```ini
[sso-session my-sso]
sso_start_url = https://d-xxxxxxxxxx.awsapps.com/start
sso_region = us-east-1
sso_registration_scopes = sso:account:access

[profile infra-dev-bedrock]
sso_session = my-sso
sso_account_id = 000000000000
sso_role_name = Bedrock-Invoke-Only
region = us-east-1
```

`~/.omp` is already bind-mounted, so editing `aws-config` takes effect on the
next session - no rebuild.

**Log in once, from the host, and every sandbox shares it.** Run:

```bash
omp-sbx-aws-login
```

from a project with `OMP_SBX_AWS_PROFILE` set (or `omp-sbx-aws-login --profile
<name>` anywhere). It opens the user's interactive browser on the host and
writes the resulting SSO token into `~/.omp/aws-sso-cache`, which
every sandbox symlinks `~/.aws/sso/cache` to. A brand new sandbox, or one
recreated with `--new`, picks up an already-valid session immediately; nothing
about the token needs redoing per sandbox.

Unlike `aws-config` above, this directory does hold something live: a
refreshable SSO session, scoped to the role in that profile. Sharing it is the
same trust boundary `~/.omp` already carries for everything else in it - one
identity, shared across this host's own sandboxes.

A sandbox can still log in on its own if you'd rather not leave it: run
`/aws-login` once the session starts, which the nudge extension below handles
with a device-code flow. The sandbox's headless Chromium cannot complete an
interactive user login, and the alternative PKCE flow redirects to a loopback
port that the host browser cannot reach. That login is shared too, through the
same symlink.

**Renewal is browserless.** `omp-init.sh` generates an `omp-bedrock` profile
whose `credential_process` calls `aws configure export-credentials`. omp reads
the SSO access token but not the refresh token stored next to it, so on its own
it treats an expired token as fatal. The AWS CLI does read that refresh token,
so routing through it renews silently. This requires the `[sso-session]` profile
shape above - a legacy profile with an inline `sso_start_url` gets no refresh
token from the CLI.

Three lifetimes stack up, and only the longest one needs you at a browser:

| Layer | Typical lifetime | Renewal |
|---|---|---|
| Role credentials | 12 hours | Minted from the access token |
| SSO access token | 1 hour | Silent, `grantType: refresh_token` |
| Client registration | ~31 days | `aws sso login`, opening a URL |

Read your own values from `~/.aws/sso/cache/*.json`: `expiresAt` is the access
token and `registrationExpiresAt` on the same entry is the registration. The role
credentials carry their own `Expiration`, visible via
`aws configure export-credentials`.

The role credentials are what actually sign a Bedrock request, and omp caches
them for their full 12 hours. Only when they lapse does it re-read the SSO access
token - which is an hour old at most, and which omp cannot renew. So without
`credential_process` a session dies at the 12 hour mark, and a session started
more than an hour after the last refresh fails immediately. Sending it through
the CLI removes both cliffs, because the CLI renews the access token from the
refresh token with no browser.

One caveat worth knowing: `aws sso login` restarts authorization from scratch
every time, even when the cached token is still valid. Only the credential path
(`aws configure export-credentials`) refreshes silently, which is why the
generated profile uses it.

**The nudge extension** (`sbx-kit/extensions/aws-sso-nudge.ts`) covers the
30-day boundary, which nothing renews on its own. Loaded only when Bedrock is
on, it checks every 15 minutes and warns in the chat once fewer than 2 days
remain (`OMP_SBX_AWS_SSO_WARN_DAYS` overrides the threshold). If credentials stop
working mid-session, it runs the device-code login itself and puts the URL in the
chat - open it on your host and the session recovers without a restart.

It stays out of the status line except while a login is waiting for approval.
The registration is weeks from expiry nearly always, so a standing countdown is
noise. Read the current values from `~/.aws/sso/cache/*.json` when you want them.

`AWS_CA_BUNDLE` is set in `spec.yaml` because botocore ignores the OS trust
store in favor of its own bundle, which the sbx TLS proxy would otherwise break.

Adding a region means adding its `bedrock-runtime`, `oidc`, `portal.sso`, and
`sts` hosts to the network allow-list in `sbx-kit/spec.yaml`.

### LSP servers

The image ships 51 of the 55 enabled language-server commands in OMP
`v18.1.21`. The two Nix servers (`nil` and `nixd`), `ocamllsp`, and
`tlapm_lsp` are intentionally omitted. The Nix package manager, profiles, and
runtime are not installed. The Docker build fails if this 51-installed/4-omitted
contract changes or any required command is missing.

**Installation is split by distribution mechanism:**

| Part | Location | Details |
|---|---|---|
| Package-manager servers | `sbx-kit/Dockerfile` | Pinned npm, Go, uv, Ruby, Kotlin, Erlang, and Swift installs |
| Native/toolchain servers | `sbx-kit/install-native-lsps.sh` | Architecture-specific, pinned, checksum-verified releases and source builds |
| User registration overrides | `~/.omp/lsp.yml` on the host | Live through the `~/.omp` bind mount; no image rebuild needed |

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
root but do not start a server without a matching file type. Entries in the
host's `~/.omp/lsp.yml` can replace or extend OMP's defaults for the next
session.

#### Adding or changing a server

1. Pin and install its command in `sbx-kit/Dockerfile` or
   `sbx-kit/install-native-lsps.sh`. Verify upstream checksums or signatures
   when published; otherwise pin a reviewed checksum or source commit.
2. If OMP does not register it by default, add its command, arguments, file
   types, and root markers to `~/.omp/lsp.yml`.
3. Rebuild and load the image with `./build.sh`, then start a fresh sandbox
   with `omp --new`. Registration-only changes need only a fresh session.

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

Changing the Playwright Chromium version requires a rebuild (`./build.sh`) and
a fresh sandbox (`omp --new`).

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

`omp-sbx-parallel` creates a git worktree on a separate branch and launches a dedicated sandbox for it. Run it multiple times to work on multiple tasks in parallel — each gets its own worktree, branch, and sandbox.

```bash
omp-sbx-parallel                          # interactive: pick existing branch or create new
omp-sbx-parallel --new fix-auth-bug       # create new branch + worktree + sandbox
omp-sbx-parallel --branch feature-x       # use existing branch in a new worktree
```

On exit (interactive mode), you're offered cleanup:
1. Merge the branch into your current branch and remove the worktree
2. Remove the worktree only (keep the branch)
3. Keep the worktree as-is

Worktrees are created as siblings of the repo root: `~/src/myproject@fix-auth-bug`

### VS Code worktree integration

`omp-sbx-parallel` maintains a multi-root `.code-workspace` file at the repo root (`<repo-name>.code-workspace`) so VS Code can display all active worktrees as named roots in one window. The file is gitignored (`*.code-workspace`) — it's machine-local, never committed.

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
  "command": "omp-sbx-parallel --branch feature-x",
  "options": { "cwd": "${workspaceFolder:myrepo feature-x}" }
}
```

Requires `jq` on the host (silently skips if unavailable). For full agent instructions, see [`INSTRUCTIONS.md`](INSTRUCTIONS.md).

**VS Code settings:** enable `git.detectWorktrees` to auto-list all worktrees in Source Control, even ones created outside VS Code.

## Scripted / CI use (experimental)

`omp-sbxenv` is an alternative launcher built on Docker sbx's declarative
`.sbxenv.yaml` + `sbx env` commands (sbx v0.39+; Docker marks `sbx env`
experimental and subject to change). It fits headless automation better than
`omp-sbx`'s interactive create/pause/attach flow:

```bash
omp-sbxenv --version   # create (if needed) + run a one-shot command
omp-sbxenv --new        # remove + recreate the environment
```

Under the hood this templates `sbx-kit/.sbxenv.yaml` with `${VAR}` values the
script exports (workspace path, kit path, sandbox name) and calls
`sbx env create` / `sbx env exec` / `sbx env rm` directly — no host-mounted
secret files, since `.sbxenv.yaml` (unlike `spec.yaml`) expands `${VAR}`
placeholders for real.

**Known gaps vs `omp-sbx`:**
- No force-quit recovery cascade (re-attach → restart → recreate) — just
  create-or-reuse.
- No `~/.config/gh` forwarding. A static env file can't conditionally mount a
  path that may not exist on every host, and `sbx env create` prompts
  interactively — hanging in non-interactive/CI contexts — if
  `additionalWorkspaces` points at a missing directory.
- Re-running `sbx env run` on an existing environment only re-applies
  env/MCP changes; other `.sbxenv.yaml` edits need `--new`.

For day-to-day interactive use, stick with `omp-sbx`.

## Rebuild after omp upgrade

```bash
cd ~/src/github.com/mikeatlas/omp-sbx
./build.sh
```
