#!/usr/bin/env bash
set -euo pipefail

readonly BIN_DIR=/home/agent/.local/bin
readonly SHARE_DIR=/home/agent/.local/share
readonly TMP_ROOT="$(mktemp -d)"
trap 'rm -rf "$TMP_ROOT"' EXIT

readonly RUST_VERSION=1.89.0
readonly RUSTUP_VERSION=1.29.1
readonly ZLS_VERSION=0.16.0
readonly DENO_VERSION=2.7.7
readonly EXPERT_VERSION=0.1.0
readonly GLEAM_VERSION=1.15.1
readonly LUA_LS_VERSION=3.17.1
readonly DART_VERSION=3.13.3
readonly MARKSMAN_VERSION=2026-02-08
readonly TEXLAB_VERSION=5.24.0
readonly ODIN_VERSION=dev-2026-07a
readonly ODIN_COMMIT=819fdc7a80667498b8b365999f1475a66c358640
readonly OLS_VERSION=dev-2026-06
readonly OLS_COMMIT=ca8eb6da44c2b1c9e63736af05a5c3a5a298ea82
readonly JDTLS_VERSION=1.61.0-202609031315
readonly COURSIER_VERSION=2.1.24
readonly METALS_VERSION=1.6.8
readonly GHCUP_VERSION=0.2.6.2
readonly GHC_VERSION=9.6.7
readonly CABAL_VERSION=3.12.1.0
readonly HLS_VERSION=2.13.0.0
readonly ELIXIR_LS_VERSION=0.29.3
readonly PHPACTOR_VERSION=2026.07.22.0
readonly OMNISHARP_VERSION=1.39.15
readonly TERRAFORM_LS_VERSION=0.38.5
readonly HELM_LS_VERSION=0.5.0
readonly SWIFTLINT_VERSION=0.65.1
readonly SWIFTLINT_COMMIT=6aba03e3d8302b33f106e0f922210f35ca4b52cf

case "$(uname -m)" in
  x86_64|amd64)
    readonly RUST_TARGET=x86_64-unknown-linux-gnu
    readonly ZLS_ARCHIVE=zls-x86_64-linux.tar.xz
    readonly ZLS_SHA256=ded6d562a0b86ee878b1ddf70ffab2797ce3cdca3b02d6077548f9d56dff96b6
    readonly DENO_ARCHIVE=deno-x86_64-unknown-linux-gnu.zip
    readonly EXPERT_ASSET=expert_linux_amd64
    readonly GLEAM_ARCHIVE=gleam-v1.15.1-x86_64-unknown-linux-musl.tar.gz
    readonly LUA_LS_ARCHIVE=lua-language-server-3.17.1-linux-x64.tar.gz
    readonly LUA_LS_SHA256=248b0858a0afc8233f2535e89b648398b2202cb96cf51ce187e3263923dd0223
    readonly DART_ARCH=x64
    readonly MARKSMAN_ASSET=marksman-linux-x64
    readonly MARKSMAN_SHA256=be5098e8213219269c47fc0d916a66fa31ce0602ec967475c722260aabf26087
    readonly TEXLAB_ARCHIVE=texlab-x86_64-linux.tar.gz
    readonly TEXLAB_SHA256=3756a02aedf5ad4636091b3608059ff732a20b34d73696f0ef03323ce08e9746
    readonly COURSIER_URL="https://github.com/coursier/coursier/releases/download/v${COURSIER_VERSION}/cs-x86_64-pc-linux.gz"
    readonly COURSIER_SHA256=d2c0572a17fb6146ea65349b59dd216b38beff60ae22bce6e549867c6ed2eda6
    readonly GHCUP_ASSET=x86_64-linux-ghcup-0.2.6.2
    readonly OMNISHARP_ARCHIVE=omnisharp-linux-x64.tar.gz
    readonly OMNISHARP_SHA256=8c5e03a59ae04ee18acf2e5034267f32117f2f3fe2741cda141ff1cea0c9c5d5
    readonly TERRAFORM_ARCH=amd64
    readonly HELM_LS_ASSET=helm_ls_linux_amd64
    readonly SWIFTLINT_ARCHIVE=swiftlint_linux_amd64.zip
    readonly SWIFTLINT_SHA256=caeed6f4a679c35539ffaf124f6c4ab4a8416917f7d8796279dc52b74026059d
    ;;
  aarch64|arm64)
    readonly RUST_TARGET=aarch64-unknown-linux-gnu
    readonly ZLS_ARCHIVE=zls-aarch64-linux.tar.xz
    readonly ZLS_SHA256=430cd293d201eb70ae2519dbc96c854bf8791b8df7fc9392e8d2dc9680a2bed7
    readonly DENO_ARCHIVE=deno-aarch64-unknown-linux-gnu.zip
    readonly EXPERT_ASSET=expert_linux_arm64
    readonly GLEAM_ARCHIVE=gleam-v1.15.1-aarch64-unknown-linux-musl.tar.gz
    readonly LUA_LS_ARCHIVE=lua-language-server-3.17.1-linux-arm64.tar.gz
    readonly LUA_LS_SHA256=680285a36d8cf7b17ca4be7a2f9c93643ebd8daec0b7425a6b7a02d003f3da81
    readonly DART_ARCH=arm64
    readonly MARKSMAN_ASSET=marksman-linux-arm64
    readonly MARKSMAN_SHA256=db8e124527f7f8048e3e6c91821b9c52ef173d92c01e47d221bf1337afd962fb
    readonly TEXLAB_ARCHIVE=texlab-aarch64-linux.tar.gz
    readonly TEXLAB_SHA256=c19fd01f530a7f23395d50e38a3a8f93e262c3be02f5bd61b27c8d6537b68bea
    readonly COURSIER_URL="https://github.com/VirtusLab/coursier-m1/releases/download/v${COURSIER_VERSION}/cs-aarch64-pc-linux.gz"
    readonly COURSIER_SHA256=96b4c7580d253b6999a40e94413ca6c4a9bd2339ecce4754ac31a26d1a12fcbf
    readonly GHCUP_ASSET=aarch64-linux-ghcup-0.2.6.2
    readonly OMNISHARP_ARCHIVE=omnisharp-linux-arm64.tar.gz
    readonly OMNISHARP_SHA256=11da9b1542ea23a8a45970d84750f32e5fe6dea8a21123c663c6d1f0a3eca115
    readonly TERRAFORM_ARCH=arm64
    readonly HELM_LS_ASSET=helm_ls_linux_arm64
    readonly SWIFTLINT_ARCHIVE=swiftlint_linux_arm64.zip
    readonly SWIFTLINT_SHA256=9ffa52f478e6d8eb485d37d14715ffac90abc81c58f3370d598bf75be05605f8
    ;;
  *)
    printf 'unsupported architecture: %s\n' "$(uname -m)" >&2
    exit 1
    ;;
esac

mkdir -p "$BIN_DIR" "$SHARE_DIR"

fetch() {
  curl -fsSL "$1" -o "$2"
}

verify_sha256() {
  local file=$1
  local expected=$2
  printf '%s  %s\n' "$expected" "$file" | sha256sum -c - >&2
}

download_hardcoded() {
  local name=$1
  local url=$2
  local expected=$3
  local output="$TMP_ROOT/$name"
  fetch "$url" "$output"
  verify_sha256 "$output" "$expected"
  printf '%s\n' "$output"
}

download_published() {
  local name=$1
  local url=$2
  local checksums_url=$3
  local output="$TMP_ROOT/$name"
  local checksums="$TMP_ROOT/$name.checksums"
  local expected

  fetch "$url" "$output"
  fetch "$checksums_url" "$checksums"
  expected="$(awk -v target="$name" '
    {
      candidate = $NF
      sub(/^\*/, "", candidate)
      sub(/^\.\//, "", candidate)
      if (candidate == target && tolower($1) ~ /^[0-9a-f]{64}$/) {
        print tolower($1)
        exit
      }
    }
  ' "$checksums")"
  if [[ -z "$expected" ]]; then
    expected="$(awk '
      tolower($1) ~ /^[0-9a-f]{64}$/ { hash = tolower($1); count++ }
      END { if (count == 1) print hash }
    ' "$checksums")"
  fi
  if [[ -z "$expected" ]]; then
    printf 'checksum for %s not found in %s\n' "$name" "$checksums_url" >&2
    exit 1
  fi
  verify_sha256 "$output" "$expected"
  printf '%s\n' "$output"
}

install_raw() {
  local source=$1
  local directory=$2
  local name=$3
  install -d "$directory"
  install -m 0755 "$source" "$directory/$name"
  ln -s "$directory/$name" "$BIN_DIR/$name"
}

# Rust toolchain and rust-analyzer.
rustup_init="$(download_published \
  rustup-init \
  "https://static.rust-lang.org/rustup/archive/${RUSTUP_VERSION}/${RUST_TARGET}/rustup-init" \
  "https://static.rust-lang.org/rustup/archive/${RUSTUP_VERSION}/${RUST_TARGET}/rustup-init.sha256")"
chmod +x "$rustup_init"
export CARGO_HOME=/home/agent/.cargo
export RUSTUP_HOME=/home/agent/.rustup
"$rustup_init" -y --no-modify-path --profile minimal \
  --default-toolchain "$RUST_VERSION" --component rust-analyzer
ln -s "$CARGO_HOME/bin/rust-analyzer" "$BIN_DIR/rust-analyzer"

# Native release archives.
zls_archive="$(download_hardcoded "$ZLS_ARCHIVE" \
  "https://github.com/zigtools/zls/releases/download/${ZLS_VERSION}/${ZLS_ARCHIVE}" \
  "$ZLS_SHA256")"
install -d "$SHARE_DIR/zls"
tar -xf "$zls_archive" -C "$SHARE_DIR/zls"
ln -s "$SHARE_DIR/zls/zls" "$BIN_DIR/zls"

deno_archive="$(download_published "$DENO_ARCHIVE" \
  "https://github.com/denoland/deno/releases/download/v${DENO_VERSION}/${DENO_ARCHIVE}" \
  "https://github.com/denoland/deno/releases/download/v${DENO_VERSION}/${DENO_ARCHIVE}.sha256sum")"
install -d "$SHARE_DIR/deno"
unzip -q "$deno_archive" -d "$SHARE_DIR/deno"
ln -s "$SHARE_DIR/deno/deno" "$BIN_DIR/deno"

expert_binary="$(download_published "$EXPERT_ASSET" \
  "https://github.com/expert-lsp/expert/releases/download/v${EXPERT_VERSION}/${EXPERT_ASSET}" \
  "https://github.com/expert-lsp/expert/releases/download/v${EXPERT_VERSION}/expert_checksums.txt")"
install_raw "$expert_binary" "$SHARE_DIR/expert" expert

gleam_archive="$(download_published "$GLEAM_ARCHIVE" \
  "https://github.com/gleam-lang/gleam/releases/download/v${GLEAM_VERSION}/${GLEAM_ARCHIVE}" \
  "https://github.com/gleam-lang/gleam/releases/download/v${GLEAM_VERSION}/${GLEAM_ARCHIVE}.sha256")"
install -d "$SHARE_DIR/gleam"
tar -xf "$gleam_archive" -C "$SHARE_DIR/gleam"
ln -s "$SHARE_DIR/gleam/gleam" "$BIN_DIR/gleam"

lua_ls_archive="$(download_hardcoded "$LUA_LS_ARCHIVE" \
  "https://github.com/LuaLS/lua-language-server/releases/download/${LUA_LS_VERSION}/${LUA_LS_ARCHIVE}" \
  "$LUA_LS_SHA256")"
install -d "$SHARE_DIR/lua-language-server"
tar -xf "$lua_ls_archive" -C "$SHARE_DIR/lua-language-server"
ln -s "$SHARE_DIR/lua-language-server/bin/lua-language-server" "$BIN_DIR/lua-language-server"

dart_archive_name="dartsdk-linux-${DART_ARCH}-release.zip"
dart_archive="$(download_published "$dart_archive_name" \
  "https://storage.googleapis.com/dart-archive/channels/stable/release/${DART_VERSION}/sdk/${dart_archive_name}" \
  "https://storage.googleapis.com/dart-archive/channels/stable/release/${DART_VERSION}/sdk/${dart_archive_name}.sha256sum")"
unzip -q "$dart_archive" -d "$TMP_ROOT/dart"
mv "$TMP_ROOT/dart/dart-sdk" "$SHARE_DIR/dart"
ln -s "$SHARE_DIR/dart/bin/dart" "$BIN_DIR/dart"

marksman_binary="$(download_hardcoded "$MARKSMAN_ASSET" \
  "https://github.com/artempyanykh/marksman/releases/download/${MARKSMAN_VERSION}/${MARKSMAN_ASSET}" \
  "$MARKSMAN_SHA256")"
install_raw "$marksman_binary" "$SHARE_DIR/marksman" marksman

texlab_archive="$(download_hardcoded "$TEXLAB_ARCHIVE" \
  "https://github.com/latex-lsp/texlab/releases/download/v${TEXLAB_VERSION}/${TEXLAB_ARCHIVE}" \
  "$TEXLAB_SHA256")"
install -d "$SHARE_DIR/texlab"
tar -xf "$texlab_archive" -C "$SHARE_DIR/texlab"
ln -s "$SHARE_DIR/texlab/texlab" "$BIN_DIR/texlab"

# Odin and OLS are pinned source builds.
git clone --quiet --depth 1 --branch "$ODIN_VERSION" \
  https://github.com/odin-lang/Odin.git "$TMP_ROOT/odin-src"
[[ "$(git -C "$TMP_ROOT/odin-src" rev-parse HEAD)" == "$ODIN_COMMIT" ]]
(
  cd "$TMP_ROOT/odin-src"
  LLVM_CONFIG=llvm-config ./build_odin.sh release
)
install -d "$SHARE_DIR/odin"
install -m 0755 "$TMP_ROOT/odin-src/odin" "$SHARE_DIR/odin/odin"
cp -a "$TMP_ROOT/odin-src/base" "$TMP_ROOT/odin-src/core" \
  "$TMP_ROOT/odin-src/vendor" "$TMP_ROOT/odin-src/shared" "$SHARE_DIR/odin/"
make -C "$SHARE_DIR/odin/vendor/cgltf/src"
make -C "$SHARE_DIR/odin/vendor/stb/src"
make -C "$SHARE_DIR/odin/vendor/miniaudio/src"
cat > "$BIN_DIR/odin" <<'EOF'
#!/usr/bin/env bash
export ODIN_ROOT=/home/agent/.local/share/odin
exec /home/agent/.local/share/odin/odin "$@"
EOF
chmod +x "$BIN_DIR/odin"

git clone --quiet --depth 1 --branch "$OLS_VERSION" \
  https://github.com/DanielGavin/ols.git "$TMP_ROOT/ols-src"
[[ "$(git -C "$TMP_ROOT/ols-src" rev-parse HEAD)" == "$OLS_COMMIT" ]]
sed -i 's/-microarch:native//g' "$TMP_ROOT/ols-src/build.sh"
(
  cd "$TMP_ROOT/ols-src"
  ./build.sh
  ./odinfmt.sh
)
install -d "$SHARE_DIR/ols"
install -m 0755 "$TMP_ROOT/ols-src/ols" "$TMP_ROOT/ols-src/odinfmt" "$SHARE_DIR/ols/"
cat > "$BIN_DIR/ols" <<'EOF'
#!/usr/bin/env bash
export ODIN_ROOT=/home/agent/.local/share/odin
export OLS_BUILTIN_FOLDER=/home/agent/.local/share/odin/base/builtin
exec /home/agent/.local/share/ols/ols "$@"
EOF
chmod +x "$BIN_DIR/ols"
ln -s "$SHARE_DIR/ols/odinfmt" "$BIN_DIR/odinfmt"

# JVM language servers.
jdtls_archive_name="jdt-language-server-${JDTLS_VERSION}.tar.gz"
jdtls_archive="$(download_published "$jdtls_archive_name" \
  "https://download.eclipse.org/jdtls/snapshots/${jdtls_archive_name}" \
  "https://download.eclipse.org/jdtls/snapshots/${jdtls_archive_name}.sha256")"
install -d "$SHARE_DIR/jdtls"
tar -xf "$jdtls_archive" -C "$SHARE_DIR/jdtls"
ln -s "$SHARE_DIR/jdtls/bin/jdtls" "$BIN_DIR/jdtls"

coursier_archive="$(download_hardcoded coursier.gz "$COURSIER_URL" "$COURSIER_SHA256")"
gzip -dc "$coursier_archive" > "$TMP_ROOT/cs"
chmod +x "$TMP_ROOT/cs"
install -d "$SHARE_DIR/metals/bin"
COURSIER_CACHE=/home/agent/.cache/coursier \
  "$TMP_ROOT/cs" install --install-dir "$SHARE_DIR/metals/bin" "metals:${METALS_VERSION}"
ln -s "$SHARE_DIR/metals/bin/metals" "$BIN_DIR/metals"
rm -f "$TMP_ROOT/cs" "$coursier_archive"

# Haskell toolchain and language server.
ghcup_binary="$(download_published "$GHCUP_ASSET" \
  "https://downloads.haskell.org/~ghcup/${GHCUP_VERSION}/${GHCUP_ASSET}" \
  "https://downloads.haskell.org/~ghcup/${GHCUP_VERSION}/SHA256SUMS")"
install -d /home/agent/.ghcup/bin
install -m 0755 "$ghcup_binary" /home/agent/.ghcup/bin/ghcup
export PATH="/home/agent/.ghcup/bin:$PATH"
ghcup config set url-source '["GHCupURL"]'
ghcup install ghc "$GHC_VERSION" --set
ghcup install cabal "$CABAL_VERSION" --set
ghcup install hls "$HLS_VERSION" --set
for command in ghc ghci ghc-pkg haddock runghc runhaskell cabal haskell-language-server-wrapper; do
  ln -s "/home/agent/.ghcup/bin/$command" "$BIN_DIR/$command"
done
for binary in /home/agent/.ghcup/bin/haskell-language-server-[0-9]*; do
  [[ -e "$binary" ]] || continue
  ln -s "$binary" "$BIN_DIR/$(basename "$binary")"
done

# BEAM language servers.
elixir_ls_archive="$(download_hardcoded "elixir-ls-v${ELIXIR_LS_VERSION}.zip" \
  "https://github.com/elixir-lsp/elixir-ls/releases/download/v${ELIXIR_LS_VERSION}/elixir-ls-v${ELIXIR_LS_VERSION}.zip" \
  c07680593973ee9465e7361523b873baaacd258a4741342b1533bee5ac5c6c5e)"
install -d "$SHARE_DIR/elixir-ls"
unzip -q "$elixir_ls_archive" -d "$SHARE_DIR/elixir-ls"
chmod +x "$SHARE_DIR/elixir-ls/language_server.sh"
ln -s "$SHARE_DIR/elixir-ls/language_server.sh" "$BIN_DIR/elixir-ls"

# PHP, C#, Terraform, Helm, and Swift.
phpactor_binary="$(download_hardcoded phpactor.phar \
  "https://github.com/phpactor/phpactor/releases/download/${PHPACTOR_VERSION}/phpactor.phar" \
  8c0155380b9d7559a12f35ddf8d09c1dc23e72f1797498038251fc35ad15574d)"
install_raw "$phpactor_binary" "$SHARE_DIR/phpactor" phpactor

omnisharp_archive="$(download_hardcoded "$OMNISHARP_ARCHIVE" \
  "https://github.com/OmniSharp/omnisharp-roslyn/releases/download/v${OMNISHARP_VERSION}/${OMNISHARP_ARCHIVE}" \
  "$OMNISHARP_SHA256")"
install -d "$SHARE_DIR/omnisharp"
tar -xf "$omnisharp_archive" -C "$SHARE_DIR/omnisharp"
ln -s "$SHARE_DIR/omnisharp/omnisharp/OmniSharp" "$BIN_DIR/OmniSharp"
ln -s "$SHARE_DIR/omnisharp/omnisharp/OmniSharp" "$BIN_DIR/omnisharp"

terraform_archive_name="terraform-ls_${TERRAFORM_LS_VERSION}_linux_${TERRAFORM_ARCH}.zip"
terraform_archive="$(download_published "$terraform_archive_name" \
  "https://releases.hashicorp.com/terraform-ls/${TERRAFORM_LS_VERSION}/${terraform_archive_name}" \
  "https://releases.hashicorp.com/terraform-ls/${TERRAFORM_LS_VERSION}/terraform-ls_${TERRAFORM_LS_VERSION}_SHA256SUMS")"
install -d "$SHARE_DIR/terraform-ls"
unzip -q "$terraform_archive" -d "$SHARE_DIR/terraform-ls"
ln -s "$SHARE_DIR/terraform-ls/terraform-ls" "$BIN_DIR/terraform-ls"

helm_ls_binary="$(download_published "$HELM_LS_ASSET" \
  "https://github.com/mrjosh/helm-ls/releases/download/v${HELM_LS_VERSION}/${HELM_LS_ASSET}" \
  "https://github.com/mrjosh/helm-ls/releases/download/v${HELM_LS_VERSION}/${HELM_LS_ASSET}.sha256sum")"
install_raw "$helm_ls_binary" "$SHARE_DIR/helm-ls" helm_ls

swiftlint_archive="$(download_hardcoded "$SWIFTLINT_ARCHIVE" \
  "https://github.com/realm/SwiftLint/releases/download/${SWIFTLINT_VERSION}/${SWIFTLINT_ARCHIVE}" \
  "$SWIFTLINT_SHA256")"
install -d "$SHARE_DIR/swiftlint"
unzip -q "$swiftlint_archive" -d "$SHARE_DIR/swiftlint"
if [[ ! -x "$SHARE_DIR/swiftlint/swiftlint" ]] || \
   ! "$SHARE_DIR/swiftlint/swiftlint" version >/dev/null 2>&1; then
  rm -rf "$SHARE_DIR/swiftlint"
  git clone --quiet --depth 1 --branch "$SWIFTLINT_VERSION" \
    https://github.com/realm/SwiftLint.git "$TMP_ROOT/swiftlint-src"
  [[ "$(git -C "$TMP_ROOT/swiftlint-src" rev-parse HEAD)" == "$SWIFTLINT_COMMIT" ]]
  swift build --package-path "$TMP_ROOT/swiftlint-src" \
    --configuration release --disable-sandbox
  swiftlint_bin_dir="$(swift build --package-path "$TMP_ROOT/swiftlint-src" \
    --configuration release --show-bin-path)"
  install -d "$SHARE_DIR/swiftlint"
  install -m 0755 "$swiftlint_bin_dir/swiftlint" \
    "$SHARE_DIR/swiftlint/swiftlint"
fi
ln -s "$SHARE_DIR/swiftlint/swiftlint" "$BIN_DIR/swiftlint"
