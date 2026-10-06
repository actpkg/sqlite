//! Drive the packed component through `act run --mcp` with a real MCP client.
//!
//! This replaces the python fastmcp/pytest suite that still sits next to this
//! file (`test_*.py` + `conftest.py`, kept as the reference for what each test
//! asserts): the tests observe exactly what an agent observes, over the same
//! client stack (`rmcp`) the host bridge itself is built on.
//!
//! sqlite is a session-provider (ACT-SESSIONS): the suite exercises the full
//! session path the way an agent does — the virtual `open_session`/
//! `close_session` tools the MCP adapter synthesises (ACT-MCP §4.1), the
//! session id addressed through the `_meta.std:session-id` argument channel
//! (ACT-MCP §3.2) — not `--session-args` (session-of-1, whose machinery is
//! hidden and already covered by act-cli's own `tests/session_of_1_mcp.rs`).
//!
//! Env: WASM           — path to the packed component (default: the release
//!                       build output, what `just pack` produces);
//!       ACT           — the act invocation (default `act`; `npx
//!                       @actcore/act`, the component justfile's default, also
//!                       works — whitespace-split, like the shlex.split the
//!                       python conftest did);
//!       SQLITE_VARIANT— which variant the justfile packed (`sqlite` or
//!                       `sqlite-vec`); gates the vector-search tests exactly
//!                       as it gated the python ones.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rmcp::{
    ServiceExt,
    model::CallToolRequestParams,
    transport::{ConfigureCommandExt, TokioChildProcess},
};
use serde_json::{Value, json};

/// `().serve(transport)` hands back the client-role service running over the
/// child process: role first, the unit client handler second.
type Client = rmcp::service::RunningService<rmcp::service::RoleClient, ()>;

// ---------------------------------------------------------------------------
// Harness plumbing (the conftest.py analogue)
// ---------------------------------------------------------------------------

fn wasm_path() -> PathBuf {
    PathBuf::from(std::env::var("WASM").unwrap_or_else(|_| {
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../target/wasm32-wasip2/release/component_sqlite.wasm"
        )
        .into()
    }))
}

/// The ACT invocation, honouring the same override the component justfile
/// uses. Its default there is `npx @actcore/act` — two words — which cannot
/// be `argv[0]` for a non-shell spawn, so the value is whitespace-split into
/// program + leading args.
fn act_argv() -> Vec<String> {
    std::env::var("ACT")
        .unwrap_or_else(|_| "act".into())
        .split_whitespace()
        .map(str::to_string)
        .collect()
}

/// A private directory per test, the analogue of pytest's `tmp_path` fixture.
/// sqlite holds a real database file open per session, so every test gets its
/// own process, its own directory, AND its own database file. Unique by
/// pid + tag + nanoseconds; cargo runs tests in parallel threads of one
/// process, so the pid alone does not separate them.
fn temp_dir(tag: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock is after the epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("act-sqlite-e2e-{}-{}-{}", std::process::id(), tag, nanos));
    std::fs::create_dir_all(&dir).expect("create per-test temp dir");
    dir
}

/// The grant the python conftest carried verbatim: a private database
/// directory and `/dev/urandom` (SQLite's WAL journal mode, turned on at
/// connection-open in src/lib.rs, reads OS randomness for the wal-index
/// header salt on every open). The directory, not a single file — sqlite
/// writes a `.lock` sidecar next to the database, and the WAL/journal files
/// alongside it.
///
/// Grants are NOT optional: the default policy mode is `ask` and a headless
/// run degrades it to deny.
fn fs_grant(dir: &Path) -> String {
    json!({
        "wasi:filesystem": {
            "mode": "allowlist",
            "allow": [
                {"path": "/dev/urandom", "mode": "rw"},
                {"path": dir.display().to_string(), "mode": "rw"},
            ],
        }
    })
    .to_string()
}

fn act_command(grant: &str) -> tokio::process::Command {
    let argv = act_argv();
    let mut cmd = tokio::process::Command::new(&argv[0]);
    cmd.args(&argv[1..]);
    cmd.arg("run").arg(wasm_path()).arg("--mcp");
    cmd.arg("--grant").arg(grant);
    cmd
}

async fn connect(grant: &str) -> Client {
    let transport =
        TokioChildProcess::new(act_command(grant)).expect("spawn act run --mcp");
    // No timeout around the handshake: `act run --mcp` instantiates the
    // component before it answers `initialize`, and a tight bound is how a
    // heavy-but-healthy connect turns into a false failure (the python
    // conftest bounded it at 120s for exactly that reason; the harness skill
    // says don't).
    ().serve(transport)
        .await
        .expect("rmcp handshake with act run --mcp")
}

/// A per-test session, opened against a private `test.db` under the test's
/// own directory via the virtual `open_session` tool — the path an agent
/// actually uses. Closed by [`Harness::shutdown`].
struct Harness {
    dir: PathBuf,
    client: Client,
    sid: String,
}

impl Harness {
    async fn new(tag: &str) -> Self {
        let dir = temp_dir(tag);
        let client = connect(&fs_grant(&dir)).await;
        let sid = open_session(&client, &dir.join("test.db")).await;
        Self { dir, client, sid }
    }

    /// The `_meta` argument-channel payload every real sqlite tool call
    /// needs. `std:session-id` keeps its `std:` spelling here — the argument
    /// channel (ACT-MCP §3.2) is deliberately exempt from the
    /// `dev.actcore/` respelling that governs MCP's transport-level `_meta`
    /// field (§3.1); writing `dev.actcore/session-id` in this channel would
    /// not be recognised.
    fn meta(&self) -> Value {
        json!({"std:session-id": self.sid})
    }

    async fn shutdown(self) {
        close_session(&self.client, &self.sid).await;
        self.client.cancel().await.ok();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

async fn open_session(client: &Client, db_path: &Path) -> String {
    let args = json!({"database_path": db_path.display().to_string()})
        .as_object()
        .unwrap()
        .clone();
    let result = client
        .call_tool(CallToolRequestParams::new("open_session").with_arguments(args))
        .await
        .expect("call_tool open_session");
    assert_ne!(result.is_error, Some(true), "open_session failed: {result:?}");
    let opened: Value = serde_json::from_str(&first_text_block(&result).text)
        .expect("open_session reply is a JSON object");
    opened["id"]
        .as_str()
        .expect("open_session reply carries a string id")
        .to_string()
}

async fn close_session(client: &Client, sid: &str) {
    let args = json!({"session_id": sid}).as_object().unwrap().clone();
    let result = client
        .call_tool(CallToolRequestParams::new("close_session").with_arguments(args))
        .await
        .expect("call_tool close_session");
    assert_ne!(result.is_error, Some(true), "close_session failed: {result:?}");
}

async fn call(client: &Client, tool: &str, args: Value) -> rmcp::model::CallToolResult {
    client
        .call_tool(
            CallToolRequestParams::new(tool)
                .with_arguments(args.as_object().expect("tool args are an object").clone()),
        )
        .await
        .expect("call_tool")
}

fn first_text_block(result: &rmcp::model::CallToolResult) -> &rmcp::model::TextContent {
    match result.content.first() {
        Some(rmcp::model::ContentBlock::Text(t)) => t,
        other => panic!("expected the first content block to be Text, got: {other:?}"),
    }
}

/// The value a tool returned as structured content. Python's
/// `result.structured_content[...]` raises on `None`; so does this.
fn structured(result: &rmcp::model::CallToolResult) -> &Value {
    result
        .structured_content
        .as_ref()
        .expect("expected structured_content on this result")
}

/// Python's `==` on parsed JSON numbers accepts `30` and `30.0` alike; so
/// does this. sqlite returns integers for INTEGER/COUNT columns and floats
/// for REAL ones, and the assertions must not care which.
fn assert_num_eq(actual: &Value, expected: f64, what: &str) {
    let n = actual
        .as_f64()
        .unwrap_or_else(|| panic!("{what}: expected a number, got {actual}"));
    assert_eq!(n, expected, "{what}");
}

/// The kind and message of a failed call may arrive on either path: as a
/// JSON-RPC error response (`ErrorData.data`) or as an isError result
/// (`_meta` / text content). The python conftest's `expect_error` fixture
/// handled both; so does this. `call-tool` in `act:tools` returns a bare
/// `tool-result` with NO `result<>` wrapper — only `list-tools` has one — so
/// a guest reporting a failed tool call can only do it through
/// `tool-event::error`, which is the isError path here; the JSON-RPC path
/// stays handled for the failure modes that are not the guest's tool body
/// (`list-tools`, the session operations, a wasmtime trap, an unreachable
/// actor).
async fn error_kind_of(client: &Client, params: CallToolRequestParams) -> Option<(String, String)> {
    match client.call_tool(params).await {
        Err(rmcp::ServiceError::McpError(e)) => {
            let kind = e
                .data
                .as_ref()
                .and_then(|d| d.get("dev.actcore/error-kind"))
                .and_then(|v| v.as_str())
                .map(str::to_string);
            kind.map(|k| (k, e.message.to_string()))
        }
        Ok(result) => {
            assert_eq!(result.is_error, Some(true), "call must fail: {result:?}");
            let kind = result
                .meta
                .as_ref()
                .and_then(|m| m.0.get("dev.actcore/error-kind"))
                .and_then(|v| v.as_str())
                .map(str::to_string);
            let message = result
                .content
                .first()
                .and_then(|b| match b {
                    rmcp::model::ContentBlock::Text(t) => Some(t.text.clone()),
                    _ => None,
                })
                .unwrap_or_default();
            kind.map(|k| (k, message))
        }
        Err(other) => panic!("unexpected transport failure: {other:?}"),
    }
}

/// test_vector_search.py's gate: only the sqlite-vec build registers the
/// vec0 extension (src/lib.rs, `ensure_vec_extension`, behind the `vec`
/// cargo feature), so these tests only make sense against that variant.
/// cargo has no runtime skip, so the gate is an early return — the test then
/// reports as passed rather than skipped, with the reason printed (the one
/// behavioural difference from pytest.skip, deliberate).
fn vec_variant_active() -> bool {
    std::env::var("SQLITE_VARIANT").as_deref() == Ok("sqlite-vec")
}

// ---------------------------------------------------------------------------
// test_info.py
// ---------------------------------------------------------------------------

/// The manifest probe: the packed artifact must declare a name starting with
/// "sqlite". Python's `re.search(r"^sqlite", …)` is an anchored search, not a
/// full match, so this keeps passing for the "sqlite-vec" variant's manifest
/// name too, exactly as the original hurl assertion did for both variants.
/// Also the fast-fail the python `wasm_path` fixture provided — an unpacked
/// wasm (raw `cargo build` output, no `act:component` section) declares no
/// ceiling, every grant is refused as "outside ceiling", and the failures
/// point anywhere but at the missing metadata. The justfile's
/// `test: (pack …)` ordering exists so this test finds a packed artifact.
#[test]
fn manifest_name_starts_with_sqlite() {
    let output = {
        let argv = act_argv();
        let mut cmd = std::process::Command::new(&argv[0]);
        cmd.args(&argv[1..]);
        cmd.args(["inspect", "component-manifest"])
            .arg(wasm_path())
            .output()
            .expect("run act inspect component-manifest")
    };
    assert!(
        output.status.success(),
        "inspect failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let manifest: Value = serde_json::from_slice(&output.stdout).expect("manifest is JSON");
    let name = manifest["std"]["name"]
        .as_str()
        .expect("manifest carries std.name");
    assert!(name.starts_with("sqlite"), "manifest name is {name:?}");
}

// ---------------------------------------------------------------------------
// test_tools.py, test_list_tools.py
// ---------------------------------------------------------------------------

/// test_tools.py — the coarse smoke assertion.
#[tokio::test]
async fn component_exposes_its_tools() {
    let dir = temp_dir("exposes_its_tools");
    let client = connect(&fs_grant(&dir)).await;
    let tools = client.list_tools(None).await.expect("list_tools").tools;
    assert!(tools.len() >= 1, "component must expose at least one tool");
    client.cancel().await.ok();
    let _ = std::fs::remove_dir_all(&dir);
}

/// test_list_tools.py — the five SQL tools by name. The adapter also
/// synthesises open_session/close_session for this session-provider
/// component (ACT-MCP §4.1) — not asserted here, same as the python file,
/// which checked presence via `contains`, never an exact count.
#[tokio::test]
async fn lists_the_five_sql_tools() {
    let dir = temp_dir("lists_five_sql_tools");
    let client = connect(&fs_grant(&dir)).await;
    let tools = client.list_tools(None).await.expect("list_tools").tools;
    let names: Vec<String> = tools.iter().map(|t| t.name.to_string()).collect();
    for expected in ["query", "execute", "list_tables", "describe_table", "execute_batch"] {
        assert!(
            names.iter().any(|n| n == expected),
            "{expected} must be among the tools, got: {names:?}"
        );
    }
    client.cancel().await.ok();
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// test_create_and_query.py
// ---------------------------------------------------------------------------

#[tokio::test]
async fn create_insert_query() {
    let h = Harness::new("create_insert_query").await;

    call(&h.client, "execute", json!({
        "sql": "CREATE TABLE IF NOT EXISTS users (id INTEGER PRIMARY KEY, name TEXT NOT NULL, age INTEGER)",
        "_meta": h.meta(),
    }))
    .await;

    let insert = call(&h.client, "execute", json!({
        "sql": "INSERT INTO users (name, age) VALUES (?1, ?2)",
        "params": ["Alice", 30],
        "_meta": h.meta(),
    }))
    .await;
    assert_num_eq(structured(&insert)["rows_affected"], 1.0, "rows_affected");

    let result = call(&h.client, "query", json!({
        "sql": "SELECT * FROM users",
        "_meta": h.meta(),
    }))
    .await;
    assert_ne!(result.is_error, Some(true), "query failed: {result:?}");
    // `query` returns a JSON array, not an object: structured_content is only
    // populated for a single part whose decoded value is an object (ACT-MCP
    // §2.2) — measured against the packed wasm, not assumed from the mapping
    // table. Explicit guard so a future SDK change that starts populating it
    // doesn't leave this fallback silently stale.
    assert!(
        result.structured_content.is_none(),
        "query must NOT populate structured_content, got: {:?}",
        result.structured_content
    );
    let rows: Value =
        serde_json::from_str(&first_text_block(&result).text).expect("query reply is JSON");
    assert_eq!(rows[0]["name"], "Alice");
    assert_num_eq(&rows[0]["age"], 30.0, "age");

    h.shutdown().await;
}

// ---------------------------------------------------------------------------
// test_describe_table.py
// ---------------------------------------------------------------------------

#[tokio::test]
async fn describe_table_lists_columns() {
    let h = Harness::new("describe_table_lists_columns").await;

    call(&h.client, "execute", json!({
        "sql": "CREATE TABLE IF NOT EXISTS products (id INTEGER PRIMARY KEY, name TEXT NOT NULL, price REAL)",
        "_meta": h.meta(),
    }))
    .await;

    let result = call(&h.client, "describe_table", json!({
        "table": "products",
        "_meta": h.meta(),
    }))
    .await;
    let columns = structured(&result)["columns"]
        .as_array()
        .expect("describe_table returns a columns array");
    let names: Vec<&str> = columns
        .iter()
        .map(|c| c["name"].as_str().expect("column carries a name"))
        .collect();
    assert!(names.contains(&"name"), "columns: {names:?}");
    assert!(names.contains(&"price"), "columns: {names:?}");

    h.shutdown().await;
}

/// A missing table is `std:not-found` — a guest-reported tool error
/// (`tool-event::error`), so it arrives on the isError path.
#[tokio::test]
async fn describe_nonexistent_table_is_not_found() {
    let h = Harness::new("describe_nonexistent_table").await;

    let params = CallToolRequestParams::new("describe_table").with_arguments(
        json!({"table": "nonexistent", "_meta": h.meta()})
            .as_object()
            .unwrap()
            .clone(),
    );
    let (kind, _message) = error_kind_of(&h.client, params)
        .await
        .expect("describe_table on a missing table must fail with a named error kind");
    assert_eq!(kind, "std:not-found");

    h.shutdown().await;
}

// ---------------------------------------------------------------------------
// test_execute_batch.py
// ---------------------------------------------------------------------------

#[tokio::test]
async fn execute_batch() {
    let h = Harness::new("execute_batch").await;

    let result = call(&h.client, "execute_batch", json!({
        "sql": "CREATE TABLE IF NOT EXISTS batch_a (x INT); CREATE TABLE IF NOT EXISTS batch_b (y INT);",
        "_meta": h.meta(),
    }))
    .await;
    assert_eq!(structured(&result)["status"], "ok");

    h.shutdown().await;
}

// ---------------------------------------------------------------------------
// test_blob.py
// ---------------------------------------------------------------------------

/// Raw bytes passed as a `{"$bytes": …}` param store as a real BLOB, and read
/// back the same way on the JSON path — the host's byte-string projection,
/// not a sqlite-specific encoding.
#[tokio::test]
async fn blob_roundtrip() {
    let h = Harness::new("blob_roundtrip").await;

    call(&h.client, "execute", json!({
        "sql": "CREATE TABLE IF NOT EXISTS blobs (id INTEGER PRIMARY KEY, data BLOB)",
        "_meta": h.meta(),
    }))
    .await;

    let insert = call(&h.client, "execute", json!({
        "sql": "INSERT INTO blobs (data) VALUES (?1)",
        "params": [{"$bytes": "//79"}],
        "_meta": h.meta(),
    }))
    .await;
    assert_num_eq(structured(&insert)["rows_affected"], 1.0, "rows_affected");

    let result = call(&h.client, "query", json!({
        "sql": "SELECT data FROM blobs WHERE id = 1",
        "_meta": h.meta(),
    }))
    .await;
    let rows: Value =
        serde_json::from_str(&first_text_block(&result).text).expect("query reply is JSON");
    assert_eq!(rows[0]["data"]["$bytes"], "//79");

    h.shutdown().await;
}

// ---------------------------------------------------------------------------
// test_isolation.py
// ---------------------------------------------------------------------------

/// Two sessions on two different database files must be fully isolated. This
/// opens two sessions in one test — which is exactly the path every other
/// test here already drives (one session each), so isolation needs no second
/// process and no second port.
#[tokio::test]
async fn two_sessions_two_databases_are_isolated() {
    let dir = temp_dir("isolation");
    let client = connect(&fs_grant(&dir)).await;
    let sid_a = open_session(&client, &dir.join("a.db")).await;
    let sid_b = open_session(&client, &dir.join("b.db")).await;

    let meta_a = json!({"std:session-id": sid_a});
    let meta_b = json!({"std:session-id": sid_b});

    // Session A: create + insert 'alpha'
    call(&client, "execute", json!({"sql": "CREATE TABLE t (v TEXT)", "_meta": meta_a})).await;
    call(&client, "execute", json!({"sql": "INSERT INTO t (v) VALUES ('alpha')", "_meta": meta_a})).await;

    // Session B: create + insert 'beta'
    call(&client, "execute", json!({"sql": "CREATE TABLE t (v TEXT)", "_meta": meta_b})).await;
    call(&client, "execute", json!({"sql": "INSERT INTO t (v) VALUES ('beta')", "_meta": meta_b})).await;

    // Session A sees ONLY its own row
    let result_a = call(&client, "query", json!({"sql": "SELECT v FROM t", "_meta": meta_a})).await;
    let rows_a: Value =
        serde_json::from_str(&first_text_block(&result_a).text).expect("query reply is JSON");
    assert_eq!(rows_a.as_array().expect("rows are an array").len(), 1);
    assert_eq!(rows_a[0]["v"], "alpha");

    // Session B sees ONLY its own row
    let result_b = call(&client, "query", json!({"sql": "SELECT v FROM t", "_meta": meta_b})).await;
    let rows_b: Value =
        serde_json::from_str(&first_text_block(&result_b).text).expect("query reply is JSON");
    assert_eq!(rows_b.as_array().expect("rows are an array").len(), 1);
    assert_eq!(rows_b[0]["v"], "beta");

    close_session(&client, &sid_a).await;
    close_session(&client, &sid_b).await;
    client.cancel().await.ok();
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// test_list_tables.py
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_tables() {
    let h = Harness::new("list_tables").await;

    call(&h.client, "execute", json!({
        "sql": "CREATE TABLE IF NOT EXISTS test_list (id INTEGER PRIMARY KEY)",
        "_meta": h.meta(),
    }))
    .await;

    let result = call(&h.client, "list_tables", json!({"_meta": h.meta()})).await;
    // `list_tables` returns a JSON array, so structured_content is None —
    // same reasoning as `query` (see create_insert_query).
    let rows: Value =
        serde_json::from_str(&first_text_block(&result).text).expect("list_tables reply is JSON");
    let names: Vec<&str> = rows
        .as_array()
        .expect("list_tables returns an array")
        .iter()
        .map(|row| row["name"].as_str().expect("row carries a name"))
        .collect();
    assert!(names.contains(&"test_list"), "tables: {names:?}");

    h.shutdown().await;
}

// ---------------------------------------------------------------------------
// test_param_types.py
// ---------------------------------------------------------------------------

/// Null, float, and boolean params bind correctly under the CBOR value path
/// (the cbor_params_to_sqlite arms: Null, Float, Bool→Integer).
#[tokio::test]
async fn null_float_bool_params() {
    let h = Harness::new("null_float_bool_params").await;

    call(&h.client, "execute", json!({
        "sql": "CREATE TABLE IF NOT EXISTS types_t (a, b, c, d)",
        "_meta": h.meta(),
    }))
    .await;

    let insert = call(&h.client, "execute", json!({
        "sql": "INSERT INTO types_t (a, b, c, d) VALUES (?1, ?2, ?3, ?4)",
        "params": [null, 3.5, true, false],
        "_meta": h.meta(),
    }))
    .await;
    assert_num_eq(structured(&insert)["rows_affected"], 1.0, "rows_affected");

    let result = call(&h.client, "query", json!({
        "sql": "SELECT a, b, c, d FROM types_t",
        "_meta": h.meta(),
    }))
    .await;
    let rows: Value =
        serde_json::from_str(&first_text_block(&result).text).expect("query reply is JSON");
    let row = &rows[0];
    assert!(row["a"].is_null(), "a must be null, got {}", row["a"]);
    assert_num_eq(&row["b"], 3.5, "b");
    // Booleans bind as SQLite INTEGER 0/1 and read back as such — not JSON
    // true/false.
    assert_num_eq(&row["c"], 1.0, "c");
    assert_num_eq(&row["d"], 0.0, "d");

    h.shutdown().await;
}

// ---------------------------------------------------------------------------
// test_parameterized.py
// ---------------------------------------------------------------------------

#[tokio::test]
async fn parameterized_insert_and_query() {
    let h = Harness::new("parameterized_insert_and_query").await;

    call(&h.client, "execute", json!({
        "sql": "CREATE TABLE IF NOT EXISTS params_test (id INTEGER PRIMARY KEY, val TEXT)",
        "_meta": h.meta(),
    }))
    .await;

    let insert = call(&h.client, "execute", json!({
        "sql": "INSERT INTO params_test (val) VALUES (?1)",
        "params": ["hello"],
        "_meta": h.meta(),
    }))
    .await;
    assert_num_eq(structured(&insert)["rows_affected"], 1.0, "rows_affected");

    let result = call(&h.client, "query", json!({
        "sql": "SELECT val FROM params_test WHERE val = ?1",
        "params": ["hello"],
        "_meta": h.meta(),
    }))
    .await;
    let rows: Value =
        serde_json::from_str(&first_text_block(&result).text).expect("query reply is JSON");
    assert_eq!(rows[0]["val"], "hello");

    let multi = call(&h.client, "execute", json!({
        "sql": "INSERT INTO params_test (val) VALUES (?1), (?2)",
        "params": ["foo", "bar"],
        "_meta": h.meta(),
    }))
    .await;
    assert_num_eq(structured(&multi)["rows_affected"], 2.0, "rows_affected");

    h.shutdown().await;
}

// ---------------------------------------------------------------------------
// test_update_delete.py
// ---------------------------------------------------------------------------

#[tokio::test]
async fn update_and_delete() {
    let h = Harness::new("update_and_delete").await;

    call(&h.client, "execute", json!({
        "sql": "CREATE TABLE IF NOT EXISTS items (id INTEGER PRIMARY KEY, name TEXT, qty INTEGER)",
        "_meta": h.meta(),
    }))
    .await;

    let insert = call(&h.client, "execute", json!({
        "sql": "INSERT INTO items (name, qty) VALUES ('apple', 10), ('banana', 5), ('cherry', 3)",
        "_meta": h.meta(),
    }))
    .await;
    assert_num_eq(structured(&insert)["rows_affected"], 3.0, "rows_affected");

    let update = call(&h.client, "execute", json!({
        "sql": "UPDATE items SET qty = 20 WHERE name = 'apple'",
        "_meta": h.meta(),
    }))
    .await;
    assert_num_eq(structured(&update)["rows_affected"], 1.0, "rows_affected");

    let verify = call(&h.client, "query", json!({
        "sql": "SELECT qty FROM items WHERE name = 'apple'",
        "_meta": h.meta(),
    }))
    .await;
    let rows: Value =
        serde_json::from_str(&first_text_block(&verify).text).expect("query reply is JSON");
    assert_num_eq(&rows[0]["qty"], 20.0, "qty");

    let delete = call(&h.client, "execute", json!({
        "sql": "DELETE FROM items WHERE qty < 10",
        "_meta": h.meta(),
    }))
    .await;
    assert_num_eq(structured(&delete)["rows_affected"], 2.0, "rows_affected");

    let remaining = call(&h.client, "query", json!({
        "sql": "SELECT COUNT(*) as cnt FROM items",
        "_meta": h.meta(),
    }))
    .await;
    let rows: Value =
        serde_json::from_str(&first_text_block(&remaining).text).expect("query reply is JSON");
    assert_num_eq(&rows[0]["cnt"], 1.0, "cnt");

    h.shutdown().await;
}

// ---------------------------------------------------------------------------
// test_vector_search.py — sqlite-vec variant only
// ---------------------------------------------------------------------------

#[tokio::test]
async fn vec0_knn_search() {
    if !vec_variant_active() {
        eprintln!(
            "skipped: vec0 virtual table needs the sqlite-vec build (`just test sqlite-vec`)"
        );
        return;
    }

    let h = Harness::new("vec0_knn_search").await;

    call(&h.client, "execute", json!({
        "sql": "CREATE VIRTUAL TABLE IF NOT EXISTS embeddings USING vec0(embedding float[4])",
        "_meta": h.meta(),
    }))
    .await;

    let insert = call(&h.client, "execute", json!({
        "sql": "INSERT INTO embeddings (rowid, embedding) VALUES (?1, ?2)",
        "params": [1, [1.0, 0.0, 0.0, 0.0]],
        "_meta": h.meta(),
    }))
    .await;
    assert_num_eq(structured(&insert)["rows_affected"], 1.0, "rows_affected");

    call(&h.client, "execute", json!({
        "sql": "INSERT INTO embeddings (rowid, embedding) VALUES (?1, ?2)",
        "params": [2, [0.0, 1.0, 0.0, 0.0]],
        "_meta": h.meta(),
    }))
    .await;
    call(&h.client, "execute", json!({
        "sql": "INSERT INTO embeddings (rowid, embedding) VALUES (?1, ?2)",
        "params": [3, [0.0, 0.0, 1.0, 0.0]],
        "_meta": h.meta(),
    }))
    .await;

    // KNN search — find nearest to [1.0, 0.1, 0.0, 0.0]. Nearest should be
    // rowid 1 (the [1,0,0,0] vector).
    let result = call(&h.client, "query", json!({
        "sql": "SELECT rowid, distance FROM embeddings WHERE embedding MATCH ?1 ORDER BY distance LIMIT 2",
        "params": [[1.0, 0.1, 0.0, 0.0]],
        "_meta": h.meta(),
    }))
    .await;
    let rows: Value =
        serde_json::from_str(&first_text_block(&result).text).expect("query reply is JSON");
    assert_num_eq(&rows[0]["rowid"], 1.0, "rowid");

    h.shutdown().await;
}

#[tokio::test]
async fn vec_version() {
    if !vec_variant_active() {
        eprintln!(
            "skipped: vec0 virtual table needs the sqlite-vec build (`just test sqlite-vec`)"
        );
        return;
    }

    let h = Harness::new("vec_version").await;

    let result = call(&h.client, "query", json!({
        "sql": "SELECT vec_version()",
        "_meta": h.meta(),
    }))
    .await;
    let rows: Value =
        serde_json::from_str(&first_text_block(&result).text).expect("query reply is JSON");
    let version = rows[0]["vec_version()"]
        .as_str()
        .expect("vec_version() returns a string");
    assert!(version.starts_with("v0."), "vec_version() is {version:?}");

    h.shutdown().await;
}
