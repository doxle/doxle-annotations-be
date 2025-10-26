use lambda_http::{Body, Error, Response, http::StatusCode};
use aws_sdk_dynamodb::Client as DynamoClient;
use crate::types::{Block, CreateBlockRequest, UpdateBlockRequest};

/// Create a new block in a project
pub async fn create_block(
    client: &DynamoClient,
    table_name: &str,
    project_id: &str,
    body: &[u8],
) -> Result<Response<Body>, Error> {
    let req: CreateBlockRequest = serde_json::from_slice(body)?;
    
    let block_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let pk = format!("PROJECT#{}", project_id);
    let sk = format!("BLOCK#{}", block_id);
    
    // Store block
    client
        .put_item()
        .table_name(table_name)
        .item("PK", aws_sdk_dynamodb::types::AttributeValue::S(pk.clone()))
        .item("SK", aws_sdk_dynamodb::types::AttributeValue::S(sk.clone()))
        .item("name", aws_sdk_dynamodb::types::AttributeValue::S(req.name.clone()))
        .item("state", aws_sdk_dynamodb::types::AttributeValue::S("draft".to_string()))
        .item("locked", aws_sdk_dynamodb::types::AttributeValue::Bool(false))
        .item("created_at", aws_sdk_dynamodb::types::AttributeValue::S(now.clone()))
        .send()
        .await?;
    
    // Also store with BLOCK as PK for easy lookups
    client
        .put_item()
        .table_name(table_name)
        .item("PK", aws_sdk_dynamodb::types::AttributeValue::S(sk.clone()))
        .item("SK", aws_sdk_dynamodb::types::AttributeValue::S(sk.clone()))
        .item("project_id", aws_sdk_dynamodb::types::AttributeValue::S(pk))
        .item("name", aws_sdk_dynamodb::types::AttributeValue::S(req.name.clone()))
        .item("state", aws_sdk_dynamodb::types::AttributeValue::S("draft".to_string()))
        .item("locked", aws_sdk_dynamodb::types::AttributeValue::Bool(false))
        .item("created_at", aws_sdk_dynamodb::types::AttributeValue::S(now.clone()))
        .send()
        .await?;
    
    let block = Block {
        block_id: block_id.clone(),
        project_id: project_id.to_string(),
        name: req.name,
        state: "draft".to_string(),
        locked: false,
        assigned_to: None,
        created_at: now,
    };
    
    Ok(Response::builder()
        .status(StatusCode::CREATED)
        .header("Content-Type", "application/json")
        .header("Access-Control-Allow-Origin", "*")
        .body(serde_json::to_string(&block)?.into())
        .map_err(Box::new)?)
}

/// Get a specific block
pub async fn get_block(
    client: &DynamoClient,
    table_name: &str,
    block_id: &str,
) -> Result<Response<Body>, Error> {
    let pk = format!("BLOCK#{}", block_id);
    
    let result = client
        .get_item()
        .table_name(table_name)
        .key("PK", aws_sdk_dynamodb::types::AttributeValue::S(pk.clone()))
        .key("SK", aws_sdk_dynamodb::types::AttributeValue::S(pk))
        .send()
        .await?;
    
    if let Some(item) = result.item() {
        let block = Block {
            block_id: block_id.to_string(),
            project_id: item.get("project_id").and_then(|v| v.as_s().ok()).map(|s| s.to_string()).unwrap_or_default(),
            name: item.get("name").and_then(|v| v.as_s().ok()).map(|s| s.to_string()).unwrap_or_default(),
            state: item.get("state").and_then(|v| v.as_s().ok()).map(|s| s.to_string()).unwrap_or_default(),
            locked: item.get("locked").and_then(|v| v.as_bool().ok()).copied().unwrap_or(false),
            assigned_to: item.get("assigned_to").and_then(|v| v.as_s().ok()).map(|s| s.to_string()),
            created_at: item.get("created_at").and_then(|v| v.as_s().ok()).map(|s| s.to_string()).unwrap_or_default(),
        };
        
        Ok(Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "application/json")
            .header("Access-Control-Allow-Origin", "*")
            .body(serde_json::to_string(&block)?.into())
            .map_err(Box::new)?)
    } else {
        Ok(Response::builder()
            .status(StatusCode::NOT_FOUND)
            .header("Content-Type", "application/json")
            .header("Access-Control-Allow-Origin", "*")
            .body(serde_json::json!({"error": "Block not found"}).to_string().into())
            .map_err(Box::new)?)
    }
}

/// List all blocks for a project
pub async fn list_project_blocks(
    client: &DynamoClient,
    table_name: &str,
    project_id: &str,
) -> Result<Response<Body>, Error> {
    let pk = format!("PROJECT#{}", project_id);
    
    let result = client
        .query()
        .table_name(table_name)
        .key_condition_expression("PK = :pk AND begins_with(SK, :sk_prefix)")
        .expression_attribute_values(":pk", aws_sdk_dynamodb::types::AttributeValue::S(pk))
        .expression_attribute_values(":sk_prefix", aws_sdk_dynamodb::types::AttributeValue::S("BLOCK#".to_string()))
        .send()
        .await?;
    
    let mut blocks = Vec::new();
    
    for item in result.items() {
            if let Some(sk) = item.get("SK").and_then(|v| v.as_s().ok()) {
                if let Some(block_id) = sk.strip_prefix("BLOCK#") {
                    let block = Block {
                        block_id: block_id.to_string(),
                        project_id: project_id.to_string(),
                        name: item.get("name").and_then(|v| v.as_s().ok()).map(|s| s.to_string()).unwrap_or_default(),
                        state: item.get("state").and_then(|v| v.as_s().ok()).map(|s| s.to_string()).unwrap_or_default(),
                        locked: item.get("locked").and_then(|v| v.as_bool().ok()).copied().unwrap_or(false),
                        assigned_to: item.get("assigned_to").and_then(|v| v.as_s().ok()).map(|s| s.to_string()),
                        created_at: item.get("created_at").and_then(|v| v.as_s().ok()).map(|s| s.to_string()).unwrap_or_default(),
                    };
                    blocks.push(block);
                }
            }
    }
    
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .header("Access-Control-Allow-Origin", "*")
        .body(serde_json::to_string(&blocks)?.into())
        .map_err(Box::new)?)
}

/// Update a block
pub async fn update_block(
    client: &DynamoClient,
    table_name: &str,
    block_id: &str,
    body: &[u8],
) -> Result<Response<Body>, Error> {
    let req: UpdateBlockRequest = serde_json::from_slice(body)?;
    let pk = format!("BLOCK#{}", block_id);
    
    let mut update_expr = vec![];
    let mut expr_names = std::collections::HashMap::new();
    let mut expr_values = std::collections::HashMap::new();
    
    if let Some(name) = req.name {
        update_expr.push("#name = :name");
        expr_names.insert("#name".to_string(), "name".to_string());
        expr_values.insert(":name".to_string(), aws_sdk_dynamodb::types::AttributeValue::S(name));
    }
    
    if let Some(state) = req.state {
        update_expr.push("#state = :state");
        expr_names.insert("#state".to_string(), "state".to_string());
        expr_values.insert(":state".to_string(), aws_sdk_dynamodb::types::AttributeValue::S(state));
    }
    
    if let Some(locked) = req.locked {
        update_expr.push("#locked = :locked");
        expr_names.insert("#locked".to_string(), "locked".to_string());
        expr_values.insert(":locked".to_string(), aws_sdk_dynamodb::types::AttributeValue::Bool(locked));
    }
    
    if let Some(assigned_to) = req.assigned_to {
        update_expr.push("#assigned_to = :assigned_to");
        expr_names.insert("#assigned_to".to_string(), "assigned_to".to_string());
        expr_values.insert(":assigned_to".to_string(), aws_sdk_dynamodb::types::AttributeValue::S(assigned_to));
    }
    
    if !update_expr.is_empty() {
        let mut builder = client
            .update_item()
            .table_name(table_name)
            .key("PK", aws_sdk_dynamodb::types::AttributeValue::S(pk.clone()))
            .key("SK", aws_sdk_dynamodb::types::AttributeValue::S(pk))
            .update_expression(format!("SET {}", update_expr.join(", ")));
        
        for (k, v) in expr_names {
            builder = builder.expression_attribute_names(k, v);
        }
        
        for (k, v) in expr_values {
            builder = builder.expression_attribute_values(k, v);
        }
        
        builder.send().await?;
    }
    
    get_block(client, table_name, block_id).await
}

/// Delete a block
pub async fn delete_block(
    client: &DynamoClient,
    table_name: &str,
    block_id: &str,
) -> Result<Response<Body>, Error> {
    let pk = format!("BLOCK#{}", block_id);
    
    client
        .delete_item()
        .table_name(table_name)
        .key("PK", aws_sdk_dynamodb::types::AttributeValue::S(pk.clone()))
        .key("SK", aws_sdk_dynamodb::types::AttributeValue::S(pk))
        .send()
        .await?;
    
    Ok(Response::builder()
        .status(StatusCode::NO_CONTENT)
        .header("Access-Control-Allow-Origin", "*")
        .body(Body::Empty)
        .map_err(Box::new)?)
}
