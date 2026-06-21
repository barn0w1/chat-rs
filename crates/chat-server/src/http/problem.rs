use axum::{
    Json,
    extract::rejection::{FormRejection, JsonRejection},
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
        Self::field_validation(field, "out_of_range", Some(max))
    }

    pub(crate) fn field_validation(
        field: &'static str,
        code: &'static str,
        max: Option<usize>,
    ) -> Self {
        let mut problem = Self::new(
            "urn:chat-rs:problem:validation-failed",
            "Request validation failed",
            StatusCode::UNPROCESSABLE_ENTITY,
        );
        problem.errors.push(FieldError { field, code, max });
        problem
    }

    pub(crate) fn content_too_large() -> Self {
        Self::new(
            "urn:chat-rs:problem:content-too-large",
            "Request content is too large",
            StatusCode::PAYLOAD_TOO_LARGE,
        )
    }

    pub(crate) fn unsupported_media_type() -> Self {
        Self::new(
            "urn:chat-rs:problem:unsupported-media-type",
            "Request media type is not supported",
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
        )
    }

    pub(crate) fn from_json_rejection(rejection: JsonRejection) -> Self {
        match rejection.status() {
            StatusCode::PAYLOAD_TOO_LARGE => Self::content_too_large(),
            StatusCode::UNSUPPORTED_MEDIA_TYPE => Self::unsupported_media_type(),
            StatusCode::BAD_REQUEST | StatusCode::UNPROCESSABLE_ENTITY => Self::invalid_request(),
            status => {
                tracing::error!(%status, "unexpected JSON extraction failure");
                Self::internal()
            }
        }
    }

    pub(crate) fn from_form_rejection(rejection: FormRejection) -> Self {
        match rejection.status() {
            StatusCode::PAYLOAD_TOO_LARGE => Self::content_too_large(),
            StatusCode::UNSUPPORTED_MEDIA_TYPE => Self::unsupported_media_type(),
            StatusCode::BAD_REQUEST | StatusCode::UNPROCESSABLE_ENTITY => Self::invalid_request(),
            status => {
                tracing::error!(%status, "unexpected form extraction failure");
                Self::internal()
            }
        }
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
    #[serde(skip_serializing_if = "Option::is_none")]
    max: Option<usize>,
}
