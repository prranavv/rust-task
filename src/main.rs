use axum::{routing::{get,post}, Router};
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use uuid::Uuid;

pub mod routes;
pub mod models;
pub mod auth;
pub mod jwt;

use routes::create_users::create_users;
use routes::login::*;
use routes::latest_verification_code::latest_code;
use routes::create_task::{create_task,assign_tasks};
use routes::view_task::view_my_tasks;
pub struct CachedTasks {
    pub value: serde_json::Value,
    pub stored_at: Instant,
}

pub struct AppState {
    pub db: sqlx::PgPool,
    pub task_cache: Mutex<HashMap<Uuid, CachedTasks>>,
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://appuser:secret@localhost:5432/appdb".into());

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await
        .expect("failed to connect to postgres");

    let state = Arc::new(AppState {
        db: pool,
        task_cache: Mutex::new(HashMap::new()),
    });
    let app = Router::new()
    .route("/seed/users", get(create_users))
    .route("/auth/login", post(login))
    .route("/auth/verify-2fa", post(verify_login))
    .route("/dev/email-logs/latest", get(latest_code))
    .route("/tasks", post(create_task))
    .route("/tasks/assign", post(assign_tasks))
    .route("/tasks/view-my-tasks", get(view_my_tasks))
    .with_state(state);
    
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("listening on 0.0.0.0:3000");
    axum::serve(listener, app).await.unwrap();
}