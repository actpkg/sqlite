async def test_lists_the_five_sql_tools(client):
    # The adapter also synthesises open_session/close_session for this
    # session-provider component (ACT-MCP §4.1) — not asserted here, same as
    # the original hurl file, which checked presence via `contains`, never
    # an exact count.
    tools = await client.list_tools()
    names = [t.name for t in tools]
    assert "query" in names
    assert "execute" in names
    assert "list_tables" in names
    assert "describe_table" in names
    assert "execute_batch" in names
