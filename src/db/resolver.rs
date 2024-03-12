use async_graphql::{OutputType, SimpleObject};
use chrono::Utc;
use serde::{de::DeserializeOwned, Serialize};
use sqlx::{postgres::PgQueryResult, types::Json, FromRow, PgPool, Row as SqliteRow};
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

#[allow(dead_code)]
#[derive(FromRow, SimpleObject, Serialize, Debug, Clone)]
pub struct IndexerStats {
    graph_account: String,
    message_count: i64,
    subgraphs_count: i64,
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

pub async fn count_messages(pool: &PgPool) -> anyhow::Result<i64> {
    let result = sqlx::query!(
        r#"
        SELECT COUNT(*) as "count!: i64"
        FROM messages
        "#
    )
    .fetch_one(pool)
    .await
    .map_err(|e| {
        trace!("Database query error: {:#?}", e);
        anyhow::Error::new(e)
    })?;

    Ok(result.count)
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

/// Function to delete messages older than `retention` minutes in batches
/// Returns the total number of messages deleted
/// Arguments:
/// - `pool`: &PgPool - A reference to the PostgreSQL connection pool
/// - `retention`: i32 - The retention time in minutes
/// - `batch_size`: i64 - The number of messages to delete in each batch
pub async fn prune_old_messages(
    pool: &PgPool,
    retention: i32,
    batch_size: i64,
) -> Result<i64, anyhow::Error> {
    let cutoff_nonce = Utc::now().timestamp() - (retention as i64 * 60);
    let mut total_deleted = 0i64;

    loop {
        let delete_query = sqlx::query(
            r#"
            WITH deleted AS (
                SELECT id
                FROM messages
                WHERE (message->>'nonce')::bigint < $1
                ORDER BY id ASC
                LIMIT $2
                FOR UPDATE SKIP LOCKED
            )
            DELETE FROM messages
            WHERE id IN (SELECT id FROM deleted)
            RETURNING id
            "#
        )
        .bind(cutoff_nonce)
        .bind(batch_size);

        let result: PgQueryResult = delete_query.execute(pool).await?;
        let deleted_count = result.rows_affected() as i64;

        total_deleted += deleted_count;

        // Break the loop if we deleted fewer rows than the batch size, indicating we've processed all eligible messages.
        if deleted_count < batch_size {
            break;
        }
    }

    Ok(total_deleted)
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
        .map_err(anyhow::Error::new)?
        .iter()
        .map(|row| row.get::<String, _>("graph_account"))
        .collect();

    Ok(rows)
}

pub async fn get_indexer_stats(
    pool: &PgPool,
    indexers: Option<Vec<String>>,
    from_timestamp: i64,
) -> Result<Vec<IndexerStats>, anyhow::Error> {
    let base_query = "
        SELECT 
            message->>'graph_account' as graph_account, 
            COUNT(*) as message_count, 
            COUNT(DISTINCT message->>'identifier') as subgraphs_count -- Updated field name
        FROM messages 
        WHERE (CAST(message->>'nonce' AS BIGINT)) > $1";

    let mut query = String::from(base_query);

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

    query.push_str(" GROUP BY graph_account");

    let mut dynamic_query = sqlx::query_as::<_, IndexerStats>(&query).bind(from_timestamp);

    if let Some(indexers) = indexers {
        for account in indexers {
            dynamic_query = dynamic_query.bind(account);
        }
    }

    let stats = dynamic_query
        .fetch_all(pool)
        .await
        .map_err(anyhow::Error::new)?;

    Ok(stats)
}

#[cfg(test)]
mod tests {
    use crate::message_types::PublicPoiMessage;

    use super::*;
    use sqlx::PgPool;

    async fn insert_test_data(pool: &PgPool, entries: Vec<(i64, &str, &str)>) {
        for (nonce, graph_account, identifier) in entries {
            let message = PublicPoiMessage {
                identifier: identifier.to_string(),
                content: "0xText".to_string(),
                nonce: nonce.try_into().unwrap(),
                network: "testnet".to_string(),
                block_number: 1,
                block_hash: "hash".to_string(),
                graph_account: graph_account.to_string(),
            };

            add_message(pool, message)
                .await
                .expect("Failed to insert test data");
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn test_list_active_indexers_without_indexers(pool: PgPool) {
        insert_test_data(
            &pool,
            vec![(
                1707328517,
                "0xb4b4570df6f7fe320f10fdfb702dba7e35244550",
                "QmTamam",
            )],
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
            vec![(
                1707328517,
                "0xb4b4570df6f7fe320f10fdfb702dba7e35244550",
                "QmTamam",
            )],
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
            vec![(
                specific_nonce,
                "0xb4b4570df6f7fe320f10fdfb702dba7e35244550",
                "QmTamam",
            )],
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
                (
                    1707328517,
                    "0xb4b4570df6f7fe320f10fdfb702dba7e35244550",
                    "QmTamam",
                ),
                (1707328518, "some_other_account", "QmTamam"),
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

    #[sqlx::test(migrations = "./migrations")]
    async fn test_get_indexer_stats_without_parameters(pool: PgPool) {
        insert_test_data(
            &pool,
            vec![
                (
                    1707328517,
                    "0xb4b4570df6f7fe320f10fdfb702dba7e35244550",
                    "QmTamam",
                ),
                (
                    1707328518,
                    "0xb4b4570df6f7fe320f10fdfb702dba7e35244551",
                    "QmTamam",
                ),
            ],
        )
        .await;

        let from_timestamp = 1707328516;
        let indexers = None;
        let result = get_indexer_stats(&pool, indexers, from_timestamp)
            .await
            .expect("Function should complete successfully");

        // Expected: At least the inserted indexers are returned with their message counts
        assert_eq!(result.len(), 2, "Should return stats for all indexers");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn test_get_indexer_stats_with_specific_indexer(pool: PgPool) {
        insert_test_data(
            &pool,
            vec![(
                1707328517,
                "0xb4b4570df6f7fe320f10fdfb702dba7e35244550",
                "QmTamam",
            )],
        )
        .await;

        let from_timestamp = 1707328516;
        let indexers = Some(vec![
            "0xb4b4570df6f7fe320f10fdfb702dba7e35244550".to_string()
        ]);
        let result = get_indexer_stats(&pool, indexers, from_timestamp)
            .await
            .expect("Function should complete successfully");

        // Expected: Only the specified indexer is returned with its message count
        assert_eq!(
            result.len(),
            1,
            "Should return stats for the specified indexer"
        );
        assert!(
            result.iter().any(|stat| stat.graph_account
                == "0xb4b4570df6f7fe320f10fdfb702dba7e35244550"
                && stat.message_count > 0),
            "Result should contain the expected graph_account with correct message count"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn test_get_indexer_stats_with_multiple_indexers(pool: PgPool) {
        insert_test_data(
            &pool,
            vec![
                (
                    1707328517,
                    "0xb4b4570df6f7fe320f10fdfb702dba7e35244550",
                    "QmTamam",
                ),
                (
                    1707328518,
                    "0xb4b4570df6f7fe320f10fdfb702dba7e35244551",
                    "QmTamam",
                ),
            ],
        )
        .await;

        let from_timestamp = 1707328516;
        let indexers = Some(vec![
            "0xb4b4570df6f7fe320f10fdfb702dba7e35244550".to_string(),
            "0xb4b4570df6f7fe320f10fdfb702dba7e35244551".to_string(),
        ]);
        let result = get_indexer_stats(&pool, indexers, from_timestamp)
            .await
            .expect("Function should complete successfully");

        // Expected: Stats for both specified indexers are returned
        assert_eq!(
            result.len(),
            2,
            "Should return stats for the specified indexers"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn test_get_indexer_stats_no_matching_records(pool: PgPool) {
        // Assuming a very high timestamp to ensure no records match
        let from_timestamp = Utc::now().timestamp() + 10000;
        let indexers = None;
        let result = get_indexer_stats(&pool, indexers, from_timestamp)
            .await
            .expect("Function should complete successfully");

        // Expected: No stats are returned since no records match the given timestamp
        assert!(
            result.is_empty(),
            "Result should be empty when no records match criteria"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn test_get_indexer_stats_with_specific_counts(pool: PgPool) {
        // Insert test data with known outcomes
        insert_test_data(
            &pool,
            vec![
                // Inserting 2 messages for the same graph_account with the same identifier (counts as 1 unique subgraph)
                (
                    1707328517,
                    "0xb4b4570df6f7fe320f10fdfb702dba7e35244550",
                    "QmUnique1",
                ),
                (
                    1707328518,
                    "0xb4b4570df6f7fe320f10fdfb702dba7e35244550",
                    "QmUnique1",
                ),
                // Inserting 1 message for another graph_account with a different identifier
                (
                    1707328519,
                    "0xb4b4570df6f7fe320f10fdfb702dba7e35244551",
                    "QmUnique2",
                ),
            ],
        )
        .await;

        let from_timestamp = 1707328516; // Ensure all inserted records are considered
        let indexers = Some(vec![
            "0xb4b4570df6f7fe320f10fdfb702dba7e35244550".to_string(),
            "0xb4b4570df6f7fe320f10fdfb702dba7e35244551".to_string(),
        ]);
        let result = get_indexer_stats(&pool, indexers, from_timestamp)
            .await
            .expect("Function should complete successfully");

        // Asserting on the expected message_count and subgraphs_count
        for stat in result {
            match stat.graph_account.as_str() {
                "0xb4b4570df6f7fe320f10fdfb702dba7e35244550" => {
                    assert_eq!(stat.message_count, 2, "The message count should be 2 for graph_account 0xb4b4570df6f7fe320f10fdfb702dba7e35244550");
                    assert_eq!(stat.subgraphs_count, 1, "The subgraphs count should be 1 for graph_account 0xb4b4570df6f7fe320f10fdfb702dba7e35244550 because both messages share the same identifier");
                }
                "0xb4b4570df6f7fe320f10fdfb702dba7e35244551" => {
                    assert_eq!(stat.message_count, 1, "The message count should be 1 for graph_account 0xb4b4570df6f7fe320f10fdfb702dba7e35244551");
                    assert_eq!(stat.subgraphs_count, 1, "The subgraphs count should also be 1 for graph_account 0xb4b4570df6f7fe320f10fdfb702dba7e35244551 as there is only one message with a unique identifier");
                }
                _ => panic!("Unexpected graph_account found in the result"),
            }
        }
    }
}
