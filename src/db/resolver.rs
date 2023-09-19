use async_graphql::OutputType;
use serde::{de::DeserializeOwned, Serialize};
use sqlx::{types::Json, PgPool};
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
pub async fn retain_max_storage(
    pool: &PgPool,
    max_storage: usize,
) -> Result<i64, anyhow::Error>
{
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
