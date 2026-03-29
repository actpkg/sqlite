# Create vec0 virtual table
POST {{baseurl}}/tools/execute
{
  "arguments": {"sql": "CREATE VIRTUAL TABLE IF NOT EXISTS embeddings USING vec0(embedding float[4])"},
  "metadata": {"database_path": "{{db_path}}"}
}
HTTP 200

# Insert vectors
POST {{baseurl}}/tools/execute
{
  "arguments": {"sql": "INSERT INTO embeddings (rowid, embedding) VALUES (?1, ?2)", "params": [1, [1.0, 0.0, 0.0, 0.0]]},
  "metadata": {"database_path": "{{db_path}}"}
}
HTTP 200
[Asserts]
jsonpath "$.content[0].data" contains "\"rows_affected\":1"

POST {{baseurl}}/tools/execute
{
  "arguments": {"sql": "INSERT INTO embeddings (rowid, embedding) VALUES (?1, ?2)", "params": [2, [0.0, 1.0, 0.0, 0.0]]},
  "metadata": {"database_path": "{{db_path}}"}
}
HTTP 200

POST {{baseurl}}/tools/execute
{
  "arguments": {"sql": "INSERT INTO embeddings (rowid, embedding) VALUES (?1, ?2)", "params": [3, [0.0, 0.0, 1.0, 0.0]]},
  "metadata": {"database_path": "{{db_path}}"}
}
HTTP 200

# KNN search — find nearest to [1.0, 0.1, 0.0, 0.0]
POST {{baseurl}}/tools/query
{
  "arguments": {"sql": "SELECT rowid, distance FROM embeddings WHERE embedding MATCH ?1 ORDER BY distance LIMIT 2", "params": [[1.0, 0.1, 0.0, 0.0]]},
  "metadata": {"database_path": "{{db_path}}"}
}
HTTP 200
[Asserts]
# Nearest should be rowid 1 (the [1,0,0,0] vector)
jsonpath "$.content[0].data" contains "\"rowid\": 1"

# Verify vec_version() works
POST {{baseurl}}/tools/query
{
  "arguments": {"sql": "SELECT vec_version()"},
  "metadata": {"database_path": "{{db_path}}"}
}
HTTP 200
[Asserts]
jsonpath "$.content[0].data" contains "v0."
