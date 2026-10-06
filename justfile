wasm := "target/wasm32-wasip2/release/component_sqlite.wasm"
# Registry/namespace to publish to (no name, no tag). The component name comes
# from the packed manifest, so each matrix variant lands on its own repo
# (sqlite, sqlite-vec). Override with OCI_REGISTRY.
registry := env("OCI_REGISTRY", "actpkg.dev/library")

act := env("ACT", "npx @actcore/act")
actbuild := env("ACT_BUILD", "npx @actcore/act-build")
cc := env("CC", "/opt/wasi-sdk/bin/clang")

# Fetch WIT deps from the registry (ghcr.io/actcore) into wit/deps/.
# wkg-registry.toml maps the act namespace -> actcore.dev (well-known -> ghcr.io/actcore).
init:
    WKG_CONFIG_FILE=wkg-registry.toml wkg wit fetch --type wit

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

# Rust e2e harness (rmcp client) — replaced the python fastmcp/pytest suite
# (test_*.py + conftest.py kept next to it as the assertion reference). Must
# run from inside e2e/: cargo discovers config from the CWD, and e2e/ carries
# the host-target pin. `pack` (not bare `build`) is the dependency on purpose:
# an unpacked wasm declares no ceiling, so every grant is refused.
test variant="sqlite": (pack variant)
    cd e2e && ACT="{{act}}" WASM="../{{wasm}}" SQLITE_VARIANT="{{variant}}" cargo test

publish variant="sqlite": (pack variant)
    #!/usr/bin/env bash
    set -euo pipefail
    INFO=$({{act}} inspect component-manifest {{wasm}})
    NAME=$(echo "$INFO" | jq -r .std.name)
    VERSION=$(echo "$INFO" | jq -r .std.version)
    OUTPUT=$({{actbuild}} push {{wasm}} "{{registry}}/$NAME:$VERSION" \
      --skip-if-exists \
      --also-tag latest 2>&1) || { echo "$OUTPUT" >&2; exit 1; }
    echo "$OUTPUT"
    DIGEST=$(echo "$OUTPUT" | grep "^Digest:" | awk '{print $2}' || true)
    if [ -n "${GITHUB_OUTPUT:-}" ]; then
      echo "image={{registry}}/$NAME" >> "$GITHUB_OUTPUT"
      echo "digest=$DIGEST" >> "$GITHUB_OUTPUT"
    fi
