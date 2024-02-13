use async_graphql::OutputType;
use chrono::Utc;
use serde::{de::DeserializeOwned, Serialize};
use sqlx::{types::Json, PgPool, Row as SqliteRow};
use std::ops::Deref;
use tracing::trace;

use crate::server::model::GraphQLRow;

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct Row<T: Clone + Serialize + DeserializeOwned + OutputType> {
    id: i64,
    message: Json<T>,
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct MessageID {
    id: i64,
}

// Define graphql type for the Row in Messages
impl<T: Clone + Serialize + DeserializeOwned + OutputType> Row<T> {
    pub fn get_graphql_row(&self) -> GraphQLRow<T> {
        GraphQLRow::new(self.get_id(), self.get_message())
    }

    pub fn get_id(&self) -> i64 {
        self.id
    }

    pub fn get_message(&self) -> T {
        self.message.clone().deref().clone()
    }
}

pub async fn add_message<T>(pool: &PgPool, message: T) -> anyhow::Result<i64>
where
    T: Clone + Serialize + DeserializeOwned + OutputType,
{
    let rec = sqlx::query!(
        r#"
INSERT INTO messages ( message )
VALUES ( $1 )
RETURNING id
        "#,
        Json(message) as _
    )
    .fetch_one(pool)
    .await?;

    Ok(rec.id)
}

pub async fn list_messages<T>(pool: &PgPool) -> Result<Vec<Row<T>>, anyhow::Error>
where
    T: Clone + Serialize + DeserializeOwned + OutputType + std::marker::Unpin,
{
    let rows = sqlx::query_as!(
        Row,
        r#"
SELECT id, message as "message: Json<T>"
FROM messages
ORDER BY id
        "#
    )
    .fetch_all(pool)
    .await
    .map_err(|e| {
        trace!("Database resolver connection error: {:#?}", e);
        e
    })?;

    Ok(rows)
}

pub async fn list_rows<T>(pool: &PgPool) -> Result<Vec<GraphQLRow<T>>, anyhow::Error>
where
    T: Clone + Serialize + DeserializeOwned + OutputType + std::marker::Unpin,
{
    let rows = sqlx::query_as!(
        Row,
        r#"
SELECT id, message as "message: Json<T>"
FROM messages
ORDER BY id
        "#
    )
    .fetch_all(pool)
    .await?
    .iter()
    .map(|r| r.get_graphql_row())
    .collect::<Vec<GraphQLRow<T>>>();

    Ok(rows)
}

pub async fn message_by_id<T>(pool: &PgPool, id: i64) -> Result<Row<T>, anyhow::Error>
where
    T: Clone + Serialize + DeserializeOwned + OutputType + std::marker::Unpin,
{
    let row = sqlx::query_as!(
        Row,
        r#"
SELECT id, message as "message: Json<T>"
FROM messages
WHERE id = $1
        "#,
        id
    )
    .fetch_one(pool)
    .await?;

    Ok(row)
}

pub async fn delete_message_by_id<T>(pool: &PgPool, id: i64) -> Result<Row<T>, anyhow::Error>
where
    T: Clone + Serialize + DeserializeOwned + OutputType + std::marker::Unpin,
{
    let row = sqlx::query_as!(
        Row,
        r#"
DELETE
FROM messages
WHERE id = $1
RETURNING id, message as "message: Json<T>"
        "#,
        id
    )
    .fetch_one(pool)
    .await?;

    Ok(row)
}

pub async fn delete_message_all<T>(pool: &PgPool) -> Result<Vec<Row<T>>, anyhow::Error>
where
    T: Clone + Serialize + DeserializeOwned + OutputType + std::marker::Unpin,
{
    let rows = sqlx::query_as!(
        Row,
        r#"
DELETE
FROM messages
RETURNING id, message as "message: Json<T>"
        "#
    )
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

/// Function to automatically prune older messages and keep the `max_storage` newest messages
/// We prune from the smallest id by the automcatic ascending behavior
/// Return the number of messages deleted
pub async fn retain_max_storage(pool: &PgPool, max_storage: usize) -> Result<i64, anyhow::Error> {
    // find out the IDs of the top `max_storage` newest messages.
    let top_ids: Vec<i64> = sqlx::query_as!(
        MessageID,
        r#"
SELECT id
FROM messages
ORDER BY id DESC
LIMIT $1
        "#,
        max_storage as i64
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|row| row.id)
    .collect::<Vec<i64>>();

    trace!(top_ids = tracing::field::debug(&top_ids), "IDs to keep");

    // Then, delete all messages except those with the above IDs.
    let deleted_ids = sqlx::query!(
        r#"
DELETE
FROM messages
WHERE id NOT IN (SELECT unnest($1::int8[]))
RETURNING id
        "#,
        &top_ids
    )
    .fetch_all(pool)
    .await?
    .len();

    Ok(deleted_ids.try_into().unwrap())
}

/// Function to delete messages older than `retention` minutes
/// Returns the number of messages deleted
pub async fn prune_old_messages(pool: &PgPool, retention: i32) -> Result<i64, anyhow::Error> {
    let cutoff_nonce = Utc::now().timestamp() - (retention as i64 * 60);

    let deleted_count = sqlx::query!(
        r#"
        DELETE FROM messages
        WHERE (message->>'nonce')::bigint < $1
        RETURNING id
        "#,
        cutoff_nonce
    )
    .fetch_all(pool)
    .await?
    .len() as i64;

    Ok(deleted_count)
}

pub async fn list_active_indexers(
    pool: &PgPool,
    indexers: Option<Vec<String>>,
    from_timestamp: i64,
) -> Result<Vec<String>, anyhow::Error> {
    let mut query = String::from("SELECT DISTINCT message->>'graph_account' as graph_account FROM messages WHERE (CAST(message->>'nonce' AS BIGINT)) > $1");

    // Dynamically add placeholders for indexers if provided.
    if let Some(ref idxs) = indexers {
        let placeholders = idxs
            .iter()
            .enumerate()
            .map(|(i, _)| format!("${}", i + 2))
            .collect::<Vec<_>>()
            .join(",");
        query.push_str(&format!(
            " AND (message->>'graph_account') IN ({})",
            placeholders
        ));
    }

    let mut query = sqlx::query(&query).bind(from_timestamp);

    // Bind indexers to the query if provided.
    if let Some(indexers) = indexers {
        for account in indexers {
            query = query.bind(account);
        }
    }

    let rows = query
        .fetch_all(pool)
        .await
        .map_err(anyhow::Error::new)? // Convert sqlx::Error to anyhow::Error for uniform error handling.
        .iter()
        .map(|row| row.get::<String, _>("graph_account"))
        .collect();

    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use sqlx::PgPool;

    async fn insert_test_data(pool: &PgPool, entries: Vec<(i64, &str)>) {
        for (nonce, graph_account) in entries {
            let message = json!({
                "nonce": nonce,
                "graph_account": graph_account
            });

            sqlx::query!("INSERT INTO messages (message) VALUES ($1)", message)
                .execute(pool)
                .await
                .expect("Failed to insert test data");
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn test_list_active_indexers_without_indexers(pool: PgPool) {
        insert_test_data(
            &pool,
            vec![(1707328517, "0xb4b4570df6f7fe320f10fdfb702dba7e35244550")],
        )
        .await;

        let from_timestamp = 1707328516;
        let indexers = None;
        let result = list_active_indexers(&pool, indexers, from_timestamp)
            .await
            .expect("Function should complete successfully");

        assert!(
            result.contains(&"0xb4b4570df6f7fe320f10fdfb702dba7e35244550".to_string()),
            "Result should contain the expected graph_account"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn test_list_active_indexers_with_specific_indexers(pool: PgPool) {
        insert_test_data(
            &pool,
            vec![(1707328517, "0xb4b4570df6f7fe320f10fdfb702dba7e35244550")],
        )
        .await;

        let from_timestamp = 1707328516;
        let indexers = Some(vec![
            "0xb4b4570df6f7fe320f10fdfb702dba7e35244550".to_string(),
            "nonexistent_indexer".to_string(),
        ]);
        let result = list_active_indexers(&pool, indexers, from_timestamp)
            .await
            .expect("Function should complete successfully");

        assert_eq!(
            result.len(),
            1,
            "Should only match records for existing indexers"
        );
        assert!(
            result.contains(&"0xb4b4570df6f7fe320f10fdfb702dba7e35244550".to_string()),
            "Result should contain the expected graph_account"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn test_list_active_indexers_no_matching_records(pool: PgPool) {
        let from_timestamp = 9999999999;
        let indexers = None;
        let result = list_active_indexers(&pool, indexers, from_timestamp)
            .await
            .expect("Function should complete successfully");

        assert!(
            result.is_empty(),
            "Result should be empty when no records match criteria"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn test_list_active_indexers_edge_cases(pool: PgPool) {
        let specific_nonce = Utc::now().timestamp();
        insert_test_data(
            &pool,
            vec![(specific_nonce, "0xb4b4570df6f7fe320f10fdfb702dba7e35244550")],
        )
        .await;

        let from_timestamp = specific_nonce;
        let indexers = None;
        let result = list_active_indexers(&pool, indexers, from_timestamp)
            .await
            .expect("Function should complete successfully");

        assert!(
            result.is_empty(),
            "Result should be empty when from_timestamp exactly matches nonce"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn test_list_active_indexers_with_partial_matching_indexers(pool: PgPool) {
        insert_test_data(
            &pool,
            vec![
                (1707328517, "0xb4b4570df6f7fe320f10fdfb702dba7e35244550"),
                (1707328518, "some_other_account"),
            ],
        )
        .await;

        let from_timestamp = 1707328516;
        let indexers = Some(vec![
            "0xb4b4570df6f7fe320f10fdfb702dba7e35244550".to_string(),
            "partial_match_indexer".to_string(),
        ]);
        let result = list_active_indexers(&pool, indexers, from_timestamp)
            .await
            .expect("Function should complete successfully");

        assert_eq!(
            result.len(),
            1,
            "Should only match records for existing indexers"
        );
        assert!(
            result.contains(&"0xb4b4570df6f7fe320f10fdfb702dba7e35244550".to_string()),
            "Result should contain the expected graph_account"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn test_list_active_indexers_with_nonexistent_indexers(pool: PgPool) {
        let from_timestamp = 1707328516;
        let indexers = Some(vec![
            "nonexistent_indexer_1".to_string(),
            "nonexistent_indexer_2".to_string(),
        ]);
        let result = list_active_indexers(&pool, indexers, from_timestamp)
            .await
            .expect("Function should complete successfully");

        assert!(
            result.is_empty(),
            "Result should be empty for non-existent indexers"
        );
    }
}
