import json


async def test_create_insert_query(client, sql_meta):
    await client.call_tool("execute", {
        "sql": "CREATE TABLE IF NOT EXISTS users (id INTEGER PRIMARY KEY, name TEXT NOT NULL, age INTEGER)",
        "_meta": sql_meta,
    })

    insert = await client.call_tool("execute", {
        "sql": "INSERT INTO users (name, age) VALUES (?1, ?2)",
        "params": ["Alice", 30],
        "_meta": sql_meta,
    })
    assert insert.structured_content["rows_affected"] == 1

    result = await client.call_tool("query", {"sql": "SELECT * FROM users", "_meta": sql_meta})
    # `query` returns a JSON array, not an object: structured_content is only
    # populated for a single part whose decoded value is an object (ACT-MCP
    # §2.2) — measured against the packed wasm, not assumed from the mapping
    # table. Explicit guard so a future SDK change that starts populating it
    # doesn't leave this fallback silently stale.
    assert result.structured_content is None
    rows = json.loads(result.content[0].text)
    assert rows[0]["name"] == "Alice"
    assert rows[0]["age"] == 30
