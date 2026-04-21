POST {{baseurl}}/tools
HTTP 200
[Asserts]
jsonpath "$.tools[*].name" contains "query"
jsonpath "$.tools[*].name" contains "execute"
jsonpath "$.tools[*].name" contains "list_tables"
jsonpath "$.tools[*].name" contains "describe_table"
jsonpath "$.tools[*].name" contains "execute_batch"
