import json


async def test_list_tables(client, sql_meta):
    await client.call_tool("execute", {
        "sql": "CREATE TABLE IF NOT EXISTS test_list (id INTEGER PRIMARY KEY)",
        "_meta": sql_meta,
    })

    result = await client.call_tool("list_tables", {"_meta": sql_meta})
    # `list_tables` returns a JSON array, so structured_content is None —
    # same reasoning as `query` (see test_create_and_query.py).
    names = [row["name"] for row in json.loads(result.content[0].text)]
    assert "test_list" in names
