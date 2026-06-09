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

#[derive(Deserialize)]
pub struct CreateTaskRequest {
    pub title: String,
    pub status: Option<String>,
    pub priority: Option<String>,
    pub assigned_to: Option<Uuid>,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct Task {
    pub id: Uuid,
    pub title: String,
    pub status: String,
    pub priority: String,
    pub assigned_to: Option<Uuid>,
}

// --- auth helpers ---------------------------------------------------------

// Decode the bearer token from the Authorization header and return its claims.
// Any authenticated user passes this.
pub fn authenticate(headers: &HeaderMap) -> Result<Claims, (StatusCode, String)> {
    let auth = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or((StatusCode::UNAUTHORIZED, "missing Authorization header".into()))?;

    let token = auth
        .strip_prefix("Bearer ")
        .ok_or((StatusCode::UNAUTHORIZED, "expected Bearer token".into()))?;

    decode_token(token)
        .map_err(|_| (StatusCode::UNAUTHORIZED, "invalid or expired token".into()))
}

// Same as authenticate, but additionally requires role == "admin".
fn require_admin(headers: &HeaderMap) -> Result<Claims, (StatusCode, String)> {
    let claims = authenticate(headers)?;
    if claims.role != "admin" {
        return Err((StatusCode::FORBIDDEN, "admin only".into()));
    }
    Ok(claims)
}

// --- handlers -------------------------------------------------------------

// POST /tasks — admin only
pub async fn create_task(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<CreateTaskRequest>,
) -> Result<(StatusCode, Json<Task>), (StatusCode, String)> {
    require_admin(&headers)?;

    if body.title.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "title required".into()));
    }

    let status = body.status.unwrap_or_else(|| "todo".into());
    let priority = body.priority.unwrap_or_else(|| "medium".into());

    // If assigned_to is set, make sure the user exists so we return a clean
    // 400 instead of a 500 from the foreign-key violation.
    if let Some(uid) = body.assigned_to {
        let exists = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM users WHERE id = $1)",
        )
        .bind(uid)
        .fetch_one(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        if !exists {
            return Err((StatusCode::BAD_REQUEST, "assigned_to user not found".into()));
        }
    }

    let task = sqlx::query_as::<_, Task>(
        r#"
        INSERT INTO tasks (title, status, priority, assigned_to)
        VALUES ($1, $2, $3, $4)
        RETURNING id, title, status, priority, assigned_to
        "#,
    )
    .bind(body.title.trim())
    .bind(&status)
    .bind(&priority)
    .bind(body.assigned_to)
    .fetch_one(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok((StatusCode::CREATED, Json(task)))
}

#[derive(Deserialize)]
pub struct AssignRequest {
    pub task_ids: Vec<Uuid>,
    pub assignee_name: String,
}

#[derive(Serialize)]
pub struct AssignResponse {
    pub assigned_to: Uuid,
    pub assignee_name: String,
    pub updated_count: usize,
    pub updated_task_ids: Vec<Uuid>,
}

// POST /tasks/assign — admin only
pub async fn assign_tasks(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<AssignRequest>,
) -> Result<Json<AssignResponse>, (StatusCode, String)> {
    require_admin(&headers)?;

    if body.task_ids.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "task_ids cannot be empty".into()));
    }
    if body.assignee_name.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "assignee_name required".into()));
    }

    // Resolve the assignee by name. full_name isn't unique in the schema,
    // so guard against zero and multiple matches explicitly.
    let matches = sqlx::query_as::<_, (Uuid,)>(
        "SELECT id FROM users WHERE full_name = $1",
    )
    .bind(body.assignee_name.trim())
    .fetch_all(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let assignee_id = match matches.as_slice() {
        [] => {
            return Err((
                StatusCode::NOT_FOUND,
                format!("no user named '{}'", body.assignee_name.trim()),
            ))
        }
        [one] => one.0,
        _ => {
            return Err((
                StatusCode::CONFLICT,
                format!(
                    "multiple users named '{}' — assign by id instead",
                    body.assignee_name.trim()
                ),
            ))
        }
    };

    // Assign only the tasks that actually exist; RETURNING tells us which.
    let updated = sqlx::query_as::<_, (Uuid,)>(
        r#"
        UPDATE tasks
        SET assigned_to = $1
        WHERE id = ANY($2)
        RETURNING id
        "#,
    )
    .bind(assignee_id)
    .bind(&body.task_ids)
    .fetch_all(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let updated_task_ids: Vec<Uuid> = updated.into_iter().map(|r| r.0).collect();

    Ok(Json(AssignResponse {
        assigned_to: assignee_id,
        assignee_name: body.assignee_name.trim().to_string(),
        updated_count: updated_task_ids.len(),
        updated_task_ids,
    }))
}