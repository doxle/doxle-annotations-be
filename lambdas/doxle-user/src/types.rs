use serde::{Deserialize, Serialize};

// User stored in DynamoDB
#[derive(Debug, Serialize, Deserialize)]
pub struct User {
    pub user_id: String,
    pub email: String,
    pub role: String, // admin | annotator | builder
    pub created_at: String,
}

// Request body for creating a new user
#[derive(Debug, Deserialize)]
pub struct CreateUserRequest {
    pub email: String,
    pub role: String, // admin | annotator | builder
}

// Request body for updating user
#[derive(Debug, Deserialize)]
pub struct UpdateUserRequest {
    pub role: Option<String>, // admin | annotator | builder (email is immutable)
}
