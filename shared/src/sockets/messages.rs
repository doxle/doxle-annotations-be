use serde::{Deserialize, Serialize};

/// Incoming WebSocket message from client
#[derive(Debug, Deserialize)]
pub struct WebSocketMessage {
    pub action: String,
    #[serde(flatten)]
    pub data: serde_json::Value,
}

/// WebSocket action types
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WebSocketAction {
    // Project actions
    CreateProject,
    UpdateProject,
    DeleteProject,
    
    // Block actions
    CreateBlock,
    UpdateBlock,
    DeleteBlock,
    
    // Image actions
    CreateImage,
    UpdateImage,
    DeleteImage,
    
    // Annotation actions
    CreateAnnotation,
    UpdateAnnotation,
    DeleteAnnotation,
    BatchCreateAnnotations,
    
    // Class actions
    CreateClass,
    UpdateClass,
    DeleteClass,
}

/// Broadcast message sent to all clients
#[derive(Debug, Serialize)]
pub struct BroadcastMessage {
    pub r#type: String,
    #[serde(flatten)]
    pub data: serde_json::Value,
}

impl BroadcastMessage {
    pub fn _new(message_type: &str, data: serde_json::Value) -> Self {
        Self {
            r#type: message_type.to_string(),
            data,
        }
    }
    
    pub fn _project_created(project: &crate::types::Project) -> Self {
        Self::_new("project_created", serde_json::to_value(project).unwrap())
    }
    
    pub fn _project_deleted(project_id: &str) -> Self {
        Self::_new("project_deleted", serde_json::json!({"project_id": project_id}))
    }
    
    pub fn _project_updated(project: &crate::types::Project) -> Self {
        Self::_new("project_updated", serde_json::to_value(project).unwrap())
    }
}
