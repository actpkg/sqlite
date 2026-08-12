import json


async def test_parameterized_insert_and_query(client, sql_meta):
    await client.call_tool("execute", {
        "sql": "CREATE TABLE IF NOT EXISTS params_test (id INTEGER PRIMARY KEY, val TEXT)",
        "_meta": sql_meta,
    })

    insert = await client.call_tool("execute", {
        "sql": "INSERT INTO params_test (val) VALUES (?1)", "params": ["hello"], "_meta": sql_meta,
    })
    assert insert.structured_content["rows_affected"] == 1

    result = await client.call_tool("query", {
        "sql": "SELECT val FROM params_test WHERE val = ?1", "params": ["hello"], "_meta": sql_meta,
    })
    rows = json.loads(result.content[0].text)
    assert rows[0]["val"] == "hello"

    multi = await client.call_tool("execute", {
        "sql": "INSERT INTO params_test (val) VALUES (?1), (?2)",
        "params": ["foo", "bar"],
        "_meta": sql_meta,
    })
    assert multi.structured_content["rows_affected"] == 2
