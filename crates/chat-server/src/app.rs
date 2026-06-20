use std::time::Duration;

use axum::{Router, extract::State, http::StatusCode, routing::get};
use tower_http::trace::{DefaultMakeSpan, DefaultOnRequest, DefaultOnResponse, TraceLayer};
use tracing::Level;

use crate::sqlite::SqliteStore;

const READINESS_TIMEOUT: Duration = Duration::from_secs(1);

#[derive(Clone, Debug)]
struct AppState {
    store: SqliteStore,
}

pub(crate) fn router(store: SqliteStore) -> Router {
    let state = AppState { store };
    let trace = TraceLayer::new_for_http()
        .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
        .on_request(DefaultOnRequest::new().level(Level::DEBUG))
        .on_response(DefaultOnResponse::new().level(Level::INFO));

    Router::new()
        .route("/health/live", get(liveness))
        .route("/health/ready", get(readiness))
        .layer(trace)
        .with_state(state)
}

async fn liveness() -> StatusCode {
    StatusCode::NO_CONTENT
}

async fn readiness(State(state): State<AppState>) -> StatusCode {
    match tokio::time::timeout(READINESS_TIMEOUT, state.store.check_readiness()).await {
        Ok(Ok(())) => StatusCode::NO_CONTENT,
        Ok(Err(_)) | Err(_) => StatusCode::SERVICE_UNAVAILABLE,
    }
}

#[cfg(test)]
mod tests {
    use axum::{
        body::Body,
        http::{Method, Request},
    };
    use tempfile::TempDir;
    use tower::ServiceExt;

    use super::*;

    async fn test_app() -> (Router, SqliteStore, TempDir) {
        let directory = tempfile::tempdir().expect("temporary directory can be created");
        let store = SqliteStore::open(directory.path().join("chat.sqlite3"))
            .await
            .expect("test database can be opened");
        (router(store.clone()), store, directory)
    }

    async fn response_status(app: Router, method: Method, uri: &str) -> StatusCode {
        app.oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .body(Body::empty())
                .expect("test request is valid"),
        )
        .await
        .expect("router is infallible")
        .status()
    }

    #[tokio::test]
    async fn health_routes_report_process_and_database_state() {
        let (app, store, _directory) = test_app().await;

        assert_eq!(
            response_status(app.clone(), Method::GET, "/health/live").await,
            StatusCode::NO_CONTENT
        );
        assert_eq!(
            response_status(app.clone(), Method::GET, "/health/ready").await,
            StatusCode::NO_CONTENT
        );

        store.close().await;

        assert_eq!(
            response_status(app.clone(), Method::GET, "/health/live").await,
            StatusCode::NO_CONTENT
        );
        assert_eq!(
            response_status(app, Method::GET, "/health/ready").await,
            StatusCode::SERVICE_UNAVAILABLE
        );
    }

    #[tokio::test]
    async fn router_preserves_standard_not_found_and_method_responses() {
        let (app, store, _directory) = test_app().await;

        assert_eq!(
            response_status(app.clone(), Method::GET, "/missing").await,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            response_status(app, Method::POST, "/health/live").await,
            StatusCode::METHOD_NOT_ALLOWED
        );

        store.close().await;
    }
}
