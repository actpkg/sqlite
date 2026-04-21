wasm := "target/wasm32-wasip2/release/component_sqlite.wasm"

act := env("ACT", "npx @actcore/act")
hurl := env("HURL", "npx @orangeopensource/hurl")
cc := env("CC", "/opt/wasi-sdk/bin/clang")
oras := env("ORAS", "oras")
registry := env("OCI_REGISTRY", "ghcr.io/actpkg")
port := `npx get-port-cli`
addr := "[::1]:" + port
baseurl := "http://" + addr

init:
    wit-deps

setup: init
    prek install

build variant="sqlite":
    CC="{{cc}}" cargo build --release {{ if variant == "sqlite-vec" { "--features vec" } else { "" } }}

clippy variant="sqlite":
    CC="{{cc}}" cargo clippy {{ if variant == "sqlite-vec" { "--features vec" } else { "" } }} -- -D warnings

test variant="sqlite":
    #!/usr/bin/env bash
    set -euo pipefail
    just build {{variant}}
    DB_DIR=$(mktemp -d)
    {{act}} run --http --listen "{{addr}}" {{wasm}} \
      --fs-policy allowlist --fs-allow /tmp --fs-allow /dev/urandom --fs-allow "$DB_DIR" &
    trap "kill $!; rm -rf $DB_DIR" EXIT
    npx wait-on -t 180s {{baseurl}}/info
    if [ "{{variant}}" = "sqlite-vec" ]; then
      {{hurl}} --test --variable "baseurl={{baseurl}}" --variable "db_path=$DB_DIR/test.db" e2e/*.hurl e2e/vec/*.hurl
    else
      {{hurl}} --test --variable "baseurl={{baseurl}}" --variable "db_path=$DB_DIR/test.db" e2e/*.hurl
    fi

publish variant="sqlite":
    #!/usr/bin/env bash
    set -euo pipefail
    INFO=$({{act}} info {{wasm}} --format json)
    NAME=$(echo "$INFO" | jq -r .name)
    VERSION=$(echo "$INFO" | jq -r .version)
    DESC=$(echo "$INFO" | jq -r .description)
    # If this version is already published, require that its content matches
    # our local build exactly. A silent skip when content drifted would mean
    # the registry lies to downstream consumers (e.g. actpkg.dev).
    TMPDIR=$(mktemp -d)
    trap 'rm -rf "$TMPDIR"' EXIT
    if {{act}} pull "{{registry}}/$NAME:$VERSION" -o "$TMPDIR/remote.wasm" >/dev/null 2>&1; then
      LOCAL_HASH=$(sha256sum {{wasm}} | awk '{print $1}')
      REMOTE_HASH=$(sha256sum "$TMPDIR/remote.wasm" | awk '{print $1}')
      if [ "$LOCAL_HASH" = "$REMOTE_HASH" ]; then
        echo "$NAME:$VERSION already published (identical content), skipping"
        exit 0
      fi
      echo "ERROR: $NAME:$VERSION is already published, but its content differs from the local build." >&2
      echo "       Bump the patch version in Cargo.toml — a metadata-only change (capabilities," >&2
      echo "       description, skill/) still requires a version bump to reach the registry." >&2
      echo "       Local:  $LOCAL_HASH" >&2
      echo "       Remote: $REMOTE_HASH" >&2
      exit 1
    fi
    SOURCE=$(git remote get-url origin 2>/dev/null | sed 's/\.git$//' | sed 's|git@github.com:|https://github.com/|' || echo "")
    OUTPUT=$({{oras}} push "{{registry}}/$NAME:$VERSION" \
      --artifact-type application/wasm \
      --annotation "org.opencontainers.image.version=$VERSION" \
      --annotation "org.opencontainers.image.description=$DESC" \
      --annotation "org.opencontainers.image.source=$SOURCE" \
      "{{wasm}}:application/wasm" 2>&1)
    echo "$OUTPUT"
    DIGEST=$(echo "$OUTPUT" | grep "^Digest:" | awk '{print $2}')
    {{oras}} tag "{{registry}}/$NAME:$VERSION" latest
    if [ -n "${GITHUB_OUTPUT:-}" ]; then
      echo "image={{registry}}/$NAME" >> "$GITHUB_OUTPUT"
      echo "digest=$DIGEST" >> "$GITHUB_OUTPUT"
    fi
