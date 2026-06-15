# Ensure table exists
POST {{baseurl}}/tools/execute
{
  "arguments": {"sql": "CREATE TABLE IF NOT EXISTS products (id INTEGER PRIMARY KEY, name TEXT NOT NULL, price REAL)"}
}
HTTP 200

# Describe it
POST {{baseurl}}/tools/describe_table
{
  "arguments": {"table": "products"}
}
HTTP 200
[Asserts]
jsonpath "$.content[0].data.columns[*].name" contains "name"
jsonpath "$.content[0].data.columns[*].name" contains "price"

# Non-existent table
POST {{baseurl}}/tools/describe_table
{
  "arguments": {"table": "nonexistent"}
}
HTTP *
[Asserts]
jsonpath "$.error.kind" == "std:not-found"
