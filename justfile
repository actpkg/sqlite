wasm := "target/wasm32-wasip2/release/component_sqlite.wasm"
act := env("ACT", "act")
cc := env("CC", "/opt/wasi-sdk/bin/clang")
oras := env("ORAS", "oras")
registry := env("OCI_REGISTRY", "ghcr.io/actpkg")
port := `python3 -c 'import socket; s=socket.socket(socket.AF_INET, socket.SOCK_STREAM); s.bind(("", 0)); print(s.getsockname()[1]); s.close()'`
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
    {{act}} run --http --listen "{{addr}}" {{wasm}} --allow-dir "/data:$DB_DIR" &
    trap "kill $!; rm -rf $DB_DIR" EXIT
    npx wait-on {{baseurl}}/info
    if [ "{{variant}}" = "sqlite-vec" ]; then
      hurl --test --variable "baseurl={{baseurl}}" --variable "db_path=/data/test.db" e2e/*.hurl e2e/vec/*.hurl
    else
      hurl --test --variable "baseurl={{baseurl}}" --variable "db_path=/data/test.db" e2e/*.hurl
    fi

publish variant="sqlite":
    #!/usr/bin/env bash
    set -euo pipefail
    INFO=$({{act}} info {{wasm}} --format json)
    NAME=$(echo "$INFO" | jq -r .name)
    VERSION=$(echo "$INFO" | jq -r .version)
    DESC=$(echo "$INFO" | jq -r .description)
    if {{oras}} manifest fetch "{{registry}}/$NAME:$VERSION" >/dev/null 2>&1; then
      echo "$NAME:$VERSION already published, skipping"
      exit 0
    fi
    SOURCE=$(git remote get-url origin 2>/dev/null | sed 's/\.git$//' | sed 's|git@github.com:|https://github.com/|' || echo "")
    OUTPUT=$({{oras}} push "{{registry}}/$NAME:$VERSION" \
      --config /dev/null:application/vnd.oci.empty.v1+json \
      --annotation "org.opencontainers.image.version=$VERSION" \
      --annotation "org.opencontainers.image.description=$DESC" \
      --annotation "org.opencontainers.image.source=$SOURCE" \
      "{{wasm}}:application/wasm" 2>&1)
    echo "$OUTPUT"
    DIGEST=$(echo "$OUTPUT" | grep "^Digest:" | awk '{print $2}')
    {{oras}} tag "{{registry}}/$NAME:$VERSION" latest
    # Output for CI (GITHUB_OUTPUT)
    if [ -n "${GITHUB_OUTPUT:-}" ]; then
      echo "image={{registry}}/$NAME" >> "$GITHUB_OUTPUT"
      echo "digest=$DIGEST" >> "$GITHUB_OUTPUT"
    fi
