# Two sessions on two different database files must be fully isolated.
POST {{baseurl}}/sessions
{"arguments": {"database_path": "{{db_dir}}/a.db"}}
HTTP 201
[Captures]
sid_a: jsonpath "$.id"

POST {{baseurl}}/sessions
{"arguments": {"database_path": "{{db_dir}}/b.db"}}
HTTP 201
[Captures]
sid_b: jsonpath "$.id"

# Session A: create + insert 'alpha'
POST {{baseurl}}/tools/execute
{"arguments": {"sql": "CREATE TABLE t (v TEXT)"}, "metadata": {"std:session-id": "{{sid_a}}"}}
HTTP 200
POST {{baseurl}}/tools/execute
{"arguments": {"sql": "INSERT INTO t (v) VALUES ('alpha')"}, "metadata": {"std:session-id": "{{sid_a}}"}}
HTTP 200

# Session B: create + insert 'beta'
POST {{baseurl}}/tools/execute
{"arguments": {"sql": "CREATE TABLE t (v TEXT)"}, "metadata": {"std:session-id": "{{sid_b}}"}}
HTTP 200
POST {{baseurl}}/tools/execute
{"arguments": {"sql": "INSERT INTO t (v) VALUES ('beta')"}, "metadata": {"std:session-id": "{{sid_b}}"}}
HTTP 200

# Session A sees ONLY its own row
POST {{baseurl}}/tools/query
{"arguments": {"sql": "SELECT v FROM t"}, "metadata": {"std:session-id": "{{sid_a}}"}}
HTTP 200
[Asserts]
jsonpath "$.content[0].data" count == 1
jsonpath "$.content[0].data[0].v" == "alpha"

# Session B sees ONLY its own row
POST {{baseurl}}/tools/query
{"arguments": {"sql": "SELECT v FROM t"}, "metadata": {"std:session-id": "{{sid_b}}"}}
HTTP 200
[Asserts]
jsonpath "$.content[0].data" count == 1
jsonpath "$.content[0].data[0].v" == "beta"

# Close both
DELETE {{baseurl}}/sessions/{{sid_a}}
HTTP 204
DELETE {{baseurl}}/sessions/{{sid_b}}
HTTP 204
