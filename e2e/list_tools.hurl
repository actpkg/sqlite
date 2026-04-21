POST {{baseurl}}/tools
HTTP 200
[Asserts]
jsonpath "$.tools[*].name" contains "query"
jsonpath "$.tools[*].name" contains "execute"
jsonpath "$.tools[*].name" "list_tables"
jsonpath "$.tools[*].name" "describe_table"
jsonpath "$.tools[*].name" "execute_batch"
