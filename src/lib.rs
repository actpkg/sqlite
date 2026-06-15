use act_sdk::prelude::*;
use ciborium::value::Value as Cv;
use rusqlite::{Connection, params_from_iter, types::Value};
use std::sync::Mutex;

act_sdk::embed_skill!("skill/");

#[cfg(feature = "vec")]
use {rusqlite::ffi::sqlite3_auto_extension, std::sync::Once};

static DB: Mutex<Option<Connection>> = Mutex::new(None);

#[cfg(feature = "vec")]
static VEC_INIT: Once = Once::new();

#[cfg(feature = "vec")]
fn ensure_vec_extension() {
    VEC_INIT.call_once(|| unsafe {
        #[allow(clippy::missing_transmute_annotations)]
        sqlite3_auto_extension(Some(std::mem::transmute(
            sqlite_vec::sqlite3_vec_init as *const (),
        )));
    });
}

fn get_or_open_db(path: &str) -> ActResult<()> {
    let mut guard = DB
        .lock()
        .map_err(|e| ActError::internal(format!("Lock error: {e}")))?;
    if guard.is_none() {
        #[cfg(feature = "vec")]
        ensure_vec_extension();

        let conn = Connection::open(path)
            .map_err(|e| ActError::internal(format!("Cannot open database: {e}")))?;
        conn.execute_batch("PRAGMA journal_mode=WAL;")
            .map_err(|e| ActError::internal(format!("PRAGMA error: {e}")))?;
        *guard = Some(conn);
    }
    Ok(())
}

fn with_db<F, T>(path: &str, f: F) -> ActResult<T>
where
    F: FnOnce(&Connection) -> ActResult<T>,
{
    get_or_open_db(path)?;
    let guard = DB
        .lock()
        .map_err(|e| ActError::internal(format!("Lock error: {e}")))?;
    f(guard.as_ref().unwrap())
}

#[derive(Deserialize, JsonSchema)]
struct Config {
    /// Path to SQLite database file
    database_path: String,
}

// ── Component definition ─────────────────────────────────────────────────────
// Feature flag changes the component name and description only.

#[cfg_attr(not(feature = "vec"), act_component)]
#[cfg_attr(
    feature = "vec",
    act_component(
        name = "sqlite-vec",
        description = "SQLite database operations with vector search (sqlite-vec)"
    )
)]
mod component {
    use super::*;

    /// Execute a SELECT query and return results as structured data.
    #[act_tool(
        description = "Execute a read-only SQL query (SELECT) and return results as array of row objects",
        read_only
    )]
    fn query(
        #[doc = "SQL SELECT query to execute"] sql: String,
        #[doc = "Query parameters as array (optional)"] params: Option<Vec<SqlValue>>,
        ctx: &mut ActContext<Config>,
    ) -> ActResult<Vec<Cv>> {
        let path = ctx.metadata().database_path.clone();
        with_db(&path, |conn| {
            let param_values = cbor_params_to_sqlite(params.as_deref())?;
            let mut stmt = conn
                .prepare(&sql)
                .map_err(|e| ActError::invalid_args(format!("SQL error: {e}")))?;
            let column_names: Vec<String> =
                stmt.column_names().iter().map(|s| s.to_string()).collect();
            let rows: Vec<Cv> = stmt
                .query_map(params_from_iter(param_values.iter()), |row| {
                    let mut pairs: Vec<(Cv, Cv)> = Vec::with_capacity(column_names.len());
                    for (i, name) in column_names.iter().enumerate() {
                        let val: Value = row.get(i)?;
                        pairs.push((Cv::Text(name.clone()), sqlite_value_to_cbor(&val)));
                    }
                    Ok(Cv::Map(pairs))
                })
                .map_err(|e| ActError::internal(format!("Query error: {e}")))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| ActError::internal(format!("Row error: {e}")))?;
            Ok(rows)
        })
    }

    /// Execute a write SQL statement (INSERT, UPDATE, DELETE, CREATE, etc.)
    #[act_tool(
        description = "Execute a write SQL statement (INSERT, UPDATE, DELETE, CREATE TABLE, etc.)"
    )]
    fn execute(
        #[doc = "SQL statement to execute"] sql: String,
        #[doc = "Statement parameters as array (optional)"] params: Option<Vec<SqlValue>>,
        ctx: &mut ActContext<Config>,
    ) -> ActResult<Cv> {
        let path = ctx.metadata().database_path.clone();
        with_db(&path, |conn| {
            let param_values = cbor_params_to_sqlite(params.as_deref())?;
            let affected = conn
                .execute(&sql, params_from_iter(param_values.iter()))
                .map_err(|e| ActError::invalid_args(format!("SQL error: {e}")))?;
            Ok(cbor_obj(vec![
                ("rows_affected", Cv::from(affected as i64)),
                ("last_insert_rowid", Cv::from(conn.last_insert_rowid())),
            ]))
        })
    }

    /// List all tables in the database.
    #[act_tool(description = "List all tables in the SQLite database", read_only)]
    fn list_tables(ctx: &mut ActContext<Config>) -> ActResult<Vec<Cv>> {
        let path = ctx.metadata().database_path.clone();
        with_db(&path, |conn| {
            let mut stmt = conn.prepare(
                "SELECT name, type FROM sqlite_master WHERE type IN ('table', 'view') AND name NOT LIKE 'sqlite_%' ORDER BY name"
            ).map_err(|e| ActError::internal(format!("SQL error: {e}")))?;

            let tables: Vec<Cv> = stmt
                .query_map([], |row| {
                    let name: String = row.get(0)?;
                    let typ: String = row.get(1)?;
                    Ok(cbor_obj(vec![
                        ("name", Cv::from(name)),
                        ("type", Cv::from(typ)),
                    ]))
                })
                .map_err(|e| ActError::internal(format!("Query error: {e}")))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| ActError::internal(format!("Row error: {e}")))?;

            Ok(tables)
        })
    }

    /// Get detailed schema for a specific table.
    #[act_tool(
        description = "Get column names, types, and constraints for a table",
        read_only
    )]
    fn describe_table(
        #[doc = "Table name to describe"] table: String,
        ctx: &mut ActContext<Config>,
    ) -> ActResult<Cv> {
        let path = ctx.metadata().database_path.clone();
        with_db(&path, |conn| {
            let exists: bool = conn.query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master WHERE name = ?1 AND type IN ('table', 'view')",
                [&table],
                |row| row.get(0),
            ).map_err(|e| ActError::internal(format!("SQL error: {e}")))?;

            if !exists {
                return Err(ActError::not_found(format!("Table not found: {table}")));
            }

            let mut stmt = conn
                .prepare(&format!(
                    "PRAGMA table_info('{}')",
                    table.replace('\'', "''")
                ))
                .map_err(|e| ActError::internal(format!("SQL error: {e}")))?;

            let columns: Vec<Cv> = stmt
                .query_map([], |row| {
                    let cid: i64 = row.get(0)?;
                    let name: String = row.get(1)?;
                    let col_type: String = row.get(2)?;
                    let notnull: bool = row.get(3)?;
                    let default: Value = row.get(4)?;
                    let pk: bool = row.get(5)?;
                    Ok(cbor_obj(vec![
                        ("cid", Cv::from(cid)),
                        ("name", Cv::from(name)),
                        ("type", Cv::from(col_type)),
                        ("notnull", Cv::from(notnull)),
                        ("default", sqlite_value_to_cbor(&default)),
                        ("primary_key", Cv::from(pk)),
                    ]))
                })
                .map_err(|e| ActError::internal(format!("Query error: {e}")))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| ActError::internal(format!("Row error: {e}")))?;

            let create_sql: String = conn
                .query_row(
                    "SELECT sql FROM sqlite_master WHERE name = ?1",
                    [&table],
                    |row| row.get(0),
                )
                .unwrap_or_default();

            Ok(cbor_obj(vec![
                ("table", Cv::from(table)),
                ("columns", Cv::Array(columns)),
                ("create_sql", Cv::from(create_sql)),
            ]))
        })
    }

    /// Execute multiple SQL statements in a transaction.
    #[act_tool(description = "Execute multiple SQL statements in a single transaction")]
    fn execute_batch(
        #[doc = "SQL statements separated by semicolons"] sql: String,
        ctx: &mut ActContext<Config>,
    ) -> ActResult<Cv> {
        let path = ctx.metadata().database_path.clone();
        with_db(&path, |conn| {
            conn.execute_batch(&sql)
                .map_err(|e| ActError::invalid_args(format!("SQL error: {e}")))?;
            Ok(cbor_obj(vec![("status", Cv::from("ok"))]))
        })
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// A SQL bind value. Wraps a dynamic CBOR value so a `BLOB` parameter arrives as
/// a byte string. `ciborium::value::Value` has no `JsonSchema`, so this newtype
/// supplies a permissive one.
struct SqlValue(Cv);

impl<'de> serde::Deserialize<'de> for SqlValue {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(SqlValue(Cv::deserialize(deserializer)?))
    }
}

impl schemars::JsonSchema for SqlValue {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "SqlValue".into()
    }
    fn inline_schema() -> bool {
        true
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        // Any SQL value: null / integer / real / text / boolean, or binary as a
        // {"$bytes":"<base64>"} object (host-projected to a CBOR byte string).
        schemars::json_schema!({})
    }
}

/// Build a CBOR map (object) from string-keyed pairs.
fn cbor_obj(pairs: Vec<(&str, Cv)>) -> Cv {
    Cv::Map(
        pairs
            .into_iter()
            .map(|(k, v)| (Cv::Text(k.to_string()), v))
            .collect(),
    )
}

/// rusqlite value → CBOR value. `BLOB` becomes a byte string (no base64).
fn sqlite_value_to_cbor(val: &Value) -> Cv {
    match val {
        Value::Null => Cv::Null,
        Value::Integer(i) => Cv::Integer((*i).into()),
        Value::Real(f) => Cv::Float(*f),
        Value::Text(s) => Cv::Text(s.clone()),
        Value::Blob(b) => Cv::Bytes(b.clone()),
    }
}

/// CBOR bind values → rusqlite values. A byte string binds as a `BLOB`; a numeric
/// array binds as an f32 little-endian blob (for sqlite-vec vectors).
fn cbor_params_to_sqlite(params: Option<&[SqlValue]>) -> ActResult<Vec<Value>> {
    let Some(params) = params else {
        return Ok(vec![]);
    };
    params
        .iter()
        .map(|p| match &p.0 {
            Cv::Null => Ok(Value::Null),
            Cv::Bool(b) => Ok(Value::Integer(if *b { 1 } else { 0 })),
            Cv::Integer(i) => {
                let n: i128 = (*i).into();
                i64::try_from(n)
                    .map(Value::Integer)
                    .map_err(|_| ActError::invalid_args("Integer out of range"))
            }
            Cv::Float(f) => Ok(Value::Real(*f)),
            Cv::Text(s) => Ok(Value::Text(s.clone())),
            Cv::Bytes(b) => Ok(Value::Blob(b.clone())),
            // Numeric array → f32 blob (sqlite-vec vector params).
            Cv::Array(arr) => {
                let floats: Result<Vec<f32>, _> = arr
                    .iter()
                    .map(|v| match v {
                        Cv::Float(f) => Ok(*f as f32),
                        Cv::Integer(i) => {
                            let n: i128 = (*i).into();
                            Ok(n as f32)
                        }
                        _ => Err(ActError::invalid_args("Vector elements must be numbers")),
                    })
                    .collect();
                let bytes: Vec<u8> = floats?.iter().flat_map(|f| f.to_le_bytes()).collect();
                Ok(Value::Blob(bytes))
            }
            _ => Err(ActError::invalid_args(
                "Unsupported param type (use scalars, byte strings, or number arrays)",
            )),
        })
        .collect()
}
