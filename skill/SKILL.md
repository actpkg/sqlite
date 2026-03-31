---
name: sqlite
description: SQLite database — query, execute, inspect schema
metadata:
  act: {}
---

# SQLite Component

Persistent SQLite database with full SQL support.

## Configuration

Requires `database_path` in metadata — the path to the `.db` file on the component's filesystem.

## Tools

### query
Read-only SELECT queries. Returns JSON array of row objects.

```
query(sql: "SELECT * FROM users WHERE age > ?", params: [18])
→ [{"id": 1, "name": "Alice", "age": 30}, ...]
```

- Always use parameterized queries (`?` placeholders) — never interpolate values into SQL strings
- `params` is optional; omit for queries with no parameters
- Results are JSON: strings, numbers, booleans, null, or base64-encoded blobs

### execute
Write statements: INSERT, UPDATE, DELETE, CREATE TABLE, DROP, ALTER, etc.

```
execute(sql: "CREATE TABLE notes (id INTEGER PRIMARY KEY, text TEXT, created_at TEXT DEFAULT CURRENT_TIMESTAMP)")
execute(sql: "INSERT INTO notes (text) VALUES (?)", params: ["hello"])
→ {"rows_affected": 1, "last_insert_rowid": 1}
```

### list_tables
List all tables and views in the database.

```
list_tables()
→ [{"name": "users", "type": "table"}, {"name": "active_users", "type": "view"}]
```

### describe_table
Get column definitions for a table: name, type, nullable, default, primary key.

```
describe_table(table: "users")
→ [{"name": "id", "type": "INTEGER", "primary_key": true, ...}, ...]
```

## Workflow

1. `list_tables` to see what exists
2. `describe_table` to understand schema
3. `query` to read data
4. `execute` to write data

Create tables before inserting. Use transactions via `execute("BEGIN")` / `execute("COMMIT")` for batch writes.
