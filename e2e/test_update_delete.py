import json


async def test_update_and_delete(client, sql_meta):
    await client.call_tool("execute", {
        "sql": "CREATE TABLE IF NOT EXISTS items (id INTEGER PRIMARY KEY, name TEXT, qty INTEGER)",
        "_meta": sql_meta,
    })

    insert = await client.call_tool("execute", {
        "sql": "INSERT INTO items (name, qty) VALUES ('apple', 10), ('banana', 5), ('cherry', 3)",
        "_meta": sql_meta,
    })
    assert insert.structured_content["rows_affected"] == 3

    update = await client.call_tool("execute", {
        "sql": "UPDATE items SET qty = 20 WHERE name = 'apple'", "_meta": sql_meta,
    })
    assert update.structured_content["rows_affected"] == 1

    verify = await client.call_tool("query", {
        "sql": "SELECT qty FROM items WHERE name = 'apple'", "_meta": sql_meta,
    })
    assert json.loads(verify.content[0].text)[0]["qty"] == 20

    delete = await client.call_tool("execute", {
        "sql": "DELETE FROM items WHERE qty < 10", "_meta": sql_meta,
    })
    assert delete.structured_content["rows_affected"] == 2

    remaining = await client.call_tool("query", {
        "sql": "SELECT COUNT(*) as cnt FROM items", "_meta": sql_meta,
    })
    assert json.loads(remaining.content[0].text)[0]["cnt"] == 1
