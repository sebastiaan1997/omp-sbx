#!/usr/bin/env bash
# omp-init.sh - the image entrypoint: finish setup, then exec omp.
#
# Anything omp reads at load time is wired up here. The spec.yaml startup
# hook runs in parallel with omp itself and can lose that race.
set -euo pipefail

# Install the per-project Docker image policy before OMP can issue Docker
# commands. The helper owns daemon/plugin convergence and fails closed.
sudo -n /usr/local/libexec/docker-policy-start.sh

# ── Per-sandbox OMP state mount symlink ─────────────────────────────────────
# sbx mounts the launcher's private .omp directory at the same absolute host
# path inside the VM. Point OMP's stable guest path at that virtiofs mount.
OMP_HOST="$(awk '/virtiofs/{print $2}' /proc/mounts | grep '/\.omp$' | head -1 || true)"
if [ -n "$OMP_HOST" ] && [ "$OMP_HOST" != "$HOME/.omp" ]; then
  if [ -e "$HOME/.omp" ] && [ ! -L "$HOME/.omp" ]; then
    rm -rf "$HOME/.omp"
  fi
  ln -sfn "$OMP_HOST" "$HOME/.omp"
  if [ -L "$OMP_HOST/.omp" ]; then
    rm -f "$OMP_HOST/.omp"
  fi
else
  mkdir -p "$HOME/.omp"
  echo "omp-sbx: warning: private .omp mount not found; OMP state is VM-local and will be lost when this sandbox is removed" >&2
fi


# ── Unsubstituted GH_TOKEN placeholder ──────────────────────────────────────
# sbx injects a GH_TOKEN placeholder and substitutes a real token only when a
# `github` secret is registered (`sbx secret set github`). gh prefers env vars
# over the mounted ~/.config/gh session, so an unsubstituted placeholder fails
# every call with "invalid token" instead of falling back. A real token has a
# different prefix and survives.
case "${GH_TOKEN:-}" in
  gho_sbxproxymanaged*) unset GH_TOKEN ;;
esac

# Read supported project .env keys without sourcing shell code.
env_value() {
  local key="$1" file="$2" val
  val="$(sed -n "s/^[[:space:]]*${key}[[:space:]]*=[[:space:]]*//p" "$file" | head -1)"
  val="${val%$'\r'}"
  val="${val%\"}"; val="${val#\"}"
  val="${val%\'}"; val="${val#\'}"
  printf '%s' "$val"
}

# Flags this script adds ahead of the caller's own, e.g. --plugin-dir.
OMP_ARGS=()
OMP_ARGS+=(--plugin-dir /opt/omp-sbx/lsp)

# Read the host path, not /home/agent/workspace: the symlink to it belongs to
# the startup hook, which has not necessarily run yet.
WORKSPACE_ENV="${WORKSPACE_DIR:-/home/agent/workspace}/.env"

# ── MCP gateway (opt-in) ────────────────────────────────────────────────────
# sbx serves the host's MCP registry to this sandbox over a gateway and injects
# its URL. Register the servers on the host first, with omp-sbx-mcp-import.
#
# A project opts in with OMP_SBX_MCP_GATEWAY=1 in its .env. Off by default: a
# gateway with nothing attached still hands omp the meta-tools that register
# more servers, which is not a choice to make on every project's behalf.
#
# The server definition goes in a --plugin-dir root rather than an mcp.json.
# omp looks for mcp.json in its private persistent config and the workspace, so
# writing the sandbox-only gateway URL there would outlive the endpoint. A
# plugin root is read from wherever it points.
MCP_GATEWAY_OPT=""
if [ -f "$WORKSPACE_ENV" ]; then
  MCP_GATEWAY_OPT="$(env_value OMP_SBX_MCP_GATEWAY "$WORKSPACE_ENV")"
fi

case "$MCP_GATEWAY_OPT" in
  1|true|on|yes)
    if [ -z "${MCP_GATEWAY_URL:-}" ]; then
      echo "omp-sbx: OMP_SBX_MCP_GATEWAY is set but this sandbox has no MCP gateway" >&2
    else
      # Rewritten every launch, so a sandbox that comes back with a different
      # gateway URL picks it up.
      #
      # The manifest is a bare plugin.json for its name alone, which becomes the
      # tool prefix: mcp__sbx_gateway_<tool>. omp reads the name from here or the
      # directory basename, and nowhere else. It reads the servers from .mcp.json
      # once no manifest declares an mcpServers pointer, so the two stay apart.
      MCP_PLUGIN_DIR="$HOME/.cache/omp-sbx/plugins/sbx"
      mkdir -p "$MCP_PLUGIN_DIR"
      printf '{"name":"sbx","version":"local"}\n' >"$MCP_PLUGIN_DIR/plugin.json"
      printf '{"mcpServers":{"gateway":{"type":"http","url":"%s"}}}\n' "$MCP_GATEWAY_URL" \
        >"$MCP_PLUGIN_DIR/.mcp.json"
      OMP_ARGS+=(--plugin-dir "$MCP_PLUGIN_DIR")
    fi
    ;;
esac


# ── Shared SBX agent skills ──────────────────────────────────────────────────
SBX_SKILLS_MOUNT=""

for _ in $(seq 1 100); do
    SBX_SKILLS_MOUNT="$(
        awk '$3 == "virtiofs" {print $2}' /proc/mounts \
            | grep '/sandboxes/sandboxes/agent-skills$' \
            | head -n 1 || true
    )"

    if [ -n "$SBX_SKILLS_MOUNT" ] && [ -d "$SBX_SKILLS_MOUNT" ]; then
        break
    fi

    sleep 0.05
done

if [ -n "$SBX_SKILLS_MOUNT" ] && [ -d "$SBX_SKILLS_MOUNT" ]; then
    mkdir -p "$HOME/.agents"

    if [ -e "$HOME/.agents/skills" ] && [ ! -L "$HOME/.agents/skills" ]; then
        rm -rf "$HOME/.agents/skills"
    fi

    ln -sfn "$SBX_SKILLS_MOUNT" "$HOME/.agents/skills"

    echo "omp-sbx: shared skills: $HOME/.agents/skills -> $SBX_SKILLS_MOUNT" >&2
else
    echo "omp-sbx: warning: shared SBX skills mount not found" >&2
fi

# The startup hook turns this path into a symlink to the host workspace.
cd /home/agent/workspace
# Keep the bundled binary current on every sandbox start. The updater already
# verifies release metadata and the downloaded binary before replacing omp.
# An explicit `omp update` is passed through once rather than checked twice.
if [ "${1:-}" != "update" ]; then
  if OMP_UPDATE_OUTPUT="$(omp update 2>&1)"; then
    if [ -n "$OMP_UPDATE_OUTPUT" ]; then
      printf '%s\n' "$OMP_UPDATE_OUTPUT" >&2
    fi
  else
    OMP_UPDATE_STATUS=$?
    printf 'omp-sbx: warning: automatic omp update failed (exit %s)\n%s\n' \
      "$OMP_UPDATE_STATUS" "$OMP_UPDATE_OUTPUT" >&2
  fi
fi

exec omp "${OMP_ARGS[@]}" "$@"