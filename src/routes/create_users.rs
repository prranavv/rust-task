use axum::{extract::State, http::StatusCode, Json};
use std::sync::Arc;
use crate::{AppState, models::User};
use crate::auth::hash_password;

pub async fn create_users(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<User>>, (StatusCode, String)> {
    let seed = vec![
        ("Admin", "alice@example.com", "password123", "admin"),
        ("James Bond", "bob@example.com", "password123", "staff"),
    ];

    let mut inserted = Vec::new();
    for (name, email, pw, role) in seed {
        let hashed = hash_password(pw)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
        let user = sqlx::query_as::<_, User>(
            r#"
            INSERT INTO users (full_name, email, hashed_password, role)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (email) DO UPDATE SET full_name = EXCLUDED.full_name
            RETURNING id, full_name, email, hashed_password, role, created_at, updated_at
            "#,
        )
        .bind(name)
        .bind(email)
        .bind(&hashed)
        .bind(role)
        .fetch_one(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        inserted.push(user);
    }

    Ok(Json(inserted))
}