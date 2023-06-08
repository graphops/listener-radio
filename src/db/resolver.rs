use async_graphql::OutputType;
use serde::de::DeserializeOwned;
use serde::Serialize;
use sqlx::types::Json;
use sqlx::PgPool;

#[allow(dead_code)]
#[derive(Debug)]
pub struct Row<T: Serialize + DeserializeOwned + OutputType> {
    id: i64,
    message: Json<T>,
}

pub async fn add_message<T>(pool: &PgPool, message: T) -> anyhow::Result<i64>
where
    T: Serialize + DeserializeOwned + OutputType,
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
    T: Serialize + DeserializeOwned + OutputType + std::marker::Unpin,
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
    .await?;

    Ok(rows)
}
