//! Operation and result types for the D1 binding (SQLite)
//!
//! The JSON form of these types is the contract between the runtimes, the
//! runner and the CLI: `docs/d1.md` maps it to the D1 Worker API.

use super::sql::SqlPrimitive;

/// One SQL statement; `params[0]` binds `?1`
///
/// SQLite has no array type, so a parameter is a [`SqlPrimitive`], never an
/// array. The runner stores a boolean as the integer 0 or 1.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct D1Statement {
    pub sql: String,
    #[serde(default)]
    pub params: Vec<SqlPrimitive>,
}

/// The rows a query returns to the guest
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum D1Rows {
    /// Every row (`all`, `run`, `raw`)
    All,
    /// The first row only (`first`). The runner still runs a statement that
    /// writes to its end, so all its changes are applied.
    First,
}

/// Database operation types for the D1 binding
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum D1Op {
    /// One statement, in its own transaction
    Query {
        statement: D1Statement,
        rows: D1Rows,
    },
    /// Statements in order, in one transaction: a failure rolls back all of them
    Batch { statements: Vec<D1Statement> },
    /// Statements without parameters, each in its own transaction. Execution
    /// stops at the first error; the statements before it stay committed.
    Exec { sql: String },
}

/// Measurements of one statement, with the D1 field names
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct D1Meta {
    /// Execution time in milliseconds
    pub duration: f64,
    /// Rows changed by INSERT, UPDATE or DELETE (`sqlite3_changes`)
    pub changes: u64,
    /// Rowid of the last successful INSERT on this connection
    pub last_row_id: i64,
    /// True when the statement is not read-only (`sqlite3_stmt_readonly`)
    pub changed_db: bool,
    /// Database size in bytes after the statement
    pub size_after: u64,
}

/// Result of one statement: column names in SQL order, then one value per
/// column for each row
///
/// Rows are lists, not objects, so duplicate column names survive until the
/// runtime builds the guest objects.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct D1StatementResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<SqlPrimitive>>,
    pub meta: D1Meta,
}

/// Why a D1 operation failed
///
/// Every kind is a definite outcome: the failing transaction rolled back or
/// never started. In `exec`, the statements before the failing one stay
/// committed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum D1ErrorKind {
    /// SQLite rejected the statement (syntax, constraint, denied operation)
    Sql,
    /// A parameter or value has a type D1 does not accept; nothing ran
    Type,
    /// The number of values does not match the parameters; nothing ran
    Range,
    /// The deadline stopped the statement
    Interrupted,
    /// The runner refused the call before running it (full queue, size or
    /// disk limit); a retry later can succeed
    Overloaded,
    /// The binding has no usable database
    Unavailable,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct D1Error {
    pub kind: D1ErrorKind,
    /// Message for the guest, without file paths or credentials
    pub message: String,
    /// Index of the failing statement in a batch or an exec
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub statement: Option<usize>,
}

/// Result from a D1 operation, one variant per [`D1Op`] variant
///
/// Tagged with `type`, because [`D1Error`] has a `kind` field.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum D1Result {
    Query(D1StatementResult),
    Batch { results: Vec<D1StatementResult> },
    Exec { count: u32, duration: f64 },
    Error(D1Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::test_support::round_trip;

    #[test]
    fn query_op_round_trips() {
        let json = r#"{"kind":"query","statement":{"sql":"SELECT ?1, ?2","params":[42,{"$bytes":"AQI="}]},"rows":"first"}"#;

        assert_eq!(round_trip::<D1Op>(json), json);
    }

    #[test]
    fn statement_params_default_to_empty() {
        let op: D1Op =
            serde_json::from_str(r#"{"kind":"query","statement":{"sql":"SELECT 1"},"rows":"all"}"#)
                .unwrap();

        match op {
            D1Op::Query { statement, rows } => {
                assert!(statement.params.is_empty());
                assert_eq!(rows, D1Rows::All);
            }
            other => panic!("expected Query, got {:?}", other),
        }
    }

    #[test]
    fn statement_rejects_array_param() {
        let err = serde_json::from_str::<D1Statement>(r#"{"sql":"SELECT ?","params":[[1,2]]}"#)
            .unwrap_err();

        assert!(!err.to_string().is_empty());
    }

    #[test]
    fn batch_and_exec_ops_round_trip() {
        let batch = r#"{"kind":"batch","statements":[{"sql":"INSERT INTO t VALUES (?1)","params":[1]},{"sql":"INSERT INTO t VALUES (?1)","params":[2]}]}"#;
        let exec = r#"{"kind":"exec","sql":"CREATE TABLE t (a);\nINSERT INTO t VALUES (1);"}"#;

        assert_eq!(round_trip::<D1Op>(batch), batch);
        assert_eq!(round_trip::<D1Op>(exec), exec);
    }

    #[test]
    fn query_result_keeps_duplicate_columns() {
        let json = r#"{"type":"query","columns":["id","id"],"rows":[[1,2]],"meta":{"duration":0.5,"changes":0,"last_row_id":0,"changed_db":false,"size_after":8192}}"#;

        let result: D1Result = serde_json::from_str(json).unwrap();

        match &result {
            D1Result::Query(statement) => {
                assert_eq!(statement.columns, ["id", "id"]);
                assert_eq!(statement.rows[0].len(), 2);
            }
            other => panic!("expected Query, got {:?}", other),
        }

        assert_eq!(serde_json::to_string(&result).unwrap(), json);
    }

    #[test]
    fn error_result_round_trips() {
        let json = r#"{"type":"error","kind":"sql","message":"no such table: t","statement":1}"#;

        assert_eq!(round_trip::<D1Result>(json), json);
    }

    #[test]
    fn error_without_statement_omits_the_index() {
        let json = r#"{"type":"error","kind":"overloaded","message":"queue full"}"#;

        assert_eq!(round_trip::<D1Result>(json), json);
    }

    #[test]
    fn batch_and_exec_results_round_trip() {
        let batch = r#"{"type":"batch","results":[{"columns":[],"rows":[],"meta":{"duration":0.1,"changes":1,"last_row_id":7,"changed_db":true,"size_after":8192}}]}"#;
        let exec = r#"{"type":"exec","count":2,"duration":1.5}"#;

        assert_eq!(round_trip::<D1Result>(batch), batch);
        assert_eq!(round_trip::<D1Result>(exec), exec);
    }
}
