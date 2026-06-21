use std::time::{Duration, SystemTime};

use axum::{
    Json, Router,
    extract::{FromRequestParts, Query, State},
    http::{
        HeaderMap, HeaderValue, StatusCode,
        header::{CACHE_CONTROL, COOKIE, LOCATION, ORIGIN, SET_COOKIE},
        request::Parts,
    },
    response::{IntoResponse, Response},
    routing::get,
};
use serde::{Deserialize, Serialize};
use tower_http::trace::{DefaultMakeSpan, DefaultOnRequest, DefaultOnResponse, TraceLayer};
use tracing::Level;

use crate::{
    Config,
    auth::{AuthError, AuthStore, CookiePolicy, OidcProvider, unix_time_millis},
    sqlite::SqliteStore,
};

const READINESS_TIMEOUT: Duration = Duration::from_secs(1);
const CSRF_HEADER: &str = "x-csrf-token";

#[derive(Clone, Debug)]
struct AppState {
    store: SqliteStore,
    auth: AuthStore,
    cookies: CookiePolicy,
    expected_origin: String,
    oidc: Option<OidcProvider>,
}

pub(crate) fn router(store: SqliteStore, config: &Config, oidc: Option<OidcProvider>) -> Router {
    let state = AppState {
        auth: AuthStore::new(store.clone()),
        store,
        cookies: CookiePolicy::new(config.public_url()),
        expected_origin: config.public_url().origin().ascii_serialization(),
        oidc,
    };
    let oidc_enabled = state.oidc.is_some();
    let trace = TraceLayer::new_for_http()
        .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
        .on_request(DefaultOnRequest::new().level(Level::DEBUG))
        .on_response(DefaultOnResponse::new().level(Level::INFO));

    let mut app = Router::new()
        .route("/health/live", get(liveness))
        .route("/health/ready", get(readiness))
        .route("/api/v1/session", get(get_session).delete(delete_session));
    if oidc_enabled {
        app = app
            .route("/auth/oidc/start", get(start_oidc))
            .route("/auth/oidc/callback", get(complete_oidc));
    }

    app.layer(trace).with_state(state)
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

async fn get_session(authenticated: AuthenticatedRequest) -> Response {
    let session = authenticated.session;
    let Ok(created_at_ms) = unix_time_millis(*session.user().created_at()) else {
        return Problem::unavailable().into_response();
    };
    let payload = SessionRepresentation {
        user: UserRepresentation {
            id: session.user_id().get().to_string(),
            display_name: session.user().display_name().as_str(),
            created_at_ms,
        },
        csrf_token: session.csrf_token(),
    };
    let mut response = Json(payload).into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

struct AuthenticatedRequest {
    session: crate::auth::AuthenticatedSession,
}

impl FromRequestParts<AppState> for AuthenticatedRequest {
    type Rejection = Problem;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let token = cookie_value(&parts.headers, &state.cookies, state.cookies.session_name())
            .ok_or(Problem::unauthenticated())?;
        match state.auth.resolve_session(&token, SystemTime::now()).await {
            Ok(Some(session)) => Ok(Self { session }),
            Ok(None) => Err(Problem::unauthenticated()),
            Err(error) => {
                tracing::error!(error = %error, "session authentication failed");
                Err(Problem::unavailable())
            }
        }
    }
}

async fn delete_session(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let token = cookie_value(&headers, &state.cookies, state.cookies.session_name());
    if let Some(token) = token.as_deref() {
        match state.auth.resolve_session(token, SystemTime::now()).await {
            Ok(Some(session)) => {
                if !valid_origin(&headers, &state.expected_origin)
                    || headers
                        .get(CSRF_HEADER)
                        .and_then(|value| value.to_str().ok())
                        .is_none_or(|value| !session.verifies_csrf(value))
                {
                    return Problem::forbidden().into_response();
                }
                if let Err(error) = state.auth.revoke_session(token).await {
                    return internal_auth_error(error);
                }
            }
            Ok(None) => {}
            Err(error) => return internal_auth_error(error),
        }
    }

    empty_with_cookie(
        StatusCode::NO_CONTENT,
        state.cookies.remove_session_cookie(),
    )
}

async fn start_oidc(State(state): State<AppState>) -> Response {
    let Some(provider) = state.oidc.as_ref() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let (location, login) = match provider.begin_login().await {
        Ok(values) => values,
        Err(error) => return internal_auth_error(error),
    };
    if let Err(error) = state.auth.store_oidc_login(&login, SystemTime::now()).await {
        return internal_auth_error(error);
    }

    redirect_with_cookies(
        location.as_str(),
        [
            Some(state.cookies.login_cookie(login.browser_binding().encode())),
            None,
        ],
    )
}

#[derive(Debug, Deserialize)]
struct OidcCallback {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

async fn complete_oidc(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(callback): Query<OidcCallback>,
) -> Response {
    let (Some(callback_state), Some(binding), Some(provider)) = (
        callback.state,
        cookie_value(&headers, &state.cookies, state.cookies.login_name()),
        state.oidc.as_ref(),
    ) else {
        return login_failure(&state);
    };
    let login = match state
        .auth
        .consume_oidc_login(&callback_state, &binding, SystemTime::now())
        .await
    {
        Ok(login) => login,
        Err(AuthError::LoginTransactionRejected | AuthError::InvalidToken(_)) => {
            return login_failure(&state);
        }
        Err(error) => return internal_auth_error(error),
    };
    if callback.error.is_some() {
        return login_failure(&state);
    }
    let Some(code) = callback.code else {
        return login_failure(&state);
    };
    let identity = match provider.exchange(code, &login).await {
        Ok(identity) => identity,
        Err(error) => {
            tracing::warn!(error = %error, "OIDC login verification failed");
            return login_failure(&state);
        }
    };
    let now = SystemTime::now();
    let user = match state.auth.resolve_or_provision(&identity, now).await {
        Ok(user) => user,
        Err(error) => return internal_auth_error(error),
    };
    let previous = cookie_value(&headers, &state.cookies, state.cookies.session_name());
    let issued = match state
        .auth
        .issue_session(user.id(), previous.as_deref(), now)
        .await
    {
        Ok(session) => session,
        Err(error) => return internal_auth_error(error),
    };

    redirect_with_cookies(
        "/",
        [
            Some(state.cookies.session_cookie(issued.session_token())),
            Some(state.cookies.remove_login_cookie()),
        ],
    )
}

fn valid_origin(headers: &HeaderMap, expected: &str) -> bool {
    headers
        .get(ORIGIN)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value == expected)
}

fn cookie_value(headers: &HeaderMap, policy: &CookiePolicy, name: &str) -> Option<String> {
    headers
        .get_all(COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find_map(|value| policy.find(value, name))
}

fn empty_with_cookie(status: StatusCode, cookie: cookie::Cookie<'static>) -> Response {
    let mut response = status.into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    if let Ok(value) = HeaderValue::from_str(&cookie.to_string()) {
        response.headers_mut().append(SET_COOKIE, value);
    }
    response
}

fn redirect_with_cookies(
    location: &str,
    cookies: [Option<cookie::Cookie<'static>>; 2],
) -> Response {
    let mut response = StatusCode::SEE_OTHER.into_response();
    let Ok(location) = HeaderValue::from_str(location) else {
        return Problem::internal().into_response();
    };
    response.headers_mut().insert(LOCATION, location);
    for cookie in cookies.into_iter().flatten() {
        let Ok(value) = HeaderValue::from_str(&cookie.to_string()) else {
            return Problem::internal().into_response();
        };
        response.headers_mut().append(SET_COOKIE, value);
    }
    response
}

fn login_failure(state: &AppState) -> Response {
    let mut response = Problem::login_failed().into_response();
    if let Ok(value) = HeaderValue::from_str(&state.cookies.remove_login_cookie().to_string()) {
        response.headers_mut().append(SET_COOKIE, value);
    }
    response
}

fn internal_auth_error(error: AuthError) -> Response {
    tracing::error!(error = %error, "authentication operation failed");
    Problem::unavailable().into_response()
}

#[derive(Debug, Serialize)]
struct SessionRepresentation<'a> {
    user: UserRepresentation<'a>,
    csrf_token: String,
}

#[derive(Debug, Serialize)]
struct UserRepresentation<'a> {
    id: String,
    display_name: &'a str,
    created_at_ms: i64,
}

#[derive(Debug, Serialize)]
struct Problem {
    #[serde(rename = "type")]
    type_uri: &'static str,
    title: &'static str,
    status: u16,
}

impl Problem {
    const fn unauthenticated() -> Self {
        Self::new(
            "urn:chat-rs:problem:authentication-required",
            "Authentication required",
            401,
        )
    }

    const fn forbidden() -> Self {
        Self::new(
            "urn:chat-rs:problem:forbidden",
            "Request is not permitted",
            403,
        )
    }

    const fn login_failed() -> Self {
        Self::new(
            "urn:chat-rs:problem:login-failed",
            "Login could not be completed",
            401,
        )
    }

    const fn unavailable() -> Self {
        Self::new(
            "urn:chat-rs:problem:service-unavailable",
            "Service temporarily unavailable",
            503,
        )
    }

    const fn internal() -> Self {
        Self::new("urn:chat-rs:problem:internal", "Internal server error", 500)
    }

    const fn new(type_uri: &'static str, title: &'static str, status: u16) -> Self {
        Self {
            type_uri,
            title,
            status,
        }
    }
}

impl IntoResponse for Problem {
    fn into_response(self) -> Response {
        let status = StatusCode::from_u16(self.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        let mut response = (status, Json(self)).into_response();
        response.headers_mut().insert(
            axum::http::header::CONTENT_TYPE,
            HeaderValue::from_static("application/problem+json"),
        );
        response
            .headers_mut()
            .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
        response
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

    use crate::config::ConfigValues;
    use chat::{Chat, CreateUser};

    use super::*;

    async fn test_app() -> (Router, SqliteStore, TempDir) {
        let directory = tempfile::tempdir().expect("temporary directory can be created");
        let store = SqliteStore::open(directory.path().join("chat.sqlite3"))
            .await
            .expect("test database can be opened");
        let config = Config::from_values(ConfigValues {
            database_path: Some(directory.path().join("chat.sqlite3").into_os_string()),
            ..ConfigValues::default()
        })
        .expect("test configuration is valid");
        (router(store.clone(), &config, None), store, directory)
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
    async fn router_preserves_standard_and_authentication_responses() {
        let (app, store, _directory) = test_app().await;

        assert_eq!(
            response_status(app.clone(), Method::GET, "/missing").await,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            response_status(app.clone(), Method::POST, "/health/live").await,
            StatusCode::METHOD_NOT_ALLOWED
        );
        assert_eq!(
            response_status(app.clone(), Method::GET, "/api/v1/session").await,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            response_status(app, Method::GET, "/auth/oidc/start").await,
            StatusCode::NOT_FOUND
        );
        store.close().await;
    }

    #[tokio::test]
    async fn session_resource_returns_csrf_and_logout_requires_it() {
        let (app, store, _directory) = test_app().await;
        let user_id = Chat::new(store.clone())
            .create_user(CreateUser::new("Alice"))
            .await
            .unwrap()
            .user()
            .id();
        let auth = AuthStore::new(store.clone());
        let now = SystemTime::now();
        let issued = auth.issue_session(user_id, None, now).await.unwrap();
        let cookie = format!("chat_session={}", issued.session_token());
        let csrf = auth
            .resolve_session(&issued.session_token(), now)
            .await
            .unwrap()
            .unwrap()
            .csrf_token();

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/session")
                    .header(COOKIE, &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[CACHE_CONTROL], "no-store");

        let rejected = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::DELETE)
                    .uri("/api/v1/session")
                    .header(COOKIE, &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(rejected.status(), StatusCode::FORBIDDEN);

        let logged_out = app
            .oneshot(
                Request::builder()
                    .method(Method::DELETE)
                    .uri("/api/v1/session")
                    .header(COOKIE, cookie)
                    .header(ORIGIN, "http://127.0.0.1:3000")
                    .header(CSRF_HEADER, csrf)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(logged_out.status(), StatusCode::NO_CONTENT);
        assert!(logged_out.headers().contains_key(SET_COOKIE));
        store.close().await;
    }
}
