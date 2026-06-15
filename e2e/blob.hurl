# Create a table with a BLOB column
POST {{baseurl}}/tools/execute
{
  "arguments": {"sql": "CREATE TABLE IF NOT EXISTS blobs (id INTEGER PRIMARY KEY, data BLOB)"},
  "metadata": {"database_path": "{{db_path}}"}
}
HTTP 200

# Insert raw bytes passed as a {"$bytes":…} param → stored as a real BLOB
POST {{baseurl}}/tools/execute
{
  "arguments": {"sql": "INSERT INTO blobs (data) VALUES (?1)", "params": [{"$bytes": "//79"}]},
  "metadata": {"database_path": "{{db_path}}"}
}
HTTP 200
[Asserts]
jsonpath "$.content[0].data.rows_affected" == 1

# Read it back → the BLOB column returns as {"$bytes":…} on the JSON path
POST {{baseurl}}/tools/query
{
  "arguments": {"sql": "SELECT data FROM blobs WHERE id = 1"},
  "metadata": {"database_path": "{{db_path}}"}
}
HTTP 200
[Asserts]
jsonpath "$.content[0].data[0].data['$bytes']" == "//79"
