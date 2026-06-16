wasm := "target/wasm32-wasip2/release/component_sqlite.wasm"
# OCI reference to publish to (registry/namespace/name, no tag). Override with OCI_REF.
component_ref := env("OCI_REF", "actpkg.dev/library/sqlite")

act := env("ACT", "npx @actcore/act")
actbuild := env("ACT_BUILD", "npx @actcore/act-build")
hurl := env("HURL", "npx @orangeopensource/hurl")
cc := env("CC", "/opt/wasi-sdk/bin/clang")
# Random port for the e2e server, in a safe range: above the well-known/common
# dev ports and below the Linux outbound ephemeral range (32768+).
port := `shuf -i 10000-29999 -n 1`
addr := "[::1]:" + port
baseurl := "http://" + addr
port2 := `shuf -i 10000-29999 -n 1`
addr2 := "[::1]:" + port2
baseurl2 := "http://" + addr2

init:
    wit-deps

setup: init
    prek install

build variant="sqlite":
    CC="{{cc}}" cargo build --release {{ if variant == "sqlite-vec" { "--features vec" } else { "" } }}

clippy variant="sqlite":
    CC="{{cc}}" cargo clippy {{ if variant == "sqlite-vec" { "--features vec" } else { "" } }} -- -D warnings

# Embed act:component metadata and act:skill into the wasm. The vec variant
# overrides name/description at pack time (lean macro takes no args).
pack variant="sqlite": (build variant)
    {{actbuild}} pack {{wasm}} {{ if variant == "sqlite-vec" { '--set std.name=sqlite-vec --set "std.description=SQLite database operations with vector search (sqlite-vec)"' } else { "" } }}

test variant="sqlite":
    #!/usr/bin/env bash
    set -euo pipefail
    just pack {{variant}}
    DB_DIR=$(mktemp -d)
    # Mode 1: session-of-1 (single-DB deployment). Host pre-opens the session and
    # injects std:session-id into every call; existing tests need no db_path.
    {{act}} run --http --listen "{{addr}}" {{wasm}} \
      --session-args "{\"database_path\":\"$DB_DIR/test.db\"}" \
      --fs-policy allowlist --fs-allow /dev/urandom --fs-allow "$DB_DIR" &
    PID=$!
    npx wait-on -t 180s {{baseurl}}/info
    if [ "{{variant}}" = "sqlite-vec" ]; then
      {{hurl}} --test --variable "baseurl={{baseurl}}" e2e/*.hurl e2e/vec/*.hurl
    else
      {{hurl}} --test --variable "baseurl={{baseurl}}" e2e/*.hurl
    fi
    kill $PID; wait $PID 2>/dev/null || true
    # Mode 2: full session-provider — isolation across two sessions/DBs.
    {{act}} run --http --listen "{{addr2}}" {{wasm}} \
      --fs-policy allowlist --fs-allow /dev/urandom --fs-allow "$DB_DIR" &
    PID=$!
    trap "kill $PID 2>/dev/null; rm -rf $DB_DIR" EXIT
    npx wait-on -t 180s {{baseurl2}}/info
    {{hurl}} --test --variable "baseurl={{baseurl2}}" --variable "db_dir=$DB_DIR" e2e/isolation/*.hurl

publish variant="sqlite": (pack variant)
    #!/usr/bin/env bash
    set -euo pipefail
    INFO=$({{act}} info {{wasm}} --format json)
    VERSION=$(echo "$INFO" | jq -r .version)
    SOURCE=$(git remote get-url origin 2>/dev/null | sed 's/\.git$//' | sed 's|git@github.com:|https://github.com/|' || echo "")
    OUTPUT=$({{actbuild}} push {{wasm}} "{{component_ref}}:$VERSION" \
      --skip-if-exists \
      --also-tag latest \
      --source "$SOURCE" 2>&1) || { echo "$OUTPUT" >&2; exit 1; }
    echo "$OUTPUT"
    DIGEST=$(echo "$OUTPUT" | grep "^Digest:" | awk '{print $2}' || true)
    if [ -n "${GITHUB_OUTPUT:-}" ]; then
      echo "image={{component_ref}}" >> "$GITHUB_OUTPUT"
      echo "digest=$DIGEST" >> "$GITHUB_OUTPUT"
    fi
