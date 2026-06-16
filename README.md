# SQLite Component

Self-contained SQLite database as an [ACT](https://actcore.dev) WebAssembly component. No system dependencies — SQLite is bundled inside the `.wasm` binary.

## Variants

| Package | OCI Reference | Description |
|---------|--------------|-------------|
| **sqlite** | `ghcr.io/actpkg/sqlite` | Core SQLite operations |
| **sqlite-vec** | `ghcr.io/actpkg/sqlite-vec` | SQLite + [sqlite-vec](https://github.com/asg017/sqlite-vec) vector search |

## Quick Start

```bash
# Install act CLI
npm i -g @actcore/act

# Create a table
act call ghcr.io/actpkg/sqlite:latest execute-batch \
  --args '{"sql":"CREATE TABLE notes (id INTEGER PRIMARY KEY, text TEXT, created_at TEXT DEFAULT CURRENT_TIMESTAMP)"}' \
  --metadata '{"database_path":"/data/notes.db"}' \
  --allow-dir /data:./data

# Insert data
act call ghcr.io/actpkg/sqlite:latest execute \
  --args '{"sql":"INSERT INTO notes (text) VALUES (?1)","params":["Hello from ACT"]}' \
  --metadata '{"database_path":"/data/notes.db"}' \
  --allow-dir /data:./data

# Query
act call ghcr.io/actpkg/sqlite:latest query \
  --args '{"sql":"SELECT * FROM notes"}' \
  --metadata '{"database_path":"/data/notes.db"}' \
  --allow-dir /data:./data
```

## Tools

### query

Execute a read-only SQL query (SELECT) and return results as JSON array.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `sql` | string | yes | SQL SELECT query |
| `params` | array | no | Bind parameters |

### execute

Execute a write SQL statement (INSERT, UPDATE, DELETE, CREATE TABLE, etc.).

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `sql` | string | yes | SQL statement |
| `params` | array | no | Bind parameters |

### list-tables

List all tables and views in the database. No parameters.

### describe-table

Get column names, types, and constraints for a table.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `table` | string | yes | Table name |

### execute-batch

Execute multiple SQL statements in a single transaction.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `sql` | string | yes | SQL statements separated by semicolons |

## Metadata

The component requires `database_path` in metadata — the path to the SQLite database file inside the sandbox.

```json
{"database_path": "/data/mydb.db"}
```

Use `--allow-dir /data:./local-dir` to grant the component access to a host directory.

## Parameterized Queries

Use `?1`, `?2`, ... placeholders with the `params` array:

```bash
act call ghcr.io/actpkg/sqlite:latest execute \
  --args '{"sql":"INSERT INTO users (name, age) VALUES (?1, ?2)","params":["Alice", 30]}' \
  --metadata '{"database_path":"/data/app.db"}' \
  --allow-dir /data:./data
```

Supported parameter types: strings, numbers, booleans, null, and arrays of numbers (converted to f32 blob for sqlite-vec).

## Vector Search (sqlite-vec)

The `sqlite-vec` variant adds vector similarity search via [sqlite-vec](https://github.com/asg017/sqlite-vec).

```bash
# Create a vector table (4-dimensional float vectors)
act call ghcr.io/actpkg/sqlite-vec:latest execute \
  --args '{"sql":"CREATE VIRTUAL TABLE embeddings USING vec0(embedding float[4])"}' \
  --metadata '{"database_path":"/data/vec.db"}' \
  --allow-dir /data:./data

# Insert vectors
act call ghcr.io/actpkg/sqlite-vec:latest execute \
  --args '{"sql":"INSERT INTO embeddings (rowid, embedding) VALUES (?1, ?2)","params":[1, [1.0, 0.0, 0.0, 0.0]]}' \
  --metadata '{"database_path":"/data/vec.db"}' \
  --allow-dir /data:./data

# KNN search
act call ghcr.io/actpkg/sqlite-vec:latest query \
  --args '{"sql":"SELECT rowid, distance FROM embeddings WHERE embedding MATCH ?1 ORDER BY distance LIMIT 5","params":[[0.9, 0.1, 0.0, 0.0]]}' \
  --metadata '{"database_path":"/data/vec.db"}' \
  --allow-dir /data:./data
```

JSON arrays of numbers are automatically converted to f32 blobs for vector parameters.

## Serving

```bash
# ACT-HTTP server
act run --http ghcr.io/actpkg/sqlite:latest \
  --metadata '{"database_path":"/data/app.db"}' \
  --allow-dir /data:./data

# MCP stdio (for Claude, Cursor, etc.)
act run --mcp ghcr.io/actpkg/sqlite:latest \
  --metadata '{"database_path":"/data/app.db"}' \
  --allow-dir /data:./data
```

## Building from Source

Requires [wasi-sdk](https://github.com/WebAssembly/wasi-sdk) and [just](https://github.com/casey/just).

```bash
just build sqlite       # base variant
just build sqlite-vec   # with vector search

just test sqlite        # run e2e tests
just test sqlite-vec

just clippy sqlite      # lint
just clippy sqlite-vec
```

## Publishing

Pushing to `main` publishes a signed component to
`actpkg.dev/<owner>/sqlite` (owner derived from the git remote;
override the full path with the `OCI_REGISTRY` env var). CI signs the image
keylessly with [cosign](https://docs.sigstore.dev/) via GitHub OIDC.

One-time setup: create a Personal Access Token at
[actpkg.dev](https://actpkg.dev) and add it as a repository secret named
**`ACTPKG_TOKEN`** (Settings → Secrets and variables → Actions).

```bash
just publish   # local publish (unsigned); CI signs on push to main
```

## License

MIT OR Apache-2.0
