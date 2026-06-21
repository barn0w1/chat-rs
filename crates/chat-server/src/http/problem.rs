use axum::{
    Json,
    http::{
        HeaderValue, StatusCode,
        header::{CACHE_CONTROL, CONTENT_TYPE, WWW_AUTHENTICATE},
    },
    response::{IntoResponse, Response},
};
use serde::Serialize;

const AUTHENTICATION_CHALLENGE: &str = "ChatSession realm=\"chat\"";

#[derive(Debug)]
pub(crate) struct Problem {
    type_uri: &'static str,
    title: &'static str,
    status: StatusCode,
    errors: Vec<FieldError>,
}

impl Problem {
    pub(crate) fn unauthenticated() -> Self {
        Self::new(
            "urn:chat-rs:problem:authentication-required",
            "Authentication required",
            StatusCode::UNAUTHORIZED,
        )
    }

    pub(crate) fn forbidden() -> Self {
        Self::new(
            "urn:chat-rs:problem:forbidden",
            "Request is not permitted",
            StatusCode::FORBIDDEN,
        )
    }

    pub(crate) fn invalid_request() -> Self {
        Self::new(
            "urn:chat-rs:problem:invalid-request",
            "Request is invalid",
            StatusCode::BAD_REQUEST,
        )
    }

    pub(crate) fn validation(field: &'static str, max: usize) -> Self {
        let mut problem = Self::new(
            "urn:chat-rs:problem:validation-failed",
            "Request validation failed",
            StatusCode::UNPROCESSABLE_ENTITY,
        );
        problem.errors.push(FieldError {
            field,
            code: "out_of_range",
            max,
        });
        problem
    }

    pub(crate) fn not_found() -> Self {
        Self::new(
            "urn:chat-rs:problem:not-found",
            "Resource not found",
            StatusCode::NOT_FOUND,
        )
    }

    pub(crate) fn login_failed() -> Self {
        Self::new(
            "urn:chat-rs:problem:login-failed",
            "Login could not be completed",
            StatusCode::UNAUTHORIZED,
        )
    }

    pub(crate) fn unavailable() -> Self {
        Self::new(
            "urn:chat-rs:problem:service-unavailable",
            "Service temporarily unavailable",
            StatusCode::SERVICE_UNAVAILABLE,
        )
    }

    pub(crate) fn internal() -> Self {
        Self::new(
            "urn:chat-rs:problem:internal",
            "Internal server error",
            StatusCode::INTERNAL_SERVER_ERROR,
        )
    }

    fn new(type_uri: &'static str, title: &'static str, status: StatusCode) -> Self {
        Self {
            type_uri,
            title,
            status,
            errors: Vec::new(),
        }
    }
}

impl IntoResponse for Problem {
    fn into_response(self) -> Response {
        let status = self.status;
        let document = ProblemDocument {
            type_uri: self.type_uri,
            title: self.title,
            status: status.as_u16(),
            errors: self.errors,
        };
        let mut response = (status, Json(document)).into_response();
        response.headers_mut().insert(
            CONTENT_TYPE,
            HeaderValue::from_static("application/problem+json"),
        );
        response
            .headers_mut()
            .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
        if status == StatusCode::UNAUTHORIZED {
            response.headers_mut().insert(
                WWW_AUTHENTICATE,
                HeaderValue::from_static(AUTHENTICATION_CHALLENGE),
            );
        }
        response
    }
}

#[derive(Debug, Serialize)]
struct ProblemDocument {
    #[serde(rename = "type")]
    type_uri: &'static str,
    title: &'static str,
    status: u16,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    errors: Vec<FieldError>,
}

#[derive(Debug, Serialize)]
struct FieldError {
    field: &'static str,
    code: &'static str,
    max: usize,
}
