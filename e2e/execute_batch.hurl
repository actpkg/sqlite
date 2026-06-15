POST {{baseurl}}/tools/execute_batch
{
  "arguments": {"sql": "CREATE TABLE IF NOT EXISTS batch_a (x INT); CREATE TABLE IF NOT EXISTS batch_b (y INT);"}
}
HTTP 200
[Asserts]
jsonpath "$.content[0].data.status" == "ok"
