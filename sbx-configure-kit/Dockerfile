# syntax=docker/dockerfile:1.7

FROM ubuntu:24.04 AS omp-downloader

ARG OMP_VERSION=18.1.10

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

RUN set -eu; \
    case "$(dpkg --print-architecture)" in \
      amd64) omp_arch=x64 ;; \
      arm64) omp_arch=arm64 ;; \
      *) echo "unsupported architecture: $(dpkg --print-architecture)" >&2; exit 1 ;; \
    esac; \
    install -d /out; \
    curl -fsSL \
      "https://github.com/can1357/oh-my-pi/releases/download/v${OMP_VERSION}/omp-linux-${omp_arch}" \
      -o /out/omp; \
    chmod 0755 /out/omp; \
    HOME=/tmp /out/omp --version; \
    rm -rf /tmp/.omp

FROM docker/sandbox-templates:shell AS runtime

USER root

COPY --from=omp-downloader --chmod=0755 /out/omp /usr/local/bin/omp
COPY --chmod=0755 omp-configure-init.sh /usr/local/bin/omp-configure-init

ENV HOME=/home/agent \
    PI_CONFIG_DIR=.omp

USER agent
WORKDIR /home/agent
ENTRYPOINT ["/usr/local/bin/omp-configure-init"]

LABEL com.docker.sandboxes.flavor="omp-configure"
