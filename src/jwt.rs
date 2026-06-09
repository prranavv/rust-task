use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Serialize, Deserialize)]
pub struct Claims {
    pub sub: Uuid,    // user id
    pub role: String, // "admin" / "user"
    pub exp: usize,   // expiry (unix seconds)
    pub iat: usize,   // issued at
}

fn secret() -> String {
    std::env::var("JWT_SECRET").unwrap_or_else(|_| "dev-insecure-secret-change-me".into())
}

pub fn issue_token(user_id: Uuid, role: &str) -> Result<String, String> {
    let now = Utc::now();
    let claims = Claims {
        sub: user_id,
        role: role.to_string(),
        iat: now.timestamp() as usize,
        exp: (now + Duration::hours(1)).timestamp() as usize,
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret().as_bytes()),
    )
    .map_err(|e| e.to_string())
}

pub fn decode_token(token: &str) -> Result<Claims, String> {
    decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret().as_bytes()),
        &Validation::default(),
    )
    .map(|data| data.claims)
    .map_err(|e| e.to_string())
}