use axum::{extract::State, http::StatusCode, Json};
use chrono::{Duration, Utc};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::{auth::verify_password, AppState};
use crate::jwt::issue_token;

#[derive(Serialize)]
pub struct VerifyResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: u64,
}
#[derive(Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct LoginResponse {
    pub challenge_id: Uuid,
    pub message: String,
}



#[derive(Deserialize)]
pub struct VerifyRequest {
    pub challenge_id: Uuid,
    pub code: String,
}

// minimal credential check helper
struct AuthRow {
    id: Uuid,
    hashed_password: String,
}

pub async fn login(
    State(state): State<Arc<AppState>>,
    Json(body): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, (StatusCode, String)> {
    // basic email shape validation
    if !body.email.contains('@') || body.email.len() < 3 {
        return Err((StatusCode::BAD_REQUEST, "invalid email".into()));
    }
    if body.password.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "password required".into()));
    }

    // look up user
    let row = sqlx::query_as::<_, (Uuid, String)>(
        "SELECT id, hashed_password FROM users WHERE email = $1",
    )
    .bind(&body.email)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // generic error to avoid leaking which part failed
    let auth = match row {
        Some((id, hashed_password)) => AuthRow { id, hashed_password },
        None => return Err((StatusCode::UNAUTHORIZED, "invalid credentials".into())),
    };

    if !verify_password(&body.password, &auth.hashed_password) {
        return Err((StatusCode::UNAUTHORIZED, "invalid credentials".into()));
    }

    // create 2FA challenge
    let code: u32 = rand::thread_rng().gen_range(100_000..1_000_000);
    let code_str = code.to_string();
    let expires_at = Utc::now() + Duration::minutes(5);

    let challenge_id = sqlx::query_scalar::<_, Uuid>(
        r#"
        INSERT INTO login_challenges (user_id, code, expires_at)
        VALUES ($1, $2, $3)
        RETURNING id
        "#,
    )
    .bind(auth.id)
    .bind(&code_str)
    .bind(expires_at)
    .fetch_one(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // "send" the code — just log it
    println!(
        "[2FA] code for {} (challenge {}): {}",
        body.email, challenge_id, code_str
    );

    Ok(Json(LoginResponse {
        challenge_id,
        message: "2FA code sent. Check the server console.".into(),
    }))
}

pub async fn verify_login(
    State(state): State<Arc<AppState>>,
    Json(body): Json<VerifyRequest>,
) -> Result<Json<VerifyResponse>, (StatusCode, String)> {
    // fetch challenge + the user's role in one go
    let row = sqlx::query_as::<_, (Uuid, String, chrono::DateTime<Utc>, bool, String)>(
        r#"
        SELECT c.user_id, c.code, c.expires_at, c.consumed, u.role
        FROM login_challenges c
        JOIN users u ON u.id = c.user_id
        WHERE c.id = $1
        "#,
    )
    .bind(body.challenge_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let (user_id, code, expires_at, consumed, role) = row
        .ok_or((StatusCode::NOT_FOUND, "challenge not found".into()))?;

    if consumed {
        return Err((StatusCode::BAD_REQUEST, "challenge already used".into()));
    }
    if Utc::now() > expires_at {
        return Err((StatusCode::BAD_REQUEST, "challenge expired".into()));
    }
    if code != body.code {
        return Err((StatusCode::UNAUTHORIZED, "incorrect code".into()));
    }

    // mark consumed
    sqlx::query("UPDATE login_challenges SET consumed = true WHERE id = $1")
        .bind(body.challenge_id)
        .execute(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let token = issue_token(user_id, &role)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    Ok(Json(VerifyResponse {
        access_token: token,
        token_type: "Bearer".into(),
        expires_in: 3600,
    }))
}