import json
import os
import pytest

# Only the sqlite-vec build registers the vec0 extension (src/lib.rs,
# `ensure_vec_extension`, behind the `vec` cargo feature); the plain sqlite
# build has no vec0 module, so this file only makes sense against that
# variant. `SQLITE_VARIANT` is set by the justfile's `test` recipe
# (`just test sqlite-vec`) — see conftest.py.
pytestmark = pytest.mark.skipif(
    os.environ.get("SQLITE_VARIANT") != "sqlite-vec",
    reason="vec0 virtual table needs the sqlite-vec build (`just test sqlite-vec`)",
)


async def test_vec0_knn_search(client, sql_meta):
    await client.call_tool("execute", {
        "sql": "CREATE VIRTUAL TABLE IF NOT EXISTS embeddings USING vec0(embedding float[4])",
        "_meta": sql_meta,
    })

    insert = await client.call_tool("execute", {
        "sql": "INSERT INTO embeddings (rowid, embedding) VALUES (?1, ?2)",
        "params": [1, [1.0, 0.0, 0.0, 0.0]],
        "_meta": sql_meta,
    })
    assert insert.structured_content["rows_affected"] == 1

    await client.call_tool("execute", {
        "sql": "INSERT INTO embeddings (rowid, embedding) VALUES (?1, ?2)",
        "params": [2, [0.0, 1.0, 0.0, 0.0]],
        "_meta": sql_meta,
    })
    await client.call_tool("execute", {
        "sql": "INSERT INTO embeddings (rowid, embedding) VALUES (?1, ?2)",
        "params": [3, [0.0, 0.0, 1.0, 0.0]],
        "_meta": sql_meta,
    })

    # KNN search — find nearest to [1.0, 0.1, 0.0, 0.0]. Nearest should be
    # rowid 1 (the [1,0,0,0] vector).
    result = await client.call_tool("query", {
        "sql": "SELECT rowid, distance FROM embeddings WHERE embedding MATCH ?1 ORDER BY distance LIMIT 2",
        "params": [[1.0, 0.1, 0.0, 0.0]],
        "_meta": sql_meta,
    })
    rows = json.loads(result.content[0].text)
    assert rows[0]["rowid"] == 1


async def test_vec_version(client, sql_meta):
    result = await client.call_tool("query", {"sql": "SELECT vec_version()", "_meta": sql_meta})
    row = json.loads(result.content[0].text)[0]
    assert row["vec_version()"].startswith("v0.")
