use aws_sdk_apigatewaymanagement::Client as ApiGatewayManagementClient;
use aws_sdk_dynamodb::Client as DynamoClient;
use lambda_http::Error;
use super::messages::BroadcastMessage;
use super::connections::_get_all_connections;

/// Broadcast a message to all connected WebSocket clients
pub async fn _broadcast_to_all(
    dynamo_client: &DynamoClient,
    api_gateway_client: &ApiGatewayManagementClient,
    table_name: &str,
    message: &BroadcastMessage,
) -> Result<(), Error> {
    let connections = _get_all_connections(dynamo_client, table_name).await?;
    let message_json = serde_json::to_string(message)?;
    
    tracing::info!("Broadcasting to {} connections", connections.len());
    
    for conn in connections {
        let result = api_gateway_client
            .post_to_connection()
            .connection_id(&conn.connection_id)
            .data(message_json.as_bytes().to_vec().into())
            .send()
            .await;
        
        if let Err(e) = result {
            tracing::warn!(
                "Failed to send to connection {}: {}. Connection may be stale.",
                conn.connection_id,
                e
            );
            // Optionally: remove stale connection from DynamoDB
            // remove_connection(dynamo_client, table_name, &conn.connection_id).await.ok();
        }
    }
    
    Ok(())
}

/// Broadcast to specific connections (e.g., by user_id or project_id)
pub async fn _broadcast_to_connections(
    api_gateway_client: &ApiGatewayManagementClient,
    connection_ids: Vec<String>,
    message: &BroadcastMessage,
) -> Result<(), Error> {
    let message_json = serde_json::to_string(message)?;
    
    for connection_id in connection_ids {
        let result = api_gateway_client
            .post_to_connection()
            .connection_id(&connection_id)
            .data(message_json.as_bytes().to_vec().into())
            .send()
            .await;
        
        if let Err(e) = result {
            tracing::warn!("Failed to send to connection {}: {}", connection_id, e);
        }
    }
    
    Ok(())
}
