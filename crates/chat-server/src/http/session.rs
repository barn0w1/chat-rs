use std::time::SystemTime;

use axum::{
    Router,
    extract::{Query, State},
    http::{
        HeaderMap, HeaderValue, StatusCode,
        header::{CACHE_CONTROL, LOCATION, ORIGIN, SET_COOKIE},
    },
    response::{IntoResponse, Response},
    routing::get,
};
use serde::Deserialize;

use crate::{app::AppState, auth::AuthError};

use super::{
    authentication::{AuthenticatedSessionRequest, cookie_value},
    problem::Problem,
    representation::{SessionRepresentation, UserRepresentation, json_no_store},
};

const CSRF_HEADER: &str = "x-csrf-token";

pub(super) fn routes(oidc_enabled: bool) -> Router<AppState> {
    let app = Router::new().route("/api/v1/session", get(get_session).delete(delete_session));
    if oidc_enabled {
        app.route("/auth/oidc/start", get(start_oidc))
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
    callback: Result<Query<OidcCallback>, axum::extract::rejection::QueryRejection>,
) -> Response {
    let Ok(Query(callback)) = callback else {
        return login_failure(&state);
    };
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
