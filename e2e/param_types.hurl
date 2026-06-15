# Null, float, and boolean params bind correctly under the CBOR value path
# (these exercise the new cbor_params_to_sqlite arms: Null, Float, Bool→Integer).

POST {{baseurl}}/tools/execute
{
  "arguments": {"sql": "CREATE TABLE IF NOT EXISTS types_t (a, b, c, d)"}
}
HTTP 200

POST {{baseurl}}/tools/execute
{
  "arguments": {"sql": "INSERT INTO types_t (a, b, c, d) VALUES (?1, ?2, ?3, ?4)", "params": [null, 3.5, true, false]}
}
HTTP 200
[Asserts]
jsonpath "$.content[0].data.rows_affected" == 1

POST {{baseurl}}/tools/query
{
  "arguments": {"sql": "SELECT a, b, c, d FROM types_t"}
}
HTTP 200
[Asserts]
jsonpath "$.content[0].data[0].a" == null
jsonpath "$.content[0].data[0].b" == 3.5
jsonpath "$.content[0].data[0].c" == 1
jsonpath "$.content[0].data[0].d" == 0
