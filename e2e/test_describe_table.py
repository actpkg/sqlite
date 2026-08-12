async def test_describe_table_lists_columns(client, sql_meta):
    await client.call_tool("execute", {
        "sql": "CREATE TABLE IF NOT EXISTS products (id INTEGER PRIMARY KEY, name TEXT NOT NULL, price REAL)",
        "_meta": sql_meta,
    })

    result = await client.call_tool("describe_table", {"table": "products", "_meta": sql_meta})
    names = [c["name"] for c in result.structured_content["columns"]]
    assert "name" in names
    assert "price" in names


async def test_describe_nonexistent_table_is_not_found(client, sql_meta, expect_error):
    await expect_error(
        client, "describe_table", {"table": "nonexistent", "_meta": sql_meta}, "std:not-found",
    )
