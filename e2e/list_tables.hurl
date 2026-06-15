# Ensure table exists first
POST {{baseurl}}/tools/execute
{
  "arguments": {"sql": "CREATE TABLE IF NOT EXISTS test_list (id INTEGER PRIMARY KEY)"}
}
HTTP 200

POST {{baseurl}}/tools/list_tables
{
  "arguments": {}
}
HTTP 200
[Asserts]
jsonpath "$.content[0].data[*].name" contains "test_list"
