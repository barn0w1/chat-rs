use std::time::SystemTime;

use axum::{
    Form, Router,
    extract::{DefaultBodyLimit, Query, State, rejection::FormRejection},
    http::{
        HeaderMap, HeaderValue, StatusCode,
        header::{CACHE_CONTROL, LOCATION, SET_COOKIE},
    },
    response::{IntoResponse, Response},
    routing::get,
};
use serde::Deserialize;

use crate::{
    app::AppState,
    auth::{AdmissionCodeId, AdmissionOutcome, AuthError, SecretToken},
};

use super::{
    authentication::{
        AuthenticatedSessionRequest, CSRF_HEADER, SameOrigin, auth_operation_problem, cookie_value,
        valid_origin,
    },
    problem::Problem,
    representation::{SessionRepresentation, UserRepresentation, json_no_store},
};

const MAX_AUTHORIZATION_CODE_BYTES: usize = 4 * 1024;
const MAX_CALLBACK_STATE_BYTES: usize = 64;
const MAX_PROVIDER_ERROR_BYTES: usize = 1024;

pub(super) fn routes(oidc_enabled: bool) -> Router<AppState> {
    let app = Router::new().route("/api/v1/session", get(get_session).delete(delete_session));
    if oidc_enabled {
        app.route(
            "/auth/oidc/start",
            get(start_oidc)
                .post(start_oidc_with_code)
                .layer(DefaultBodyLimit::max(256)),
        )
        .route("/auth/oidc/callback", get(complete_oidc))
    } else {
        app
    }
}

async fn get_session(authenticated: AuthenticatedSessionRequest) -> Result<Response, Problem> {
    let session = authenticated.into_session();
    Ok(json_no_store(SessionRepresentation {
        user: UserRepresentation::try_from_user(session.user())?,
        csrf_token: session.csrf_token(),
    }))
}

async fn delete_session(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let token = match cookie_value(&headers, &state.cookies, state.cookies.session_name()) {
        Ok(token) => token,
        Err(_) => return Problem::forbidden().into_response(),
    };
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

async fn start_oidc(State(state): State<AppState>, headers: HeaderMap) -> Response {
    begin_oidc(state, &headers, None).await
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AdmissionCodeForm {
    admission_code: String,
}

async fn start_oidc_with_code(
    State(state): State<AppState>,
    _same_origin: SameOrigin,
    headers: HeaderMap,
    form: Result<Form<AdmissionCodeForm>, FormRejection>,
) -> Response {
    let form = match form {
        Ok(Form(form)) => form,
        Err(rejection) => return Problem::from_form_rejection(rejection).into_response(),
    };
    let code_id = match state
        .auth
        .resolve_admission_code(&form.admission_code, SystemTime::now())
        .await
    {
        Ok(Some(code_id)) => code_id,
        Ok(None) => return Problem::forbidden().into_response(),
        Err(error) => return internal_auth_error(error),
    };
    begin_oidc(state, &headers, Some(code_id)).await
}

async fn begin_oidc(
    state: AppState,
    headers: &HeaderMap,
    admission_code_id: Option<AdmissionCodeId>,
) -> Response {
    let Some(provider) = state.oidc.as_ref() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let browser_binding = match cookie_value(headers, &state.cookies, state.cookies.login_name()) {
        Ok(Some(value)) => match SecretToken::parse(&value) {
            Ok(binding) => Some(binding),
            Err(_) => return login_failure(&state),
        },
        Ok(None) => None,
        Err(_) => return login_failure(&state),
    };
    let (location, mut login) = match provider.begin_login(browser_binding).await {
        Ok(values) => values,
        Err(error) => return internal_auth_error(error),
    };
    if let Some(code_id) = admission_code_id {
        login = login.with_admission_code(code_id);
    }
    if let Err(error) = state.auth.store_oidc_login(&login, SystemTime::now()).await {
        return match error {
            AuthError::LoginCapacityReached => Problem::unavailable().into_response(),
            error => internal_auth_error(error),
        };
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
    callback: Result<Query<OidcCallback>, axum::extract::rejection::QueryRejection>,
) -> Response {
    let Ok(Query(callback)) = callback else {
        return login_failure(&state);
    };
    if !callback.has_valid_shape() {
        return login_failure(&state);
    }
    let (Some(callback_state), Some(binding), Some(provider)) = (
        callback.state,
        match cookie_value(&headers, &state.cookies, state.cookies.login_name()) {
            Ok(binding) => binding,
            Err(_) => return login_failure(&state),
        },
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
    let user = match state
        .auth
        .resolve_or_admit(
            &identity,
            login.admission_code_id(),
            state.admission_mode,
            now,
        )
        .await
    {
        Ok(AdmissionOutcome::Admitted(user)) => user,
        Ok(AdmissionOutcome::Denied) => return login_failure(&state),
        Err(error) => return internal_auth_error(error),
    };
    let previous = match cookie_value(&headers, &state.cookies, state.cookies.session_name()) {
        Ok(previous) => previous,
        Err(_) => return login_failure(&state),
    };
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
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
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
    auth_operation_problem(&error).into_response()
}

impl OidcCallback {
    fn has_valid_shape(&self) -> bool {
        self.state
            .as_ref()
            .is_none_or(|value| !value.is_empty() && value.len() <= MAX_CALLBACK_STATE_BYTES)
            && self.code.as_ref().is_none_or(|value| {
                !value.is_empty() && value.len() <= MAX_AUTHORIZATION_CODE_BYTES
            })
            && self
                .error
                .as_ref()
                .is_none_or(|value| !value.is_empty() && value.len() <= MAX_PROVIDER_ERROR_BYTES)
    }
}

#[cfg(test)]
mod tests {
    use axum::{
        body::Body,
        http::{Method, Request, header::CONTENT_TYPE},
    };
    use chat::Chat;
    use tower::ServiceExt;

    use crate::{
        Config,
        auth::{AuthStore, CookiePolicy},
        config::ConfigValues,
        sqlite::SqliteStore,
    };

    use super::*;

    async fn test_app() -> (Router, SqliteStore, tempfile::TempDir) {
        let directory = tempfile::tempdir().expect("temporary directory can be created");
        let store = SqliteStore::open(directory.path().join("chat.sqlite3"))
            .await
            .expect("test database can be opened");
        let config = Config::from_values(ConfigValues::default()).unwrap();
        let state = AppState {
            chat: Chat::new(store.clone()),
            auth: AuthStore::new(store.clone()),
            store: store.clone(),
            cookies: CookiePolicy::new(config.public_url()),
            expected_origin: config.public_url().origin().ascii_serialization(),
            oidc: None,
            admission_mode: config.admission_mode(),
        };
        (routes(true).with_state(state), store, directory)
    }

    fn code_request(origin: Option<&str>, content_type: Option<&str>, body: &str) -> Request<Body> {
        let mut builder = Request::builder()
            .method(Method::POST)
            .uri("/auth/oidc/start");
        if let Some(origin) = origin {
            builder = builder.header("origin", origin);
        }
        if let Some(content_type) = content_type {
            builder = builder.header(CONTENT_TYPE, content_type);
        }
        builder
            .body(Body::from(body.to_owned()))
            .expect("test request is valid")
    }

    #[tokio::test]
    async fn admission_code_start_checks_origin_before_form_content() {
        let (app, store, _directory) = test_app().await;
        let response = app
            .oneshot(code_request(
                Some("https://attacker.example"),
                Some("application/x-www-form-urlencoded"),
                "{",
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        store.close().await;
    }

    #[tokio::test]
    async fn admission_code_start_rejects_media_form_size_and_unknown_code() {
        let (app, store, _directory) = test_app().await;
        let origin = Some("http://127.0.0.1:3000");

        let unsupported = app
            .clone()
            .oneshot(code_request(origin, None, "admission_code=value"))
            .await
            .unwrap();
        assert_eq!(unsupported.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);

        let malformed = app
            .clone()
            .oneshot(code_request(
                origin,
                Some("application/x-www-form-urlencoded"),
                "unknown=value",
            ))
            .await
            .unwrap();
        assert_eq!(malformed.status(), StatusCode::BAD_REQUEST);

        let oversized_body = format!("admission_code={}", "x".repeat(300));
        let oversized = app
            .clone()
            .oneshot(code_request(
                origin,
                Some("application/x-www-form-urlencoded"),
                &oversized_body,
            ))
            .await
            .unwrap();
        assert_eq!(oversized.status(), StatusCode::PAYLOAD_TOO_LARGE);

        let unknown = app
            .oneshot(code_request(
                origin,
                Some("application/x-www-form-urlencoded"),
                "admission_code=not-a-valid-code",
            ))
            .await
            .unwrap();
        assert_eq!(unknown.status(), StatusCode::FORBIDDEN);
        store.close().await;
    }

    #[test]
    fn callback_values_are_explicitly_bounded() {
        assert!(
            OidcCallback {
                code: Some(String::from("code")),
                state: Some(String::from("state")),
                error: None,
            }
            .has_valid_shape()
        );
        assert!(
            !OidcCallback {
                code: Some("x".repeat(MAX_AUTHORIZATION_CODE_BYTES + 1)),
                state: Some(String::from("state")),
                error: None,
            }
            .has_valid_shape()
        );
        assert!(
            !OidcCallback {
                code: None,
                state: Some(String::new()),
                error: Some(String::from("access_denied")),
            }
            .has_valid_shape()
        );
    }

    #[test]
    fn authentication_redirects_are_not_cacheable() {
        let response = redirect_with_cookies("/", [None, None]);

        assert_eq!(response.headers()[CACHE_CONTROL], "no-store");
    }

    #[test]
    fn authentication_error_categories_have_stable_statuses() {
        assert_eq!(
            internal_auth_error(AuthError::LoginCapacityReached).status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
        assert_eq!(
            internal_auth_error(AuthError::InvalidStoredData).status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }
}
