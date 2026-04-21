# Ensure table exists first
POST {{baseurl}}/tools/execute
{
  "arguments": {"sql": "CREATE TABLE IF NOT EXISTS test_list (id INTEGER PRIMARY KEY)"},
  "metadata": {"database_path": "{{db_path}}"}
}
HTTP 200

POST {{baseurl}}/tools/list_tables
{
  "arguments": {},
  "metadata": {"database_path": "{{db_path}}"}
}
HTTP 200
[Asserts]
jsonpath "$.content[0].data[*].name" contains "test_list"
