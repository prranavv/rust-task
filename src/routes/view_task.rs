use std::time::{Duration, Instant};
use crate::CachedTasks;
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;
use crate::jwt::{decode_token, Claims};
use crate::AppState;
use crate::routes::create_task::authenticate;

const CACHE_TTL: Duration = Duration::from_secs(30);

// GET /tasks/view-my-tasks — any authenticated user, sees only their own tasks
pub async fn view_my_tasks(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let claims = authenticate(&headers)?;
    let user_id = claims.sub;

    // 1. cache lookup
    {
        let cache = state.task_cache.lock().unwrap();
        if let Some(entry) = cache.get(&user_id) {
            if entry.stored_at.elapsed() < CACHE_TTL {
                let mut hit = entry.value.clone();
                hit["cache"] = serde_json::json!({ "hit": true });
                return Ok(Json(hit));
            }
        }
    } // lock released before we touch the DB

    // 2. who is this user (email + role for the response header block)
    let (email, role) = sqlx::query_as::<_, (String, String)>(
        "SELECT email, role FROM users WHERE id = $1",
    )
    .bind(user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::UNAUTHORIZED, "unknown user".into()))?;

    // 3. their tasks — assigned_to rendered as the user's email
    let rows = sqlx::query_as::<_, (Uuid, String, String, String)>(
        r#"
        SELECT id, title, status, priority
        FROM tasks
        WHERE assigned_to = $1
        ORDER BY id
        "#,
    )
    .bind(user_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let tasks: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|(id, title, status, priority)| {
            serde_json::json!({
                "id": id,
                "title": title,
                "status": status,
                "priority": priority,
                "assigned_to": email,
            })
        })
        .collect();

    let body = serde_json::json!({
        "user": { "email": email, "role": role },
        "tasks": tasks,
        "summary": { "total_assigned_tasks": tasks.len() },
        "cache": { "hit": false },
    });

    // 4. store in cache (with hit:false baked in; we overwrite to true on hit)
    {
        let mut cache = state.task_cache.lock().unwrap();
        cache.insert(
            user_id,
            CachedTasks { value: body.clone(), stored_at: Instant::now() },
        );
    }

    Ok(Json(body))
}