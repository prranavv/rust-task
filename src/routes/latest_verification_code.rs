use axum::{extract::State, http::StatusCode, Json};
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::sync::Arc;
use uuid::Uuid;

use crate::AppState;

#[derive(Serialize)]
pub struct LatestCode {
    pub challenge_id: Uuid,
    pub user_id: Uuid,
    pub email: String,
    pub code: String,
    pub expires_at: DateTime<Utc>,
    pub consumed: bool,
}

pub async fn latest_code(
    State(state): State<Arc<AppState>>,
) -> Result<Json<LatestCode>, (StatusCode, String)> {
    // runtime guard — refuse unless explicitly in dev mode
    if std::env::var("APP_ENV").unwrap_or_default() != "development" {
        return Err((StatusCode::NOT_FOUND, "not found".into()));
    }

    let row = sqlx::query_as::<_, (Uuid, Uuid, String, String, DateTime<Utc>, bool)>(
        r#"
        SELECT c.id, c.user_id, u.email, c.code, c.expires_at, c.consumed
        FROM login_challenges c
        JOIN users u ON u.id = c.user_id
        ORDER BY c.created_at DESC
        LIMIT 1
        "#,
    )
    .fetch_optional(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let (challenge_id, user_id, email, code, expires_at, consumed) =
        row.ok_or((StatusCode::NOT_FOUND, "no challenges yet".into()))?;

    Ok(Json(LatestCode {
        challenge_id,
        user_id,
        email,
        code,
        expires_at,
        consumed,
    }))
}