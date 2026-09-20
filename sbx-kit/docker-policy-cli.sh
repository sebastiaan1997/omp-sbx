#!/usr/bin/env bash
set -euo pipefail

REAL_DOCKER=/usr/local/libexec/docker-real
POLICY_FILE=/var/lib/omp-sbx-policy/build-policy.rego
POLICY_ID_FILE=/var/lib/omp-sbx-policy/policy-id

ARGS=("$@")
GLOBAL=()
index=0
while [ "$index" -lt "${#ARGS[@]}" ]; do
  arg="${ARGS[$index]}"
  case "$arg" in
    --config|--context|--host|-H|--log-level|-l)
      GLOBAL+=("$arg")
      index=$((index + 1))
      if [ "$index" -ge "${#ARGS[@]}" ]; then
        exec "$REAL_DOCKER" "$@"
      fi
      GLOBAL+=("${ARGS[$index]}")
      index=$((index + 1))
      ;;
    --config=*|--context=*|--host=*|--log-level=*|--debug|-D|--tls|--tlsverify)
      GLOBAL+=("$arg")
      index=$((index + 1))
      ;;
    -*) exec "$REAL_DOCKER" "$@" ;;
    *) break ;;
  esac
done

if [ "$index" -ge "${#ARGS[@]}" ]; then
  exec "$REAL_DOCKER" "$@"
fi
COMMAND="${ARGS[$index]}"
index=$((index + 1))
REST=("${ARGS[@]:$index}")

reject_policy_override() {
  local arg
  for arg in "$@"; do
    case "$arg" in
      --policy|--policy=*)
        echo "omp-sbx: --policy is managed by .omp-sbx-docker-images.yaml" >&2
        exit 2
        ;;
    esac
  done
}

policy_headers() {
  local policy_id existing pair name value merged
  if [ ! -r "$POLICY_ID_FILE" ] || [ ! -r "$POLICY_FILE" ]; then
    echo "omp-sbx: Docker image policy is not initialized" >&2
    exit 1
  fi
  policy_id="$(cat "$POLICY_ID_FILE")"
  existing="${DOCKER_CUSTOM_HEADERS:-}"
  merged="$existing"
  IFS=',' read -r -a pairs <<< "$existing"
  for pair in "${pairs[@]}"; do
    name="${pair%%=*}"
    value="${pair#*=}"
    if [ "${name,,}" = "x-omp-sbx-policy" ]; then
      if [ "$value" != "$policy_id" ]; then
        echo "omp-sbx: conflicting X-OMP-SBX-Policy header" >&2
        exit 2
      fi
      printf '%s' "$merged"
      return
    fi
  done
  if [ -n "$merged" ]; then
    merged+=","
  fi
  merged+="X-OMP-SBX-Policy=$policy_id"
  printf '%s' "$merged"
}

run_policy_build() {
  local headers
  headers="$(policy_headers)"
  exec env DOCKER_CUSTOM_HEADERS="$headers" "$REAL_DOCKER" "${GLOBAL[@]}" "$@"
}

case "$COMMAND" in
  build)
    reject_policy_override "${REST[@]}"
    run_policy_build buildx build --load \
      --policy "reset=true,filename=$POLICY_FILE,strict=true" "${REST[@]}"
    ;;
  buildx)
    if [ "${#REST[@]}" -gt 0 ] && { [ "${REST[0]}" = build ] || [ "${REST[0]}" = bake ]; }; then
      reject_policy_override "${REST[@]:1}"
      run_policy_build buildx "${REST[0]}" \
        --policy "reset=true,filename=$POLICY_FILE,strict=true" "${REST[@]:1}"
    fi
    exec "$REAL_DOCKER" "${GLOBAL[@]}" buildx "${REST[@]}"
    ;;
  compose)
    for arg in "${REST[@]}"; do
      if [ "$arg" = "--build" ]; then
        echo "omp-sbx: Compose builds require 'docker buildx bake -f <compose-file>'; Docker Compose cannot require the sandbox build policy" >&2
        exit 2
      fi
    done
    compose_command=""
    compose_index=0
    while [ "$compose_index" -lt "${#REST[@]}" ]; do
      arg="${REST[$compose_index]}"
      case "$arg" in
        -f|--file|--profile|--project-name|-p|--project-directory|--env-file|--parallel)
          compose_index=$((compose_index + 2)) ;;
        --file=*|--profile=*|--project-name=*|--project-directory=*|--env-file=*|--parallel=*|--ansi=*|--progress=*|--compatibility|--dry-run)
          compose_index=$((compose_index + 1)) ;;
        -*) compose_index=$((compose_index + 1)) ;;
        *) compose_command="$arg"; break ;;
      esac
    done
    if [ "$compose_command" = build ]; then
      echo "omp-sbx: Compose builds require 'docker buildx bake -f <compose-file>'; Docker Compose cannot require the sandbox build policy" >&2
      exit 2
    fi
    exec "$REAL_DOCKER" "${GLOBAL[@]}" compose "${REST[@]}"
    ;;
  *) exec "$REAL_DOCKER" "$@" ;;
esac
