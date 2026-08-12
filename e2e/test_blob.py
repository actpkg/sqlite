import json


async def test_blob_roundtrip(client, sql_meta):
    """Raw bytes passed as a `{"$bytes": …}` param store as a real BLOB, and
    read back the same way on the JSON path — the host's byte-string
    projection, not a sqlite-specific encoding.
    """
    await client.call_tool("execute", {
        "sql": "CREATE TABLE IF NOT EXISTS blobs (id INTEGER PRIMARY KEY, data BLOB)",
        "_meta": sql_meta,
    })

    insert = await client.call_tool("execute", {
        "sql": "INSERT INTO blobs (data) VALUES (?1)",
        "params": [{"$bytes": "//79"}],
        "_meta": sql_meta,
    })
    assert insert.structured_content["rows_affected"] == 1

    result = await client.call_tool("query", {
        "sql": "SELECT data FROM blobs WHERE id = 1", "_meta": sql_meta,
    })
    rows = json.loads(result.content[0].text)
    assert rows[0]["data"]["$bytes"] == "//79"
