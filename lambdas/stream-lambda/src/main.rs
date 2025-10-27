use aws_config;
use aws_lambda_events::event::dynamodb::{Event, EventRecord};
use aws_sdk_apigatewaymanagement::Client as ApiGatewayManagementClient;
use aws_sdk_dynamodb::Client as DynamoClient;
use doxle_shared::sockets::broadcast::_broadcast_to_all;
use doxle_shared::sockets::messages::BroadcastMessage;
use lambda_runtime::{run, service_fn, Error, LambdaEvent};

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .without_time()
        .init();

    run(service_fn(function_handler)).await
}

async fn function_handler(event: LambdaEvent<Event>) -> Result<(), Error> {
    tracing::info!("DynamoDB Stream event received with {} records", event.payload.records.len());

    // Initialize AWS clients
    let config = aws_config::load_from_env().await;
    let dynamo_client = DynamoClient::new(&config);

    // Get WebSocket API endpoint from environment
    let ws_endpoint = std::env::var("WS_API_ENDPOINT")
        .expect("WS_API_ENDPOINT must be set for stream handler");

    let api_config = aws_sdk_apigatewaymanagement::config::Builder::from(&config)
        .endpoint_url(ws_endpoint)
        .build();
    let api_gateway_client = ApiGatewayManagementClient::from_conf(api_config);

    let table_name = std::env::var("TABLE_NAME").unwrap_or_else(|_| "doxle-annotations".to_string());

    // Process each record
    for record in event.payload.records {
        if let Err(e) = process_record(&record, &dynamo_client, &api_gateway_client, &table_name).await {
            tracing::error!("Failed to process record: {}", e);
        }
    }

    Ok(())
}

async fn process_record(
    record: &EventRecord,
    dynamo_client: &DynamoClient,
    api_gateway_client: &ApiGatewayManagementClient,
    table_name: &str,
) -> Result<(), Error> {
    let event_name = &record.event_name;

    tracing::info!("Processing {} event", event_name);

    // Determine entity type from PK
    // For REMOVE events, new_image is empty; use old_image instead
    let image = if record.change.new_image.is_empty() {
        &record.change.old_image
    } else {
        &record.change.new_image
    };
    
    let pk = image.get("PK")
        .and_then(|attr| {
            // Convert to string - the AttributeValue should be a String variant
            serde_json::to_value(attr).ok()
                .and_then(|v| v.as_str().map(|s| s.to_string()))
        })
        .ok_or("Missing PK")?;
    
    let pk_str = pk.as_str();

    // Skip connection records (they're not data changes)
    if pk_str.starts_with("CONNECTION#") {
        return Ok(());
    }

    // Determine entity type and create appropriate broadcast message
    let message = match event_name.as_str() {
        "INSERT" => {
            if pk_str.starts_with("PROJECT#") {
                create_project_broadcast(record, "project_created")?
            } else if pk_str.starts_with("BLOCK#") {
                create_entity_broadcast(record, "block_created")?
            } else if pk_str.starts_with("IMAGE#") {
                create_entity_broadcast(record, "image_created")?
            } else if pk_str.starts_with("ANNOTATION#") {
                create_entity_broadcast(record, "annotation_created")?
            } else if pk_str.starts_with("CLASS#") {
                create_entity_broadcast(record, "class_created")?
            } else {
                return Ok(()); // Skip unknown entities
            }
        }
        "MODIFY" => {
            if pk_str.starts_with("PROJECT#") {
                create_project_broadcast(record, "project_updated")?
            } else if pk_str.starts_with("BLOCK#") {
                create_entity_broadcast(record, "block_updated")?
            } else if pk_str.starts_with("IMAGE#") {
                create_entity_broadcast(record, "image_updated")?
            } else if pk_str.starts_with("ANNOTATION#") {
                create_entity_broadcast(record, "annotation_updated")?
            } else if pk_str.starts_with("CLASS#") {
                create_entity_broadcast(record, "class_updated")?
            } else {
                return Ok(());
            }
        }
        "REMOVE" => {
            // For deletes, we only have the old image
            let entity_id = extract_id_from_pk(pk_str);
            let message_type = if pk_str.starts_with("PROJECT#") {
                "project_deleted"
            } else if pk_str.starts_with("BLOCK#") {
                "block_deleted"
            } else if pk_str.starts_with("IMAGE#") {
                "image_deleted"
            } else if pk_str.starts_with("ANNOTATION#") {
                "annotation_deleted"
            } else if pk_str.starts_with("CLASS#") {
                "class_deleted"
            } else {
                return Ok(());
            };

            BroadcastMessage::_new(message_type, serde_json::json!({ "id": entity_id }))
        }
        _ => return Ok(()),
    };

    // Broadcast to all connected WebSocket clients
    _broadcast_to_all(dynamo_client, api_gateway_client, table_name, &message).await?;

    tracing::info!("Broadcast sent: {}", message.r#type);

    Ok(())
}

fn create_project_broadcast(record: &EventRecord, message_type: &str) -> Result<BroadcastMessage, Error> {
    let new_image = &record.change.new_image;

    // Convert DynamoDB AttributeValue HashMap to JSON
    let json_data = serde_json::to_value(new_image)?;

    Ok(BroadcastMessage::_new(message_type, json_data))
}

fn create_entity_broadcast(record: &EventRecord, message_type: &str) -> Result<BroadcastMessage, Error> {
    let new_image = &record.change.new_image;

    let json_data = serde_json::to_value(new_image)?;

    Ok(BroadcastMessage::_new(message_type, json_data))
}

fn extract_id_from_pk(pk: &str) -> String {
    pk.split('#').nth(1).unwrap_or(pk).to_string()
}
