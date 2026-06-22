mod authentication;
mod conversation;
mod problem;
mod realtime;
mod representation;
mod session;

use axum::Router;

use crate::app::AppState;

pub(crate) fn routes(oidc_enabled: bool) -> Router<AppState> {
    session::routes(oidc_enabled)
        .merge(conversation::routes())
        .merge(realtime::routes())
}
