use aws_sdk_dynamodb::Client as DynamoClient;
use lambda_http::Error;
use serde::{Deserialize, Serialize};

/// WebSocket connection stored in DynamoDB
#[derive(Debug, Serialize, Deserialize)]
pub struct Connection {
    pub connection_id: String,
    pub user_id: String,
    pub connected_at: String,
}

/// Save a WebSocket connection to DynamoDB
pub async fn save_connection(
    client: &DynamoClient,
    table_name: &str,
    connection_id: &str,
    user_id: &str,
) -> Result<(), Error> {
    let now = chrono::Utc::now().to_rfc3339();
    let pk = format!("CONNECTION#{}", connection_id);
    
    client
        .put_item()
        .table_name(table_name)
        .item("PK", aws_sdk_dynamodb::types::AttributeValue::S(pk.clone()))
        .item("SK", aws_sdk_dynamodb::types::AttributeValue::S(pk))
        .item("connection_id", aws_sdk_dynamodb::types::AttributeValue::S(connection_id.to_string()))
        .item("user_id", aws_sdk_dynamodb::types::AttributeValue::S(user_id.to_string()))
        .item("connected_at", aws_sdk_dynamodb::types::AttributeValue::S(now))
        .item("entity_type", aws_sdk_dynamodb::types::AttributeValue::S("connection".to_string()))
        .send()
        .await?;
    
    tracing::info!("Connection saved: {} (user: {})", connection_id, user_id);
    Ok(())
}

/// Remove a WebSocket connection from DynamoDB
pub async fn remove_connection(
    client: &DynamoClient,
    table_name: &str,
    connection_id: &str,
) -> Result<(), Error> {
    let pk = format!("CONNECTION#{}", connection_id);
    
    client
        .delete_item()
        .table_name(table_name)
        .key("PK", aws_sdk_dynamodb::types::AttributeValue::S(pk.clone()))
        .key("SK", aws_sdk_dynamodb::types::AttributeValue::S(pk))
        .send()
        .await?;
    
    tracing::info!("Connection removed: {}", connection_id);
    Ok(())
}

/// Get all active WebSocket connections
pub async fn _get_all_connections(
    client: &DynamoClient,
    table_name: &str,
) -> Result<Vec<Connection>, Error> {
    let mut connections = Vec::new();
    
    let result = client
        .scan()
        .table_name(table_name)
        .filter_expression("entity_type = :type")
        .expression_attribute_values(
            ":type",
            aws_sdk_dynamodb::types::AttributeValue::S("connection".to_string()),
        )
        .send()
        .await?;
    
    if let Some(items) = result.items {
        for item in items {
            if let (Some(conn_id), Some(user_id), Some(connected_at)) = (
                item.get("connection_id").and_then(|v| v.as_s().ok()),
                item.get("user_id").and_then(|v| v.as_s().ok()),
                item.get("connected_at").and_then(|v| v.as_s().ok()),
            ) {
                connections.push(Connection {
                    connection_id: conn_id.clone(),
                    user_id: user_id.clone(),
                    connected_at: connected_at.clone(),
                });
            }
        }
    }
    
    Ok(connections)
}
