use async_graphql::{Context, EmptySubscription, Object, OutputType, Schema, SimpleObject};

use chrono::Utc;
use serde::{de::DeserializeOwned, Serialize};
use sqlx::{Pool, Postgres};
use std::{collections::HashMap, sync::Arc, time::Duration};
use thiserror::Error;
use tracing::error;

use crate::{
    config::Config,
    db::resolver::{
        count_distinct_subgraphs, delete_message_all, delete_message_by_id, fetch_aggregates,
        get_indexer_stats, list_active_indexers, list_messages, list_rows, message_by_id,
        IndexerStats,
    },
    operator::radio_types::RadioPayloadMessage,
};
use graphcast_sdk::{graphcast_agent::message_typing::GraphcastMessage, graphql::QueryError};

pub type RadioSchema = Schema<QueryRoot, MutationRoot, EmptySubscription>;

pub async fn build_schema(ctx: Arc<RadioContext>) -> RadioSchema {
    Schema::build(QueryRoot, MutationRoot, EmptySubscription)
        .data(ctx.db.clone())
        .finish()
}

pub struct RadioContext {
    pub radio_config: Config,
    pub db: Pool<Postgres>,
}

impl RadioContext {
    pub fn init(radio_config: Config, db: Pool<Postgres>) -> Self {
        Self { radio_config, db }
    }
}

#[derive(Serialize, SimpleObject)]
pub struct Summary {
    total_message_count: HashMap<String, i64>,
    average_subgraphs_count: HashMap<String, i64>,
    total_subgraphs_covered: i64,
}

// Unified query object for resolvers
#[derive(Default)]
pub struct QueryRoot;

#[Object]
impl QueryRoot {
    async fn health_check(&self) -> &str {
        "Healthy"
    }

    // List rows but without filter options since msg fields are saved in jsonb
    // Later flatten the messages to have columns from graphcast message.
    async fn rows(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Vec<GraphQLRow<GraphcastMessage<RadioPayloadMessage>>>, HttpServiceError> {
        let pool = ctx.data_unchecked::<Pool<Postgres>>();

        let rows: Vec<GraphQLRow<GraphcastMessage<RadioPayloadMessage>>> = list_rows(pool).await?;
        Ok(rows)
    }

    async fn query_active_indexers(
        &self,
        ctx: &Context<'_>,
        indexers: Option<Vec<String>>,
        minutes_ago: Option<u64>,
    ) -> Result<Vec<String>, HttpServiceError> {
        let pool = ctx.data_unchecked::<Pool<Postgres>>();
        // Use a default time window if not specified
        // Default to 1440 minutes (24 hours) if not provided
        let minutes_ago = minutes_ago.unwrap_or(1440);
        let from_timestamp = (Utc::now() - Duration::from_secs(minutes_ago * 60)).timestamp();

        let active_indexers = list_active_indexers(pool, indexers, from_timestamp).await?;
        Ok(active_indexers)
    }

    async fn query_indexer_stats(
        &self,
        ctx: &Context<'_>,
        indexers: Option<Vec<String>>,
        minutes_ago: Option<u64>,
    ) -> Result<Vec<IndexerStats>, HttpServiceError> {
        let pool = ctx.data_unchecked::<Pool<Postgres>>();
        let minutes_ago = minutes_ago.unwrap_or(1440);
        let from_timestamp = (Utc::now() - Duration::from_secs(minutes_ago * 60)).timestamp();

        let stats = get_indexer_stats(pool, indexers, from_timestamp).await?;
        Ok(stats)
    }

    /// Grab a row from db by db entry id
    async fn row(
        &self,
        ctx: &Context<'_>,
        id: i64,
    ) -> Result<GraphQLRow<GraphcastMessage<RadioPayloadMessage>>, HttpServiceError> {
        let pool = ctx.data_unchecked::<Pool<Postgres>>();

        let row: GraphQLRow<GraphcastMessage<RadioPayloadMessage>> =
            message_by_id(pool, id).await?.get_graphql_row();
        Ok(row)
    }

    // List messages but without filter options since msg fields are saved in jsonb
    // Later flatten the messages to have columns from graphcast message.
    async fn messages(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Vec<GraphcastMessage<RadioPayloadMessage>>, HttpServiceError> {
        let pool = ctx.data_unchecked::<Pool<Postgres>>();

        let msgs: Vec<GraphcastMessage<RadioPayloadMessage>> = list_messages(pool)
            .await?
            .iter()
            .map(|r| r.get_message())
            .collect::<Vec<GraphcastMessage<RadioPayloadMessage>>>();
        Ok(msgs)
    }

    async fn message(
        &self,
        ctx: &Context<'_>,
        id: i64,
    ) -> Result<GraphcastMessage<RadioPayloadMessage>, HttpServiceError> {
        let pool = ctx.data_unchecked::<Pool<Postgres>>();

        let msg: GraphcastMessage<RadioPayloadMessage> =
            message_by_id(pool, id).await?.get_message();
        Ok(msg)
    }

    async fn query_aggregate_stats(
        &self,
        ctx: &Context<'_>,
        days: i32,
    ) -> Result<Summary, HttpServiceError> {
        let pool = ctx.data_unchecked::<Pool<Postgres>>();

        let since_timestamp =
            (Utc::now() - chrono::Duration::try_days(days.into()).unwrap()).timestamp();
        let aggregates = fetch_aggregates(pool, since_timestamp)
            .await
            .map_err(HttpServiceError::Others)?;

        let mut total_message_count: HashMap<String, i64> = HashMap::new();
        let mut total_subgraphs_count: HashMap<String, i64> = HashMap::new();

        let mut subgraphs_counts = HashMap::new();

        for stat in aggregates {
            *total_message_count
                .entry(stat.graph_account.clone())
                .or_default() += stat.message_count;
            *total_subgraphs_count
                .entry(stat.graph_account.clone())
                .or_default() += stat.subgraphs_count;
            subgraphs_counts
                .entry(stat.graph_account.clone())
                .or_insert_with(Vec::new)
                .push(stat.subgraphs_count);
        }

        let average_subgraphs_count: HashMap<String, i64> = total_subgraphs_count
            .iter()
            .map(|(key, &total_count)| {
                let count = subgraphs_counts.get(key).map_or(1, |counts| counts.len());
                (
                    key.clone(),
                    if count > 0 {
                        (total_count as f64 / count as f64).ceil() as i64
                    } else {
                        0
                    },
                )
            })
            .collect();

        let total_subgraphs_covered = count_distinct_subgraphs(pool, since_timestamp)
            .await
            .map_err(HttpServiceError::Others)?;
        Ok(Summary {
            total_message_count,
            average_subgraphs_count,
            total_subgraphs_covered,
        })
    }
}

// Unified query object for resolvers
#[derive(Default)]
pub struct MutationRoot;

#[Object]
impl MutationRoot {
    async fn delete_message(
        &self,
        ctx: &Context<'_>,
        id: i64,
    ) -> Result<GraphcastMessage<RadioPayloadMessage>, HttpServiceError> {
        let pool = ctx.data_unchecked::<Pool<Postgres>>();

        let msg: GraphcastMessage<RadioPayloadMessage> =
            delete_message_by_id(pool, id).await?.get_message();
        Ok(msg)
    }

    async fn delete_messages(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Vec<GraphcastMessage<RadioPayloadMessage>>, HttpServiceError> {
        let pool = ctx.data_unchecked::<Pool<Postgres>>();

        let msgs: Vec<GraphcastMessage<RadioPayloadMessage>> = delete_message_all(pool)
            .await?
            .iter()
            .map(|r| r.get_message())
            .collect::<Vec<GraphcastMessage<RadioPayloadMessage>>>();
        Ok(msgs)
    }
}

#[derive(Clone, Debug, SimpleObject)]
pub struct GraphQLRow<T: Clone + Serialize + DeserializeOwned + OutputType> {
    id: i64,
    message: T,
}

impl<T: Clone + Serialize + DeserializeOwned + OutputType> GraphQLRow<T> {
    pub fn new(id: i64, message: T) -> Self {
        GraphQLRow { id, message }
    }
}

#[derive(Error, Debug)]
pub enum HttpServiceError {
    #[error("Missing requested data: {0}")]
    MissingData(String),
    #[error("Reqwest Error: {0}")]
    Reqwest(reqwest::Error),
    #[error("Query failed: {0}")]
    QueryError(QueryError),
    // Below ones are not used yet
    #[error("HTTP request failed: {0}")]
    RequestFailed(String),
    #[error("HTTP response error: {0}")]
    ResponseError(String),
    #[error("Timeout error")]
    TimeoutError,
    #[error("Invalid URL: {0}")]
    InvalidUrl(String),
    #[error("HTTP client error: {0}")]
    HttpClientError(#[from] reqwest::Error),
    #[error("{0}")]
    Others(#[from] anyhow::Error),
}
