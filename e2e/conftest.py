"""Shared fixtures for the MCP-driven e2e suite.

The suite drives the packed component through `act run --mcp` over stdio with
a real MCP client, so what the tests observe is what an agent observes.

sqlite is a session-provider: every real tool call needs `std:session-id` in
its argument metadata (ACT-MCP §3.2). This suite gets that id by calling the
virtual `open_session`/`close_session` tools the MCP adapter synthesises for
any session-provider component — the path an agent actually uses — rather
than starting the host with `--session-args` (session-of-1). That shortcut
opens the session *before* the host's listener comes up, so a guest that
stalls in `open-session` leaves a host that never comes up with nothing on
stderr; two components are red in CI right now for exactly that reason.
Session-of-1 itself is already covered by `act-cli`'s own
`tests/session_of_1_mcp.rs` suite, so nothing is lost by not re-testing it
here — this suite only exercises the full session-provider path.
"""

import asyncio
import json
import os
import shlex
import subprocess
import pytest
from contextlib import AsyncExitStack
from pathlib import Path

from fastmcp import Client
from fastmcp.client.transports import StdioTransport

# Measured in docs/specs/2026-08-08-e2e-harness-findings.md, question 1.
from mcp.shared.exceptions import McpError

WASM = "target/wasm32-wasip2/release/component_sqlite.wasm"

# ACT's audit trail writes to stderr unconditionally — it is not governed by
# RUST_LOG — so it is redirected to a file rather than left to flood pytest.
LOG_FILE = Path(".pytest-act-stderr.log")

# Deliberately loose. `act run --mcp` instantiates the component before it
# answers `initialize`, so "connect" includes that cost -- for a heavy
# component (servo embeds a browser engine) it is seconds, and on a loaded
# runner it varies. 30s tripped servo in CI while its healthy connect was
# ~8s, so the bound sits well above the worst observed cost and still well
# below the per-test timeout, keeping this the diagnostic that fires first.
CONNECT_TIMEOUT = 120

# Which build variant the justfile's `test` recipe packed before invoking
# pytest. sqlite and sqlite-vec export the identical tool set — only the SQL
# surface differs (vec0 virtual tables) — so this only gates
# test_vector_search.py; every other test runs unchanged against either.
VARIANT = os.environ.get("SQLITE_VARIANT", "sqlite")


@pytest.fixture(scope="session")
def act_command() -> list[str]:
    """The ACT invocation, honouring the same override the justfile uses.

    Parsed with shlex, not treated as a single path: the justfile's own
    default for its `act` variable is `npx @actcore/act` — two words — which
    cannot be `argv[0]` for a non-shell `subprocess.run`/`StdioTransport`
    call. A bare `os.environ.get("ACT", "act")` string breaks that default;
    splitting it is what makes both forms ("act" on PATH, and the npx
    two-word default) actually spawn.
    """
    return shlex.split(os.environ.get("ACT", "act"))


@pytest.fixture(scope="session")
def wasm_path(act_command: list[str]) -> Path:
    """The packed component.

    Existence is not enough and neither is a fresh mtime: `cargo build`
    produces a wasm with no `act:component` custom section, and an unpacked
    artifact declares no capability ceiling, so every grant is refused as
    "outside ceiling" and the failures point anywhere but here. This has
    already bitten three components in this workspace, so the fixture checks
    the section rather than the file.
    """
    path = Path(WASM)
    if not path.exists():
        pytest.fail(f"{path} is missing — run `just pack` first")
    probe = subprocess.run(
        [*act_command, "inspect", "component-manifest", str(path)],
        capture_output=True, text=True,
    )
    name = json.loads(probe.stdout or "{}").get("std", {}).get("name", "unknown")
    if name in ("", "unknown"):
        pytest.fail(f"{path} is built but not packed — run `just pack`")
    return path


@pytest.fixture
async def client(act_command: list[str], wasm_path: Path, tmp_path: Path):
    """A connected MCP client, one `act` process per test.

    sqlite holds a real database file open per session, so it is stateful
    across both calls and files — the old justfile's grant covered exactly
    two things: a private database directory and `/dev/urandom` (SQLite's
    WAL journal mode, turned on at connection-open in src/lib.rs, reads OS
    randomness for the wal-index header salt on every open). Both are
    carried verbatim here, `/dev/urandom` fixed and the database directory
    scoped to this test's own `tmp_path` rather than one shared `mktemp -d`
    for the whole suite.
    """
    grant = json.dumps({
        "wasi:filesystem": {
            "mode": "allowlist",
            "allow": [
                {"path": "/dev/urandom", "mode": "rw"},
                {"path": str(tmp_path), "mode": "rw"},
            ],
        }
    })
    transport = StdioTransport(
        command=act_command[0],
        args=[*act_command[1:], "run", str(wasm_path), "--mcp", "--grant", grant],
        keep_alive=False,  # stateful component: fresh process per test is not optional here
        log_file=LOG_FILE,
    )
    async with AsyncExitStack() as stack:
        # Bound the connect, not the test body. A stalled handshake otherwise
        # consumes the whole pytest timeout with no diagnostic at all — which
        # is precisely how the webdriver-bidi CI hang presented for hours.
        try:
            async with asyncio.timeout(CONNECT_TIMEOUT):
                connected = await stack.enter_async_context(Client(transport))
        except TimeoutError:
            pytest.fail(
                f"MCP client did not connect within {CONNECT_TIMEOUT}s; "
                f"act's stderr, if it wrote any, is dumped at session end"
            )
        yield connected


@pytest.fixture
async def session(client, tmp_path: Path) -> str:
    """A per-test session, opened against a private `test.db` under
    `tmp_path` via the virtual `open_session` tool, closed via
    `close_session` after the test. Each test therefore gets its own
    process, its own directory, AND its own database file — no two hurl
    files sharing one `mktemp -d`'d database the way the old suite did.
    """
    db_path = str(tmp_path / "test.db")
    opened = await client.call_tool("open_session", {"database_path": db_path})
    sid = json.loads(opened.content[0].text)["id"]
    yield sid
    await client.call_tool("close_session", {"session_id": sid})


@pytest.fixture
def sql_meta(session: str) -> dict:
    """The `_meta` argument-channel payload every real sqlite tool call
    needs. `std:session-id` keeps its `std:` spelling here — the argument
    channel (ACT-MCP §3.2) is deliberately exempt from the `dev.actcore/`
    respelling that governs MCP's transport-level `_meta` field (§3.1);
    writing `dev.actcore/session-id` in this channel would not be recognised.
    """
    return {"std:session-id": session}


@pytest.fixture
def expect_error():
    """Assert a call fails with a specific ACT error kind.

    Exposed as a fixture rather than a plain function so tests never have to
    import from `conftest` — that import only resolves when the test
    directory happens to be on `sys.path`, which is not something to rely on.

    Measured, not assumed. `call-tool` in `act:tools` returns a bare
    `tool-result` with NO `result<>` wrapper — only `list-tools` has one — so
    a guest reporting a failed tool call can only do it through
    `tool-event::error`, which arrives as a result with `is_error` set and the
    kind in `_meta`. **That is the path a tool test will take.**

    The JSON-RPC error path exists for failures that are not the guest's tool
    body: `list-tools`, the session operations, a wasmtime trap, an
    unreachable actor. It raises `mcp.shared.exceptions.McpError` with the
    payload at `exc.error.data`. Session-lifecycle tests reach it; a
    describe-a-missing-table test does not. Both are handled here so callers
    need not care.
    """

    async def _expect(client, tool: str, arguments: dict, kind: str):
        try:
            result = await client.call_tool(tool, arguments, raise_on_error=False)
        except McpError as exc:
            data = getattr(getattr(exc, "error", None), "data", None) or {}
            assert data.get("dev.actcore/error-kind") == kind, (
                f"expected {kind} on the JSON-RPC error path, got {data!r}"
            )
            return

        assert result.is_error, f"expected {tool} to fail, got {result!r}"
        meta = result.meta or {}
        assert meta.get("dev.actcore/error-kind") == kind, (
            f"expected {kind} on the isError path, got {meta!r}"
        )

    return _expect


def pytest_sessionfinish(session, exitstatus):
    """Print act's stderr when the run did not pass.

    `log_file` keeps the audit trail out of the test output, which is right
    for a green run and wrong for every other kind: on an ephemeral CI runner
    nothing ever reads that file. Diagnosing a CI-only hang in this fleet
    cost several rounds of probing that one line of this stream would have
    answered. A hook rather than a fixture finaliser on purpose — fixture
    teardown does not run when the session dies mid-test.
    """
    if exitstatus == 0 or not LOG_FILE.exists():
        return
    text = LOG_FILE.read_text(errors="replace").strip()
    if text:
        print(f"\n--- act stderr ({LOG_FILE}) ---\n{text}")
