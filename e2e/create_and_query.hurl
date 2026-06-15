# Create table
POST {{baseurl}}/tools/execute
{
  "arguments": {"sql": "CREATE TABLE IF NOT EXISTS users (id INTEGER PRIMARY KEY, name TEXT NOT NULL, age INTEGER)"}
}
HTTP 200

# Insert row
POST {{baseurl}}/tools/execute
{
  "arguments": {"sql": "INSERT INTO users (name, age) VALUES (?1, ?2)", "params": ["Alice", 30]}
}
HTTP 200
[Asserts]
jsonpath "$.content[0].data.rows_affected" == 1

# Query
POST {{baseurl}}/tools/query
{
  "arguments": {"sql": "SELECT * FROM users"}
}
HTTP 200
[Asserts]
jsonpath "$.content[0].data[0].name" == "Alice"
jsonpath "$.content[0].data[0].age" == 30
