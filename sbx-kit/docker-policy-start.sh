#!/usr/bin/env bash
set -euo pipefail

if [ "$(id -u)" -ne 0 ]; then
  echo "omp-sbx: docker-policy-start.sh must run as root" >&2
  exit 1
fi

RUNTIME_DIR=/run/omp-sbx
POLICY_DIR=/var/lib/omp-sbx-policy
PLUGIN_NAME=omp-sbx-image-policy
PLUGIN_BIN=/usr/local/libexec/docker-image-authz
COMPILER=/usr/local/libexec/docker-image-policy.py
PLUGIN_SOCKET="$RUNTIME_DIR/image-authz.sock"
NEXT_SOCKET="$RUNTIME_DIR/image-authz.next.sock"
OLD_SOCKET="$RUNTIME_DIR/image-authz.old.sock"
PID_FILE="$RUNTIME_DIR/image-authz.pid"
NEXT_PID_FILE="$RUNTIME_DIR/image-authz.next.pid"
DAEMON_CONFIG=/etc/docker/daemon.json

mkdir -p "$RUNTIME_DIR" "$POLICY_DIR" /etc/docker
chmod 0755 "$RUNTIME_DIR" "$POLICY_DIR"

POLICY_SOURCE="$({
python3 - <<'PY'
import errno
from pathlib import Path

# Keyed by resolved path so one host directory offered through several mounts
# counts once; the value is the mount path, which is the one read later.
matches = {}
with open('/proc/mounts', encoding='utf-8') as mounts:
    for line in mounts:
        fields = line.split()
        if len(fields) < 4 or fields[2] != 'virtiofs':
            continue
        options = set(fields[3].split(','))
        if 'ro' not in options:
            continue
        path = fields[1]
        for encoded, decoded in [('\\040', ' '), ('\\011', '\t'), ('\\012', '\n'), ('\\134', '\\')]:
            path = path.replace(encoded, decoded)
        mounted = Path(path)
        if mounted.is_symlink():
            continue
        if mounted.is_dir():
            candidate = mounted / '.omp-sbx-docker-images.yaml'
        elif mounted.name == '.omp-sbx-docker-images.yaml':
            candidate = mounted
        else:
            continue
        # A symlink would lead out of the read-only mount, to a path this
        # sandbox can rewrite.
        if candidate.is_symlink() or not candidate.is_file():
            continue
        matches.setdefault(str(candidate.resolve()), str(candidate))

if len(matches) > 1:
    raise SystemExit(f'omp-sbx: expected at most one read-only Docker image policy, found {len(matches)}')
if matches:
    source = next(iter(matches.values()))
    # This runs as root, which ignores the 0444 mode bits, so opening for write
    # probes the mount: a policy the sandbox can edit is not host-controlled.
    try:
        open(source, 'r+').close()
    except OSError as error:
        if error.errno != errno.EROFS:
            raise SystemExit(f'omp-sbx: cannot read Docker image policy {source}: {error}')
    else:
        raise SystemExit(f'omp-sbx: Docker image policy is writable inside the sandbox: {source}')
    print(source)
PY
} 2>&1)" || {
  printf '%s\n' "$POLICY_SOURCE" >&2
  exit 1
}

if [ -z "$POLICY_SOURCE" ]; then
  POLICY_SOURCE="$POLICY_DIR/deny-all.yaml"
  printf 'schemaVersion: 1\nallowedImages: []\n' > "$POLICY_SOURCE"
  chmod 0444 "$POLICY_SOURCE"
  POLICY_DESCRIPTION="deny-all (no read-only policy mount)"
else
  POLICY_DESCRIPTION="$POLICY_SOURCE"
fi

"$COMPILER" compile --input "$POLICY_SOURCE" --output-dir "$POLICY_DIR"
POLICY_ID="$(cat "$POLICY_DIR/policy-id")"

rm -f "$NEXT_SOCKET" "$NEXT_PID_FILE"
nohup "$PLUGIN_BIN" --policy "$POLICY_DIR/policy.json" --socket "$NEXT_SOCKET" \
  >"$RUNTIME_DIR/image-authz.log" 2>&1 &
NEXT_PID=$!
printf '%s\n' "$NEXT_PID" > "$NEXT_PID_FILE"
plugin_ready=false
for _ in $(seq 1 100); do
  if curl --silent --fail --unix-socket "$NEXT_SOCKET" \
      -X POST http://localhost/Plugin.Activate 2>/dev/null | grep -q '"authz"'; then
    plugin_ready=true
    break
  fi
  if ! kill -0 "$NEXT_PID" 2>/dev/null; then
    break
  fi
  sleep 0.05
done
if [ "$plugin_ready" != true ]; then
  kill "$NEXT_PID" 2>/dev/null || true
  wait "$NEXT_PID" 2>/dev/null || true
  cat "$RUNTIME_DIR/image-authz.log" >&2 || true
  echo "omp-sbx: Docker image authorization plugin did not become ready" >&2
  exit 1
fi

OLD_PID=""
if [ -f "$PID_FILE" ]; then
  OLD_PID="$(cat "$PID_FILE")"
fi
rm -f "$OLD_SOCKET"
if [ -S "$PLUGIN_SOCKET" ]; then
  mv "$PLUGIN_SOCKET" "$OLD_SOCKET"
fi
mv "$NEXT_SOCKET" "$PLUGIN_SOCKET"
mv "$NEXT_PID_FILE" "$PID_FILE"

CONFIG_BACKUP="$RUNTIME_DIR/daemon.json.before-policy"
CONFIG_CANDIDATE="$RUNTIME_DIR/daemon.json.with-policy"
CONFIG_CHANGED=false
CONFIG_EXISTED=false
DOCKERD_PID=""
if [ -f "$DAEMON_CONFIG" ]; then
  CONFIG_EXISTED=true
  cp "$DAEMON_CONFIG" "$CONFIG_BACKUP"
else
  printf '{}\n' > "$CONFIG_BACKUP"
fi

rollback() {
  local status=$?
  trap - ERR INT TERM
  rm -f "$PLUGIN_SOCKET"
  if [ -S "$OLD_SOCKET" ]; then
    mv "$OLD_SOCKET" "$PLUGIN_SOCKET"
  fi
  if [ -n "$OLD_PID" ]; then
    printf '%s\n' "$OLD_PID" > "$PID_FILE"
  else
    rm -f "$PID_FILE"
  fi
  if [ "$CONFIG_CHANGED" = true ]; then
    if [ "$CONFIG_EXISTED" = true ]; then
      cp "$CONFIG_BACKUP" "$DAEMON_CONFIG"
    else
      rm -f "$DAEMON_CONFIG"
    fi
    if [ -n "$DOCKERD_PID" ]; then
      kill -HUP "$DOCKERD_PID" 2>/dev/null || true
    fi
  fi
  kill "$NEXT_PID" 2>/dev/null || true
  wait "$NEXT_PID" 2>/dev/null || true
  echo "omp-sbx: failed to enable Docker image authorization policy" >&2
  exit "$status"
}
trap rollback ERR INT TERM

python3 - "$CONFIG_BACKUP" "$CONFIG_CANDIDATE" "$PLUGIN_NAME" <<'PY'
import json
import sys
from pathlib import Path

source = Path(sys.argv[1])
destination = Path(sys.argv[2])
plugin = sys.argv[3]
with source.open(encoding='utf-8') as stream:
    config = json.load(stream)
if not isinstance(config, dict):
    raise SystemExit('omp-sbx: /etc/docker/daemon.json must contain a JSON object')
plugins = config.get('authorization-plugins', [])
if not isinstance(plugins, list) or any(not isinstance(value, str) for value in plugins):
    raise SystemExit('omp-sbx: authorization-plugins must be a string list')
if plugin not in plugins:
    plugins.append(plugin)
config['authorization-plugins'] = plugins
with destination.open('w', encoding='utf-8') as stream:
    json.dump(config, stream, sort_keys=True, separators=(',', ':'))
    stream.write('\n')
PY

if ! cmp -s "$CONFIG_BACKUP" "$CONFIG_CANDIDATE"; then
  cp "$CONFIG_CANDIDATE" "$DAEMON_CONFIG"
  CONFIG_CHANGED=true
fi

dockerd --validate --config-file="$DAEMON_CONFIG" >/dev/null
mapfile -t DOCKERD_PIDS < <(pgrep -x dockerd)
if [ "${#DOCKERD_PIDS[@]}" -ne 1 ]; then
  echo "omp-sbx: expected exactly one dockerd process, found ${#DOCKERD_PIDS[@]}" >&2
  false
fi
DOCKERD_PID="${DOCKERD_PIDS[0]}"
if [ "$CONFIG_CHANGED" = true ]; then
  kill -HUP "$DOCKERD_PID"
fi

probe_ok=false
for _ in $(seq 1 100); do
  response="$(curl --silent --show-error --unix-socket /var/run/docker.sock \
    -X POST 'http://localhost/v1.24/images/create?fromImage=omp-sbx-policy-probe-denied' || true)"
  if printf '%s' "$response" | grep -q "not approved by omp-sbx policy $POLICY_ID"; then
    probe_ok=true
    break
  fi
  sleep 0.05
done
if [ "$probe_ok" != true ]; then
  echo "omp-sbx: Docker daemon did not enforce the image authorization plugin" >&2
  false
fi

trap - ERR INT TERM
if [ -n "$OLD_PID" ] && [ "$OLD_PID" != "$NEXT_PID" ]; then
  kill "$OLD_PID" 2>/dev/null || true
fi
rm -f "$OLD_SOCKET" "$CONFIG_CANDIDATE" "$CONFIG_BACKUP"
echo "omp-sbx: Docker image policy active: $POLICY_ID ($POLICY_DESCRIPTION)" >&2
