# syntax=docker/dockerfile:1.7
###############################################################################
# omp-sbx-template - sandbox template for the omp coding agent.
#
# The base image (docker/sandbox-templates:shell-docker-nightly) supplies Ubuntu,
# the non-root `agent` user, a bootstrap Node, Python, Go, Java, git, a private
# in-sandbox Docker Engine, and the sbx microVM security model. This adds
# nvm-managed Node majors, the omp binary, bun, uv, LSP servers, Chromium,
# and the dev tools the shell template leaves out.
#
# Build and load with ./build.sh.
###############################################################################

FROM docker/sandbox-templates:shell-docker-nightly

USER root
ARG OMP_VERSION=18.1.21
ARG COREPACK_VERSION=0.34.5
ARG NVM_VERSION=0.40.3
ARG PNPM_VERSION=12.4.1
ARG PLAYWRIGHT_VERSION=1.63.0
ARG KOTLIN_LSP_VERSION=263.4702.0
ARG ERLANG_LS_REV=ed1daaa4d94d2dc9c9d3f934c704ee4dafb176c6
ARG SWIFT_VERSION=6.2


# ── Dev tools and Chromium's shared libraries ────────────────────────────────
RUN apt-get update && apt-get install -y --no-install-recommends \
      binutils build-essential clang clangd curl elixir erlang wget \
      fd-find file fzf gawk gnupg2 less libffi-dev libgmp-dev llvm-dev openjdk-21-jre-headless \
      php-cli php-mbstring php-xml rebar3 \
      libc6-dev libcurl4-openssl-dev libedit2 libgcc-13-dev libncurses-dev \
      libpython3-dev libsqlite3-0 libssl-dev libstdc++-13-dev libxml2-dev \
      libz3-dev pkg-config procps python3-yaml ruby-full sqlite3 time tzdata unzip uuid-dev \
      xz-utils zlib1g-dev \
      libglib2.0-0t64 libatk1.0-0t64 libatk-bridge2.0-0t64 libcups2t64 \
      libxcb1 libxkbcommon0 libasound2t64 libgbm1 libx11-6 libxext6 \
      libcairo2 libpango-1.0-0 libxcomposite1 libxdamage1 libxfixes3 \
      libxrandr2 libatspi2.0-0t64 libnss3 libnspr4 libdrm2 \
    && update-alternatives --set java "/usr/lib/jvm/java-21-openjdk-$(dpkg --print-architecture)/bin/java" \
    && java -version 2>&1 | grep -q 'version "21\.' \
    && rm -rf /var/lib/apt/lists/* \
    && ln -sf /usr/bin/fdfind /usr/local/bin/fd


USER agent

# ── Bun ──────────────────────────────────────────────────────────────────────
RUN curl -fsSL https://bun.sh/install | bash -s "bun-v1.3.14"
ENV BUN_INSTALL=/home/agent/.bun \
    NVM_DIR=/home/agent/.nvm \
    PNPM_HOME=/home/agent/.local/bin \
    NPM_CONFIG_PREFIX= \
    npm_config_prefix= \
    GEM_HOME=/home/agent/.local/share/gem \
    GEM_PATH=/home/agent/.local/share/gem \
    ODIN_ROOT=/home/agent/.local/share/odin \
    PATH="/home/agent/.nvm/versions/node/default/bin:/home/agent/.local/bin:/home/agent/.cargo/bin:/home/agent/.ghcup/bin:/home/agent/.local/share/gem/bin:/home/agent/.local/share/swift/usr/bin:/home/agent/go/bin:/home/agent/.bun/bin:${PATH}"

# ── uv (Python package manager for the pi kernel) ────────────────────────────
RUN curl -LsSf https://astral.sh/uv/install.sh | sh

# ── Node via nvm + pnpm ──────────────────────────────────────────────────────
# Install the supported Node majors into the image. Node 24 is the default
# runtime; projects can still `nvm use 20`, `22`, `24`, or `26` at runtime.
RUN set -eu; \
    unset NPM_CONFIG_PREFIX npm_config_prefix; \
    curl -fsSL "https://raw.githubusercontent.com/nvm-sh/nvm/v${NVM_VERSION}/install.sh" | bash; \
    set +u; \
    . "${NVM_DIR}/nvm.sh"; \
    set -u; \
    mkdir -p "${PNPM_HOME}"; \
    for version in 20 22 24 26; do \
         nvm install "${version}"; \
         nvm use "${version}"; \
         npm install --global "corepack@${COREPACK_VERSION}"; \
         corepack enable --install-directory "${PNPM_HOME}" pnpm; \
         corepack prepare "pnpm@${PNPM_VERSION}" --activate; \
         node --version; \
         pnpm --version; \
       done; \
    nvm alias default 24; \
    node_dir="${NVM_DIR}/versions/node/$(nvm version 24)"; \
    ln -sfn "${node_dir}" "${NVM_DIR}/versions/node/default"; \
    nvm use 24; \
    node --version; \
    pnpm --version
USER root
RUN touch /etc/sandbox-persistent.sh \
    && if ! grep -qxF 'unset NPM_CONFIG_PREFIX npm_config_prefix' /etc/sandbox-persistent.sh; then \
         printf '%s\n' \
           'unset NPM_CONFIG_PREFIX npm_config_prefix' \
           'export NVM_DIR="/home/agent/.nvm"' \
           '[ -s "$NVM_DIR/nvm.sh" ] && \. "$NVM_DIR/nvm.sh"' \
           >> /etc/sandbox-persistent.sh; \
       fi
USER agent

# ── LSP servers ──────────────────────────────────────────────────────────────
RUN unset NPM_CONFIG_PREFIX npm_config_prefix \
    && npm install --global \
      pyright@1.1.414 \
      typescript-language-server@6.0.0 \
      typescript@7.0.2 \
      bash-language-server@5.6.0 \
      @astrojs/language-server@2.16.16 \
      yaml-language-server@1.24.0 \
      @t1ckbase/vscode-langservers-extracted@2.0.4 \
      @biomejs/biome@2.5.13 \
      @tailwindcss/language-server@0.16.0 \
      svelte-language-server@0.18.4 \
      @vue/language-server@3.3.11 \
      intelephense@1.18.5 \
      dockerfile-language-server-nodejs@0.15.0 \
      graphql-language-service-cli@3.5.0 \
      @prisma/language-server@31.12.8 \
      vim-language-server@2.3.1 \
      @olrtg/emmet-language-server@2.8.0
RUN go install golang.org/x/tools/gopls@v0.23.0
RUN uv tool install --no-cache basedpyright==1.40.1 \
    && uv tool install --no-cache python-lsp-server==1.15.0 \
    && uv tool install --no-cache ty==0.0.80 \
    && uv tool install --no-cache ruff==0.16.7
RUN gem install --no-document \
      solargraph:0.60.4 \
      ruby-lsp:0.26.11 \
      rubocop:1.91.0


# ── Upstream-only LSP distributions ─────────────────────────────────────────
RUN set -eu; \
    arch="$(uname -m)"; \
    case "$arch" in \
      x86_64|amd64) kotlin_suffix="" ;; \
      aarch64|arm64) kotlin_suffix="-aarch64" ;; \
      *) echo "unsupported arch: $arch" >&2; exit 1 ;; \
    esac; \
    archive="kotlin-server-${KOTLIN_LSP_VERSION}${kotlin_suffix}.tar.gz"; \
    url="https://download.jetbrains.com/language-server/kotlin-server/${KOTLIN_LSP_VERSION}/${archive}"; \
    tmpdir="$(mktemp -d)"; \
    curl -fsSL "$url" -o "${tmpdir}/${archive}"; \
    curl -fsSL "${url}.sha256" -o "${tmpdir}/${archive}.sha256"; \
    checksum="$(cut -d ' ' -f1 "${tmpdir}/${archive}.sha256")"; \
    test "${#checksum}" -eq 64; \
    cd "$tmpdir"; \
    printf '%s  %s\n' "$checksum" "$archive" | sha256sum -c -; \
    install -d /home/agent/.local/share/kotlin-lsp; \
    tar -xzf "$archive" -C /home/agent/.local/share/kotlin-lsp --strip-components=1; \
    chmod +x /home/agent/.local/share/kotlin-lsp/kotlin-lsp.sh; \
    ln -s /home/agent/.local/share/kotlin-lsp/kotlin-lsp.sh /home/agent/.local/bin/kotlin-lsp; \
    cd /; \
    rm -rf "$tmpdir"
RUN set -eu; \
    tmpdir="$(mktemp -d)"; \
    curl -fsSL "https://github.com/erlang-ls/erlang_ls/archive/${ERLANG_LS_REV}.tar.gz" \
      -o "${tmpdir}/erlang_ls.tar.gz"; \
    tar -xzf "${tmpdir}/erlang_ls.tar.gz" -C "$tmpdir" --strip-components=1; \
    make -C "$tmpdir"; \
    make -C "$tmpdir" install PREFIX=/home/agent/.local; \
    rm -rf "$tmpdir"

# ── Chromium (Playwright build) ───────────────────────────────────────────────
# OMP's downloaded Chrome-for-Testing build can be incompatible with the sbx
# microVM architecture. Playwright supplies native Linux x64 and ARM64 builds;
# point OMP's Puppeteer runtime at that known executable.
RUN unset NPM_CONFIG_PREFIX npm_config_prefix \
    && npx "playwright@${PLAYWRIGHT_VERSION}" install --with-deps chromium
# Resolve the architecture-specific Playwright path (for example chrome-linux64)
# and fail the build if the install did not create an executable.
USER root
RUN set -eu; \
    set -- /home/agent/.cache/ms-playwright/chromium-*/chrome-linux*/chrome; \
    test -x "$1"; \
    ln -sf "$1" /usr/local/bin/chromium; \
    /usr/local/bin/chromium --version
USER agent
ENV PUPPETEER_EXECUTABLE_PATH=/usr/local/bin/chromium

# ── omp prebuilt binary ──────────────────────────────────────────────────────
# The `omp --version` smoke test creates /home/agent/.omp on the way out, so
# delete it: omp-init.sh replaces that path with a symlink to the host mount,
# and a real directory there gets the symlink nested inside it instead.
RUN ARCH="$(uname -m)" && \
    case "$ARCH" in \
      aarch64|arm64) OMP_ARCH="arm64" ;; \
      x86_64|amd64)  OMP_ARCH="x64"   ;; \
      *) echo "unsupported arch: $ARCH" >&2; exit 1 ;; \
    esac && \
    curl -fsSL "https://github.com/can1357/oh-my-pi/releases/download/v${OMP_VERSION}/omp-linux-${OMP_ARCH}" \
      -o /home/agent/.local/bin/omp && \
    chmod +x /home/agent/.local/bin/omp && \
    omp --version && \
    rm -rf /home/agent/.omp

# ── AWS CLI v2 (Amazon Bedrock provider) ─────────────────────────────────────
# Two jobs: the browserless SSO device-code login, and `credential_process`.
# omp reads the SSO access token but not the refresh token beside it, so it
# cannot renew an expired session; the CLI can, and omp calls out to it.
USER root
RUN ARCH="$(uname -m)" && \
    case "$ARCH" in \
      aarch64|arm64) AWS_ARCH="aarch64" ;; \
      x86_64|amd64)  AWS_ARCH="x86_64"  ;; \
      *) echo "unsupported arch: $ARCH" >&2; exit 1 ;; \
    esac && \
    curl -fsSL "https://awscli.amazonaws.com/awscli-exe-linux-${AWS_ARCH}.zip" \
      -o /tmp/awscli.zip && \
    unzip -q /tmp/awscli.zip -d /tmp && \
    /tmp/aws/install && \
    rm -rf /tmp/awscli.zip /tmp/aws && \
    aws --version
USER agent

# ── Docker image authorization ───────────────────────────────────────────────
USER root
RUN mkdir -p /usr/local/libexec /etc/docker/plugins \
    && cp "$(command -v docker)" /usr/local/libexec/docker-real \
    && docker buildx version \
      | grep -Eq 'v?(0\.(3[1-9]|[4-9][0-9])|[1-9][0-9]*\.)'
COPY sbx-kit/docker-image-authz.go /tmp/docker-image-authz.go
RUN CGO_ENABLED=0 GO111MODULE=off go build -trimpath -ldflags="-s -w" \
      -o /usr/local/libexec/docker-image-authz /tmp/docker-image-authz.go \
    && rm /tmp/docker-image-authz.go \
    && printf '%s\n' 'unix:///run/omp-sbx/image-authz.sock' \
      > /etc/docker/plugins/omp-sbx-image-policy.spec
COPY sbx-kit/docker-image-policy.py /usr/local/libexec/docker-image-policy.py
COPY sbx-kit/docker-policy-start.sh /usr/local/libexec/docker-policy-start.sh
COPY sbx-kit/docker-policy-cli.sh /usr/local/bin/docker
RUN chmod 0755 \
      /usr/local/libexec/docker-image-authz \
      /usr/local/libexec/docker-image-policy.py \
      /usr/local/libexec/docker-policy-start.sh \
      /usr/local/libexec/docker-real \
      /usr/local/bin/docker
USER agent

# ── Entrypoint ───────────────────────────────────────────────────────────────
USER root
COPY sbx-kit/omp-init.sh /usr/local/bin/omp-init.sh
RUN chmod +x /usr/local/bin/omp-init.sh
# omp-init.sh loads these only for a project that opts into Bedrock.
COPY sbx-kit/extensions/ /opt/omp-sbx/extensions/
USER agent

# ── Environment ──────────────────────────────────────────────────────────────
ENV PI_CONFIG_DIR=.omp \
    HOME=/home/agent

# Not /home/agent/workspace: the startup hook swaps that path for a symlink,
# and OCI execs against a moving WORKDIR fail.
WORKDIR /home/agent

# Tells sbx this template is built for the "omp" agent, not the base shell one.
LABEL com.docker.sandboxes.flavor="omp"
