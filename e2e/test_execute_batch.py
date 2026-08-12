async def test_execute_batch(client, sql_meta):
    result = await client.call_tool("execute_batch", {
        "sql": "CREATE TABLE IF NOT EXISTS batch_a (x INT); CREATE TABLE IF NOT EXISTS batch_b (y INT);",
        "_meta": sql_meta,
    })
    assert result.structured_content["status"] == "ok"
