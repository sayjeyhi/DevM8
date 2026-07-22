use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::Response;

use super::ApiState;

/// Identity resolved from a valid bearer token, injected as a request extension
/// by `require_bearer_auth` for handlers to pull out via `Extension<AuthedUser>`.
#[derive(Debug, Clone)]
pub struct AuthedUser {
    pub email: String,
    pub is_admin: bool,
}

pub async fn require_bearer_auth(
    State(api_state): State<ApiState>,
    mut req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let token = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or(StatusCode::UNAUTHORIZED)?
        .to_string();

    let (email, is_admin) = api_state
        .app_state
        .db
        .verify_token(&token)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::UNAUTHORIZED)?;

    req.extensions_mut().insert(AuthedUser { email, is_admin });
    Ok(next.run(req).await)
}
