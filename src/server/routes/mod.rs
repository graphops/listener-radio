use async_graphql::http::{playground_source, GraphQLPlaygroundConfig};
use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use axum::{
    extract::Extension,
    http::StatusCode,
    response::{Html, IntoResponse},
    Json,
};

use serde::Serialize;
use std::sync::Arc;
use tracing::trace;

use super::model::RadioContext;
use crate::server::model::RadioSchema;

#[derive(Serialize)]
struct Health {
    healthy: bool,
}

pub(crate) async fn health() -> impl IntoResponse {
    let health = Health { healthy: true };

    (StatusCode::OK, Json(health))
}

pub(crate) async fn graphql_playground() -> impl IntoResponse {
    Html(playground_source(
        GraphQLPlaygroundConfig::new("/").subscription_endpoint("/ws"),
    ))
}

pub(crate) async fn graphql_handler(
    req: GraphQLRequest,
    Extension(schema): Extension<RadioSchema>,
    Extension(context): Extension<Arc<RadioContext>>,
) -> GraphQLResponse {
    trace!("Processing GraphQL request");
    let response = async move { schema.execute(req.into_inner().data(context)).await }.await;

    trace!("Processing GraphQL request finished");

    response.into()
}
