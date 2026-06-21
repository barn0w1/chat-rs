use std::time::SystemTime;

use axum::{
    extract::FromRequestParts,
    http::{HeaderMap, header::COOKIE, request::Parts},
};
use chat::UserId;

use crate::{
    app::AppState,
    auth::{AuthenticatedSession, CookiePolicy},
};

use super::problem::Problem;

#[derive(Clone, Copy, Debug)]
pub(crate) struct AuthenticatedUser {
    user_id: UserId,
}

impl AuthenticatedUser {
    pub(crate) const fn user_id(self) -> UserId {
        self.user_id
    }
}

impl FromRequestParts<AppState> for AuthenticatedUser {
    type Rejection = Problem;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let session = authenticate(&parts.headers, state).await?;
        Ok(Self {
            user_id: session.user_id(),
        })
    }
}

pub(crate) struct AuthenticatedSessionRequest {
    session: AuthenticatedSession,
}

impl AuthenticatedSessionRequest {
    pub(crate) fn into_session(self) -> AuthenticatedSession {
        self.session
    }
}

impl FromRequestParts<AppState> for AuthenticatedSessionRequest {
    type Rejection = Problem;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        authenticate(&parts.headers, state)
            .await
            .map(|session| Self { session })
    }
}

async fn authenticate(
    headers: &HeaderMap,
    state: &AppState,
) -> Result<AuthenticatedSession, Problem> {
    let token = cookie_value(headers, &state.cookies, state.cookies.session_name())
        .ok_or(Problem::unauthenticated())?;
    match state.auth.resolve_session(&token, SystemTime::now()).await {
        Ok(Some(session)) => Ok(session),
        Ok(None) => Err(Problem::unauthenticated()),
        Err(error) => {
            tracing::error!(error = %error, "session authentication failed");
            Err(Problem::unavailable())
        }
    }
}

pub(crate) fn cookie_value(
    headers: &HeaderMap,
    policy: &CookiePolicy,
    name: &str,
) -> Option<String> {
    headers
        .get_all(COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find_map(|value| policy.find(value, name))
}
