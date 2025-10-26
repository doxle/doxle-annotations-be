use serde::{Deserialize, Serialize};

// ========== USER ==========
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct User {
    pub user_id: String,
    pub name: String,
    pub email: String,
    pub company: Option<String>,
    pub role: String, // admin | annotator | builder
    pub created_at: String,
    pub last_login: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateUserRequest {
    pub name: String,
    pub email: String,
    pub company: Option<String>,
    pub role: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateUserRequest {
    pub name: Option<String>,
    pub company: Option<String>,
    pub role: Option<String>,
}

// ========== PROJECT ==========
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Project {
    pub project_id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub project_type: String, // building | annotation
    pub locked: bool,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateProjectRequest {
    pub name: String,
    #[serde(rename = "type")]
    pub project_type: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateProjectRequest {
    pub name: Option<String>,
    pub locked: Option<bool>,
}

// ========== CLASS ==========
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Class {
    pub class_id: String,
    pub project_id: String,
    pub name: String,
    pub color: Option<String>,
    pub properties: Option<serde_json::Value>,
    pub count: u32,
}

#[derive(Debug, Deserialize)]
pub struct CreateClassRequest {
    pub name: String,
    pub color: Option<String>,
    pub properties: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateClassRequest {
    pub name: Option<String>,
    pub color: Option<String>,
    pub properties: Option<serde_json::Value>,
}

// ========== BLOCK ==========
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Block {
    pub block_id: String,
    pub project_id: String,
    pub name: String,
    pub state: String, // draft | current | review | complete | paid
    pub locked: bool,
    pub assigned_to: Option<String>, // USER#123
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateBlockRequest {
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateBlockRequest {
    pub name: Option<String>,
    pub state: Option<String>,
    pub locked: Option<bool>,
    pub assigned_to: Option<String>,
}

// ========== IMAGE ==========
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Image {
    pub image_id: String,
    pub block_id: String,
    pub url: String,
    pub locked: bool,
    pub order: Option<i32>,
    pub uploaded_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateImageRequest {
    pub url: String,
    pub order: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateImageRequest {
    pub locked: Option<bool>,
    pub order: Option<i32>,
}

// ========== ANNOTATION ==========
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum Geometry {
    #[serde(rename = "polygon")]
    Polygon { points: Vec<Point> },
    #[serde(rename = "bbox")]
    BBox { start: Point, end: Point },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Annotation {
    pub annotation_id: String,
    pub image_id: String,
    pub class_id: String,
    pub geometry: Geometry,
    pub created_by: String, // USER#123
    pub created_at: String,
    pub updated_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateAnnotationRequest {
    pub class_id: String,
    pub geometry: Geometry,
}

#[derive(Debug, Deserialize)]
pub struct UpdateAnnotationRequest {
    pub class_id: Option<String>,
    pub geometry: Option<Geometry>,
}

#[derive(Debug, Deserialize)]
pub struct BatchCreateAnnotationsRequest {
    pub annotations: Vec<CreateAnnotationRequest>,
}

// ========== COMMENT ==========
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Comment {
    pub comment_id: String,
    pub image_id: String,
    pub user_id: String,
    pub text: String,
    pub resolved: bool,
    pub created_at: String,
}
