wasm := "target/wasm32-wasip2/release/component_sqlite.wasm"

act := env("ACT", "npx @actcore/act")
actbuild := env("ACT_BUILD", "npx @actcore/act-build")
hurl := env("HURL", "npx @orangeopensource/hurl")
cc := env("CC", "/opt/wasi-sdk/bin/clang")
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
      --fs-policy allowlist --fs-allow /dev/urandom --fs-allow "$DB_DIR" &
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
    SOURCE=$(git remote get-url origin 2>/dev/null | sed 's/\.git$//' | sed 's|git@github.com:|https://github.com/|' || echo "")
    OUTPUT=$({{actbuild}} push {{wasm}} "{{registry}}/$NAME:$VERSION" \
      --skip-if-identical \
      --also-tag latest \
      --source "$SOURCE" 2>&1)
    echo "$OUTPUT"
    DIGEST=$(echo "$OUTPUT" | grep "^Digest:" | awk '{print $2}')
    if [ -n "${GITHUB_OUTPUT:-}" ]; then
      echo "image={{registry}}/$NAME" >> "$GITHUB_OUTPUT"
      echo "digest=$DIGEST" >> "$GITHUB_OUTPUT"
    fi
