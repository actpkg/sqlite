import json


async def test_null_float_bool_params(client, sql_meta):
    """Null, float, and boolean params bind correctly under the CBOR value
    path (the cbor_params_to_sqlite arms: Null, Float, Bool→Integer).
    """
    await client.call_tool("execute", {
        "sql": "CREATE TABLE IF NOT EXISTS types_t (a, b, c, d)", "_meta": sql_meta,
    })

    insert = await client.call_tool("execute", {
        "sql": "INSERT INTO types_t (a, b, c, d) VALUES (?1, ?2, ?3, ?4)",
        "params": [None, 3.5, True, False],
        "_meta": sql_meta,
    })
    assert insert.structured_content["rows_affected"] == 1

    result = await client.call_tool("query", {
        "sql": "SELECT a, b, c, d FROM types_t", "_meta": sql_meta,
    })
    row = json.loads(result.content[0].text)[0]
    assert row["a"] is None
    assert row["b"] == 3.5
    # Booleans bind as SQLite INTEGER 0/1 and read back as such — not JSON
    # true/false.
    assert row["c"] == 1
    assert row["d"] == 0
