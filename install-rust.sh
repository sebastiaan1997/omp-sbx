#!/usr/bin/env bash
# Build the Rust host launcher and install it without replacing the legacy shell launcher.
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
PROFILE=release
DEST_DIR="${OMP_SBX_BIN_DIR:-${HOME:?HOME is not set}/.local/bin}"
BINARY_NAME="${OMP_SBX_BINARY_NAME:-omp-sbx}"

usage() {
  printf 'Usage: %s [--debug] [--dest DIR] [--name NAME]\n' "${0##*/}" >&2
}
while (($#)); do
  case "$1" in
    --debug) PROFILE=debug; shift ;;
    --dest) [[ $# -ge 2 ]] || { usage; exit 2; }; DEST_DIR=$2; shift 2 ;;
    --name) [[ $# -ge 2 ]] || { usage; exit 2; }; BINARY_NAME=$2; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) printf 'unknown argument: %s\n' "$1" >&2; usage; exit 2 ;;
  esac
done

cd "$SCRIPT_DIR"
if [[ "$PROFILE" == release ]]; then
  cargo build --workspace --package omp-sbx --release
  BUILT="target/release/omp-sbx"
else
  cargo build --workspace --package omp-sbx
  BUILT="target/debug/omp-sbx"
fi
[[ -x "$BUILT" ]] || { printf 'build did not produce executable: %s\n' "$BUILT" >&2; exit 1; }

mkdir -p "$DEST_DIR"
TEMP="$DEST_DIR/.${BINARY_NAME}.$$"
cleanup() { rm -f -- "$TEMP"; }
trap cleanup EXIT
install -m 0755 "$BUILT" "$TEMP"
mv -f -- "$TEMP" "$DEST_DIR/$BINARY_NAME"
printf 'installed %s\n' "$DEST_DIR/$BINARY_NAME"

