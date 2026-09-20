#!/usr/bin/env bash
# Build and load the interactive and configure-only images into the sbx runtime.
#
# OMP_VERSION picks the omp release to bake in: unset uses the pin below,
# X.Y.Z pins explicitly, and "latest" resolves the newest GitHub release.
# "latest" also refuses a release younger than RELEASE_COOLDOWN_DAYS unless
# you confirm, which buys time for a compromised upstream to be caught:
#
#   RELEASE_COOLDOWN_DAYS=0 OMP_VERSION=latest ./build.sh
set -euo pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RELEASE_COOLDOWN_DAYS="${RELEASE_COOLDOWN_DAYS:-3}"
OMP_VERSION_PINNED="18.1.10"  # image baseline; omp self-updates at runtime

# Colors (only when stderr is a tty)
if [ -t 2 ]; then
  C_DIM=$'\033[2m'; C_CYAN=$'\033[36m'; C_YELLOW=$'\033[33m'; C_RED=$'\033[31m'; C_GREEN=$'\033[32m'; C_BOLD=$'\033[1m'; C_RST=$'\033[0m'
else
  C_DIM=''; C_CYAN=''; C_YELLOW=''; C_RED=''; C_GREEN=''; C_BOLD=''; C_RST=''
fi

# ── Preflight ───────────────────────────────────────────────────────────────
# shellcheck source=sbx-preflight.sh
source "$DIR/sbx-preflight.sh"
ensure_sbx "build.sh"

if [ -z "${OMP_VERSION:-}" ]; then
  OMP_VERSION="$OMP_VERSION_PINNED"
  echo ">> using pinned omp v${OMP_VERSION} (set OMP_VERSION=latest to fetch the newest release)" >&2
elif [ "$OMP_VERSION" = "latest" ]; then
  echo ">> fetching latest omp release tag" >&2

  RELEASE_JSON="$(curl -fsSL https://api.github.com/repos/can1357/oh-my-pi/releases/latest)"
  OMP_VERSION="$(printf '%s\n' "$RELEASE_JSON" | sed -n 's/.*"tag_name": *"v\([^"]*\)".*/\1/p' | head -1)"
  PUBLISHED_AT="$(printf '%s\n' "$RELEASE_JSON" | sed -n 's/.*"published_at": *"\([^"]*\)".*/\1/p' | head -1)"

  OMP_VERSION="${OMP_VERSION:?could not determine OMP_VERSION}"

  # An unparseable date skips the cooldown rather than blocking the build.
  if [ "${RELEASE_COOLDOWN_DAYS}" -gt 0 ] && [ -n "$PUBLISHED_AT" ]; then
    # BSD date first, then GNU date - this runs on macOS and Linux.
    RELEASE_TS="$(date -u -j -f "%Y-%m-%dT%H:%M:%SZ" "$PUBLISHED_AT" +%s 2>/dev/null || \
                  date -u -d "$PUBLISHED_AT" +%s 2>/dev/null || echo "")"
    NOW_TS="$(date -u +%s)"

    if [ -n "$RELEASE_TS" ]; then
      AGE_DAYS=$(( (NOW_TS - RELEASE_TS) / 86400 ))
      AGE_HOURS=$(( (NOW_TS - RELEASE_TS) / 3600 ))

      if [ "$AGE_DAYS" -lt "$RELEASE_COOLDOWN_DAYS" ]; then
        echo "" >&2
        echo "${C_YELLOW}⚠ WARNING: omp v${OMP_VERSION} was published ${AGE_HOURS}h ago${C_RST}" >&2
        echo "${C_YELLOW}  (released: ${PUBLISHED_AT}, cooldown: ${RELEASE_COOLDOWN_DAYS} days)${C_RST}" >&2
        echo "${C_DIM}  This release is very new and may not have been vetted by the community yet.${C_RST}" >&2
        echo "${C_DIM}  If this is a compromised upstream, running it could execute malicious code.${C_RST}" >&2
        echo "" >&2

        if [ -t 0 ]; then
          printf '%sProceed anyway? [y/N] %s' "${C_YELLOW}" "${C_RST}" >&2
          read -r answer < /dev/tty
          case "$answer" in
            y|Y|yes|YES)
              echo "${C_DIM}proceeding with v${OMP_VERSION}${C_RST}" >&2
              ;;
            *)
              echo "${C_RED}aborted. Pin a specific version with: OMP_VERSION=18.1.10 $0${C_RST}" >&2
              exit 1
              ;;
          esac
        else
          echo "${C_RED}non-interactive shell; aborting. Set RELEASE_COOLDOWN_DAYS=0 to skip this check${C_RST}" >&2
          exit 1
        fi
      else
        echo "${C_DIM}>> omp v${OMP_VERSION} released ${AGE_DAYS}d ago (within cooldown)${C_RST}" >&2
      fi
    fi
  fi
fi

OMP_VERSION="${OMP_VERSION:?could not determine OMP_VERSION}"
IMAGE="${OMP_SBX_IMAGE:-omp-sbx:latest}"
CONFIGURE_IMAGE="${OMP_SBX_CONFIGURE_IMAGE:-omp-sbx-configure:latest}"

echo ">> building ${IMAGE} (omp v${OMP_VERSION})"
docker build \
  --build-arg "OMP_VERSION=${OMP_VERSION}" \
  -t "${IMAGE}" \
  -f "${DIR}/sbx-kit/Dockerfile" \
  "${DIR}"

echo ">> building ${CONFIGURE_IMAGE} (omp v${OMP_VERSION})"
docker build \
  --build-arg "OMP_VERSION=${OMP_VERSION}" \
  -t "${CONFIGURE_IMAGE}" \
  -f "${DIR}/sbx-configure-kit/Dockerfile" \
  "${DIR}/sbx-configure-kit"

echo ">> saving + loading into sbx runtime"
MAIN_ARCHIVE=/tmp/omp-sbx.tar
CONFIGURE_ARCHIVE=/tmp/omp-sbx-configure.tar
docker image save "${IMAGE}" -o "$MAIN_ARCHIVE"
docker image save "${CONFIGURE_IMAGE}" -o "$CONFIGURE_ARCHIVE"
if ! sbx template load "$MAIN_ARCHIVE" ||
   ! sbx template load "$CONFIGURE_ARCHIVE"
then
  if [ -t 2 ]; then C_BRED=$'\033[1;31m'; C_RST=$'\033[0m'; else C_BRED=''; C_RST=''; fi
  echo "" >&2
  echo "${C_BRED}ERROR: sbx template load failed.${C_RST}" >&2
  echo "${C_BRED}If the error mentions '401 Unauthorized' or 'no valid user session',${C_RST}" >&2
  echo "${C_BRED}you are not authenticated to Docker/sbx. Run:${C_RST}" >&2
  echo "" >&2
  echo "  sbx login" >&2
  echo "" >&2
  echo "${C_BRED}then re-run ./build.sh${C_RST}" >&2
  rm -f "$MAIN_ARCHIVE" "$CONFIGURE_ARCHIVE"
  exit 1
fi
rm -f "$MAIN_ARCHIVE" "$CONFIGURE_ARCHIVE"

echo ">> verifying"
VERIFY_NAME="omp-verify"
VERIFY_DENY_NAME="omp-verify-deny"
VERIFY_CONFIGURE_NAME="omp-verify-configure"
POLICY_DIR="$(mktemp -d)"
DENY_POLICY_DIR="$(mktemp -d)"
# Generated, not copied from the repo: verification must not depend on an
# optional project policy file that may be absent or edited.
printf 'schemaVersion: 1\nallowedImages:\n  - docker.io/docker/sandbox-templates:shell-docker-nightly\n' > "$POLICY_DIR/.omp-sbx-docker-images.yaml"
printf 'schemaVersion: 1\nallowedImages: []\n' > "$DENY_POLICY_DIR/.omp-sbx-docker-images.yaml"
chmod 0444 "$POLICY_DIR/.omp-sbx-docker-images.yaml" "$DENY_POLICY_DIR/.omp-sbx-docker-images.yaml"
cleanup_verify() {
  sbx rm -f "$VERIFY_NAME" >/dev/null 2>&1 || true
  sbx rm -f "$VERIFY_DENY_NAME" >/dev/null 2>&1 || true
  sbx rm -f "$VERIFY_CONFIGURE_NAME" >/dev/null 2>&1 || true
  rm -rf "$POLICY_DIR" "$DENY_POLICY_DIR"
}
trap cleanup_verify EXIT
cleanup_sandboxes() {
  sbx rm -f "$VERIFY_NAME" >/dev/null 2>&1 || true
  sbx rm -f "$VERIFY_DENY_NAME" >/dev/null 2>&1 || true
  sbx rm -f "$VERIFY_CONFIGURE_NAME" >/dev/null 2>&1 || true
}
cleanup_sandboxes
sbx create -q --template "${CONFIGURE_IMAGE}" --name "$VERIFY_CONFIGURE_NAME" \
  "${DIR}/sbx-configure-kit" /tmp
sleep 2
sbx exec -w /home/agent "$VERIFY_CONFIGURE_NAME" omp --version
sbx exec -w /home/agent "$VERIFY_CONFIGURE_NAME" sh -c 'test ! -S /var/run/docker.sock'
sbx rm -f "$VERIFY_CONFIGURE_NAME" >/dev/null 2>&1

sbx create -q --template "${IMAGE}" --name "$VERIFY_NAME" \
  "${DIR}/sbx-kit" /tmp "${POLICY_DIR}:ro"
sleep 2
sbx exec -w /home/agent "$VERIFY_NAME" sh -lc 'for _ in $(seq 1 60); do if test -S /var/run/docker.sock && /usr/local/libexec/docker-real version >/dev/null 2>&1 && /usr/local/libexec/docker-real ps >/dev/null 2>&1; then exit 0; fi; sleep 0.5; done; echo "Docker daemon did not become ready inside sandbox" >&2; exit 1'
sbx exec -w /home/agent "$VERIFY_NAME" omp-init.sh --version
sbx exec -w /home/agent "$VERIFY_NAME" sh -lc 'docker info >/dev/null'
sbx exec -w /home/agent "$VERIFY_NAME" sh -lc 'output=$(docker pull alpine:latest 2>&1) && exit 1; printf "%s\n" "$output" | grep -q "not approved by omp-sbx policy"'
sbx exec -w /home/agent "$VERIFY_NAME" sh -lc 'python3 -c '"'"'import json; data=json.load(open("/var/lib/omp-sbx-policy/policy.json")); assert data["images"] == ["docker.io/docker/sandbox-templates:shell-docker-nightly"]'"'"''
sbx exec -w /home/agent "$VERIFY_NAME" sh -lc "grep -F 'host $POLICY_DIR virtiofs ro,' /proc/mounts >/dev/null; if printf 'tamper\n' > '$POLICY_DIR/.omp-sbx-docker-images.yaml' 2>/dev/null; then echo 'read-only policy was writable' >&2; exit 1; fi"
# sudo is passwordless here, so root is the case that decides whether the host
# really owns the policy file.
sbx exec -w /home/agent "$VERIFY_NAME" sh -lc "if sudo cp /etc/hostname '$POLICY_DIR/.omp-sbx-docker-images.yaml' 2>/dev/null; then echo 'read-only policy was writable by root' >&2; exit 1; fi; sudo test -w '$POLICY_DIR/.omp-sbx-docker-images.yaml' && { echo 'read-only policy reports writable' >&2; exit 1; }; exit 0"
sbx exec -w /home/agent "$VERIFY_NAME" sh -lc 'd=$(mktemp -d); printf "schemaVersion: 1\nallowedImages: []\n" >"$d/empty.yaml"; /usr/local/libexec/docker-image-policy.py compile --input "$d/empty.yaml" --output-dir "$d/empty"; python3 -c '"'"'import json,sys; assert json.load(open(sys.argv[1]))["images"] == []'"'"' "$d/empty/policy.json"; printf "schemaVersion: 1\nallowedImages: []\nextra: true\n" >"$d/bad.yaml"; ! /usr/local/libexec/docker-image-policy.py compile --input "$d/bad.yaml" --output-dir "$d/bad"; printf "schemaVersion: 1\nallowedImages:\n  - alpine\n  - docker.io/library/alpine:latest\n" >"$d/duplicate.yaml"; ! /usr/local/libexec/docker-image-policy.py compile --input "$d/duplicate.yaml" --output-dir "$d/duplicate"'
sbx exec -w /home/agent "$VERIFY_NAME" sh -lc 'd=$(mktemp -d); printf "package main\nimport \"fmt\"\nfunc main(){fmt.Println(\"omp-docker-policy-smoke\")}\n" >"$d/main.go"; (cd "$d" && CGO_ENABLED=0 GO111MODULE=off go build -o hello main.go); printf "FROM scratch\nCOPY hello /hello\nENTRYPOINT [\"/hello\"]\n" >"$d/Dockerfile"; docker build -t policy-smoke "$d"; test "$(docker run --rm policy-smoke)" = omp-docker-policy-smoke; output=$(/usr/local/libexec/docker-real build -t raw-build "$d" 2>&1) && exit 1; printf "%s\n" "$output" | grep -q "requires the omp-sbx Build Policy wrapper"; printf "FROM alpine:latest\n" >"$d/Dockerfile"; output=$(docker build -t denied-build "$d" 2>&1) && exit 1; printf "%s\n" "$output" | grep -q "not approved by omp-sbx policy"; output=$(docker load </dev/null 2>&1) && exit 1; printf "%s\n" "$output" | grep -q "image load is not allowed"'
sbx exec -w /home/agent "$VERIFY_NAME" sh -lc 'test "$PUPPETEER_EXECUTABLE_PATH" = /usr/local/bin/chromium && test -x "$PUPPETEER_EXECUTABLE_PATH" && "$PUPPETEER_EXECUTABLE_PATH" --headless --no-sandbox --disable-gpu --disable-dev-shm-usage --dump-dom "data:text/html,<p>omp-browser-smoke</p>" 2>/dev/null | grep -q "omp-browser-smoke"'

sbx create -q --template "${IMAGE}" --name "$VERIFY_DENY_NAME" \
  "${DIR}/sbx-kit" /tmp "${DENY_POLICY_DIR}:ro"
sleep 2
sbx exec -w /home/agent "$VERIFY_DENY_NAME" sh -lc 'for _ in $(seq 1 60); do if test -S /var/run/docker.sock && /usr/local/libexec/docker-real version >/dev/null 2>&1; then exit 0; fi; sleep 0.5; done; exit 1'
sbx exec -w /home/agent "$VERIFY_DENY_NAME" omp-init.sh --version
sbx exec -w /home/agent "$VERIFY_DENY_NAME" sh -lc 'python3 -c '"'"'import json; assert json.load(open("/var/lib/omp-sbx-policy/policy.json"))["images"] == []'"'"'; output=$(docker pull alpine:latest 2>&1) && exit 1; printf "%s\n" "$output" | grep -q "not approved by omp-sbx policy"; d=$(mktemp -d); printf "package main\nimport \"fmt\"\nfunc main(){fmt.Println(\"deny-all-local-smoke\")}\n" >"$d/main.go"; (cd "$d" && CGO_ENABLED=0 GO111MODULE=off go build -o hello main.go); printf "FROM scratch\nCOPY hello /hello\nENTRYPOINT [\"/hello\"]\n" >"$d/Dockerfile"; docker build -t deny-all-local "$d"; test "$(docker run --rm deny-all-local)" = deny-all-local-smoke'
cleanup_verify
trap - EXIT
echo "✓ done"
