import json


async def test_two_sessions_two_databases_are_isolated(client, tmp_path):
    """Two sessions on two different database files must be fully isolated.

    Previously this needed a second `act` process started WITHOUT
    `--session-args` (the justfile's "Mode 2", against a second port) to
    exercise the full session-provider path — session-of-1 pins every call
    to one pre-opened session, so it could never have tested this. Now every
    test already drives that same path (see conftest.py's `session`
    fixture), so isolation is just two sessions opened in one test — no
    second process, no second port.
    """
    a_open = await client.call_tool("open_session", {"database_path": str(tmp_path / "a.db")})
    sid_a = json.loads(a_open.content[0].text)["id"]
    b_open = await client.call_tool("open_session", {"database_path": str(tmp_path / "b.db")})
    sid_b = json.loads(b_open.content[0].text)["id"]

    meta_a = {"std:session-id": sid_a}
    meta_b = {"std:session-id": sid_b}

    # Session A: create + insert 'alpha'
    await client.call_tool("execute", {"sql": "CREATE TABLE t (v TEXT)", "_meta": meta_a})
    await client.call_tool("execute", {"sql": "INSERT INTO t (v) VALUES ('alpha')", "_meta": meta_a})

    # Session B: create + insert 'beta'
    await client.call_tool("execute", {"sql": "CREATE TABLE t (v TEXT)", "_meta": meta_b})
    await client.call_tool("execute", {"sql": "INSERT INTO t (v) VALUES ('beta')", "_meta": meta_b})

    # Session A sees ONLY its own row
    result_a = await client.call_tool("query", {"sql": "SELECT v FROM t", "_meta": meta_a})
    rows_a = json.loads(result_a.content[0].text)
    assert len(rows_a) == 1
    assert rows_a[0]["v"] == "alpha"

    # Session B sees ONLY its own row
    result_b = await client.call_tool("query", {"sql": "SELECT v FROM t", "_meta": meta_b})
    rows_b = json.loads(result_b.content[0].text)
    assert len(rows_b) == 1
    assert rows_b[0]["v"] == "beta"

    await client.call_tool("close_session", {"session_id": sid_a})
    await client.call_tool("close_session", {"session_id": sid_b})
