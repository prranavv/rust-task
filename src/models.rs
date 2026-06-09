use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};

#[derive(Serialize, sqlx::FromRow)]
pub struct User {
    pub id: Uuid,
    pub full_name: String,
    pub email: String,
    #[serde(skip_serializing)]
    pub hashed_password: String,
    pub role: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct Task {
    pub id: Uuid,
    pub title: String,
    pub status: String,
    pub priority: String,
    pub assigned_to: Option<Uuid>,
}