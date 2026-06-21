use std::time::SystemTime;

use axum::{
    extract::FromRequestParts,
    http::{
        HeaderMap, HeaderValue,
        header::{COOKIE, ORIGIN},
        request::Parts,
    },
};
use chat::UserId;

use crate::{
    app::AppState,
    auth::{AuthenticatedSession, CookiePolicy},
};

use super::problem::Problem;

pub(super) const CSRF_HEADER: &str = "x-csrf-token";

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

#[derive(Clone, Copy, Debug)]
pub(crate) struct AuthenticatedMutation {
    user_id: UserId,
}

impl AuthenticatedMutation {
    pub(crate) const fn user_id(self) -> UserId {
        self.user_id
    }
}

impl FromRequestParts<AppState> for AuthenticatedMutation {
    type Rejection = Problem;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let session = authenticate(&parts.headers, state).await?;
        let csrf = single_header(&parts.headers, CSRF_HEADER).and_then(|value| value.to_str().ok());
        if !valid_origin(&parts.headers, &state.expected_origin)
            || csrf.is_none_or(|value| !session.verifies_csrf(value))
        {
            return Err(Problem::forbidden());
        }

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

pub(super) fn valid_origin(headers: &HeaderMap, expected: &str) -> bool {
    single_header(headers, ORIGIN.as_str())
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value == expected)
}

fn single_header<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a HeaderValue> {
    let mut values = headers.get_all(name).iter();
    let value = values.next()?;
    values.next().is_none().then_some(value)
}
