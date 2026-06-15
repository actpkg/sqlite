# Setup
POST {{baseurl}}/tools/execute
{"arguments": {"sql": "CREATE TABLE IF NOT EXISTS params_test (id INTEGER PRIMARY KEY, val TEXT)"}}
HTTP 200

# Parameterized insert
POST {{baseurl}}/tools/execute
{"arguments": {"sql": "INSERT INTO params_test (val) VALUES (?1)", "params": ["hello"]}}
HTTP 200
[Asserts]
jsonpath "$.content[0].data.rows_affected" == 1

# Parameterized query
POST {{baseurl}}/tools/query
{"arguments": {"sql": "SELECT val FROM params_test WHERE val = ?1", "params": ["hello"]}}
HTTP 200
[Asserts]
jsonpath "$.content[0].data[0].val" == "hello"

# Multiple params
POST {{baseurl}}/tools/execute
{"arguments": {"sql": "INSERT INTO params_test (val) VALUES (?1), (?2)", "params": ["foo", "bar"]}}
HTTP 200
[Asserts]
jsonpath "$.content[0].data.rows_affected" == 2
