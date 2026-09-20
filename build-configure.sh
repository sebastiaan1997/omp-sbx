#!/usr/bin/env bash
# Build and load only the lightweight omp --configure template.
set -euo pipefail

SCRIPT_PATH="$0"
while [ -L "$SCRIPT_PATH" ]; do
  SCRIPT_DIR="$(cd "$(dirname "$SCRIPT_PATH")" && pwd)"
  SCRIPT_PATH="$(readlink "$SCRIPT_PATH")"
  case "$SCRIPT_PATH" in /*) ;; *) SCRIPT_PATH="$SCRIPT_DIR/$SCRIPT_PATH" ;; esac
done
DIR="$(cd "$(dirname "$SCRIPT_PATH")" && pwd)"

# shellcheck source=sbx-preflight.sh
source "$DIR/sbx-preflight.sh"
ensure_sbx "build-configure.sh"

if ! command -v docker >/dev/null 2>&1; then
  printf '%s\n' 'build-configure.sh: docker CLI not found' >&2
  exit 1
fi

OMP_VERSION="${OMP_VERSION:-18.1.10}"
if [ "$OMP_VERSION" = latest ]; then
  printf '%s\n' 'build-configure.sh: OMP_VERSION=latest is unsupported; supply an exact version' >&2
  exit 1
fi
IMAGE="${OMP_SBX_CONFIGURE_IMAGE:-omp-sbx-configure:latest}"
ARCHIVE="$(mktemp "${TMPDIR:-/tmp}/omp-sbx-configure.XXXXXX")"
cleanup() { rm -f "$ARCHIVE"; }
trap cleanup EXIT

printf '>> building %s (omp v%s)\n' "$IMAGE" "$OMP_VERSION" >&2
docker build \
  --build-arg "OMP_VERSION=${OMP_VERSION}" \
  -t "$IMAGE" \
  -f "$DIR/sbx-configure-kit/Dockerfile" \
  "$DIR/sbx-configure-kit"

docker image save "$IMAGE" -o "$ARCHIVE"
sbx template load "$ARCHIVE"
printf '>> loaded local configure template %s\n' "$IMAGE" >&2
