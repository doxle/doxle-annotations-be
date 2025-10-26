use lambda_http::{Body, Error, Response, http::StatusCode};
use aws_sdk_dynamodb::Client as DynamoClient;
use crate::types::{Class, CreateClassRequest, UpdateClassRequest};

/// Create a new class for a project
pub async fn create_class(
    client: &DynamoClient,
    table_name: &str,
    project_id: &str,
    body: &[u8],
) -> Result<Response<Body>, Error> {
    let req: CreateClassRequest = serde_json::from_slice(body)?;
    
    let class_id = uuid::Uuid::new_v4().to_string();
    let pk = format!("PROJECT#{}", project_id);
    let sk = format!("CLASS#{}", class_id);
    
    // Store class
    let mut builder = client
        .put_item()
        .table_name(table_name)
        .item("PK", aws_sdk_dynamodb::types::AttributeValue::S(pk))
        .item("SK", aws_sdk_dynamodb::types::AttributeValue::S(sk))
        .item("name", aws_sdk_dynamodb::types::AttributeValue::S(req.name.clone()))
        .item("count", aws_sdk_dynamodb::types::AttributeValue::N("0".to_string()));
    
    if let Some(color) = &req.color {
        builder = builder.item("color", aws_sdk_dynamodb::types::AttributeValue::S(color.clone()));
    }
    
    if let Some(properties) = &req.properties {
        builder = builder.item("properties", aws_sdk_dynamodb::types::AttributeValue::S(serde_json::to_string(properties)?));
    }
    
    builder.send().await?;
    
    let class = Class {
        class_id: class_id.clone(),
        project_id: project_id.to_string(),
        name: req.name,
        color: req.color,
        properties: req.properties,
        count: 0,
    };
    
    Ok(Response::builder()
        .status(StatusCode::CREATED)
        .header("Content-Type", "application/json")
        .header("Access-Control-Allow-Origin", "*")
        .body(serde_json::to_string(&class)?.into())
        .map_err(Box::new)?)
}

/// Get a specific class
pub async fn get_class(
    client: &DynamoClient,
    table_name: &str,
    project_id: &str,
    class_id: &str,
) -> Result<Response<Body>, Error> {
    let pk = format!("PROJECT#{}", project_id);
    let sk = format!("CLASS#{}", class_id);
    
    let result = client
        .get_item()
        .table_name(table_name)
        .key("PK", aws_sdk_dynamodb::types::AttributeValue::S(pk))
        .key("SK", aws_sdk_dynamodb::types::AttributeValue::S(sk))
        .send()
        .await?;
    
    if let Some(item) = result.item() {
        let class = Class {
            class_id: class_id.to_string(),
            project_id: project_id.to_string(),
            name: item.get("name").and_then(|v| v.as_s().ok()).map(|s| s.to_string()).unwrap_or_default(),
            color: item.get("color").and_then(|v| v.as_s().ok()).map(|s| s.to_string()),
            properties: item.get("properties")
                .and_then(|v| v.as_s().ok())
                .and_then(|s| serde_json::from_str(s).ok()),
            count: item.get("count").and_then(|v| v.as_n().ok()).and_then(|n| n.parse().ok()).unwrap_or(0),
        };
        
        Ok(Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "application/json")
            .header("Access-Control-Allow-Origin", "*")
            .body(serde_json::to_string(&class)?.into())
            .map_err(Box::new)?)
    } else {
        Ok(Response::builder()
            .status(StatusCode::NOT_FOUND)
            .header("Content-Type", "application/json")
            .header("Access-Control-Allow-Origin", "*")
            .body(serde_json::json!({"error": "Class not found"}).to_string().into())
            .map_err(Box::new)?)
    }
}

/// List all classes for a project
pub async fn list_project_classes(
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
        .expression_attribute_values(":sk_prefix", aws_sdk_dynamodb::types::AttributeValue::S("CLASS#".to_string()))
        .send()
        .await?;
    
    let mut classes = Vec::new();
    
    for item in result.items() {
            if let Some(sk) = item.get("SK").and_then(|v| v.as_s().ok()) {
                if let Some(class_id) = sk.strip_prefix("CLASS#") {
                    let class = Class {
                        class_id: class_id.to_string(),
                        project_id: project_id.to_string(),
                        name: item.get("name").and_then(|v| v.as_s().ok()).map(|s| s.to_string()).unwrap_or_default(),
                        color: item.get("color").and_then(|v| v.as_s().ok()).map(|s| s.to_string()),
                        properties: item.get("properties")
                            .and_then(|v| v.as_s().ok())
                            .and_then(|s| serde_json::from_str(s).ok()),
                        count: item.get("count").and_then(|v| v.as_n().ok()).and_then(|n| n.parse().ok()).unwrap_or(0),
                    };
                    classes.push(class);
                }
            }
    }
    
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .header("Access-Control-Allow-Origin", "*")
        .body(serde_json::to_string(&classes)?.into())
        .map_err(Box::new)?)
}

/// Update a class
pub async fn update_class(
    client: &DynamoClient,
    table_name: &str,
    project_id: &str,
    class_id: &str,
    body: &[u8],
) -> Result<Response<Body>, Error> {
    let req: UpdateClassRequest = serde_json::from_slice(body)?;
    let pk = format!("PROJECT#{}", project_id);
    let sk = format!("CLASS#{}", class_id);
    
    let mut update_expr = vec![];
    let mut expr_names = std::collections::HashMap::new();
    let mut expr_values = std::collections::HashMap::new();
    
    if let Some(name) = req.name {
        update_expr.push("#name = :name");
        expr_names.insert("#name".to_string(), "name".to_string());
        expr_values.insert(":name".to_string(), aws_sdk_dynamodb::types::AttributeValue::S(name));
    }
    
    if let Some(color) = req.color {
        update_expr.push("#color = :color");
        expr_names.insert("#color".to_string(), "color".to_string());
        expr_values.insert(":color".to_string(), aws_sdk_dynamodb::types::AttributeValue::S(color));
    }
    
    if let Some(properties) = req.properties {
        update_expr.push("#properties = :properties");
        expr_names.insert("#properties".to_string(), "properties".to_string());
        expr_values.insert(":properties".to_string(), 
            aws_sdk_dynamodb::types::AttributeValue::S(serde_json::to_string(&properties)?));
    }
    
    if !update_expr.is_empty() {
        let mut builder = client
            .update_item()
            .table_name(table_name)
            .key("PK", aws_sdk_dynamodb::types::AttributeValue::S(pk.clone()))
            .key("SK", aws_sdk_dynamodb::types::AttributeValue::S(sk.clone()))
            .update_expression(format!("SET {}", update_expr.join(", ")));
        
        for (k, v) in expr_names {
            builder = builder.expression_attribute_names(k, v);
        }
        
        for (k, v) in expr_values {
            builder = builder.expression_attribute_values(k, v);
        }
        
        builder.send().await?;
    }
    
    get_class(client, table_name, project_id, class_id).await
}

/// Delete a class
pub async fn delete_class(
    client: &DynamoClient,
    table_name: &str,
    project_id: &str,
    class_id: &str,
) -> Result<Response<Body>, Error> {
    let pk = format!("PROJECT#{}", project_id);
    let sk = format!("CLASS#{}", class_id);
    
    client
        .delete_item()
        .table_name(table_name)
        .key("PK", aws_sdk_dynamodb::types::AttributeValue::S(pk))
        .key("SK", aws_sdk_dynamodb::types::AttributeValue::S(sk))
        .send()
        .await?;
    
    Ok(Response::builder()
        .status(StatusCode::NO_CONTENT)
        .header("Access-Control-Allow-Origin", "*")
        .body(Body::Empty)
        .map_err(Box::new)?)
}

/// Increment class count (when annotations are added/removed)
pub async fn increment_class_count(
    client: &DynamoClient,
    table_name: &str,
    project_id: &str,
    class_id: &str,
    delta: i32,
) -> Result<(), Error> {
    let pk = format!("PROJECT#{}", project_id);
    let sk = format!("CLASS#{}", class_id);
    
    client
        .update_item()
        .table_name(table_name)
        .key("PK", aws_sdk_dynamodb::types::AttributeValue::S(pk))
        .key("SK", aws_sdk_dynamodb::types::AttributeValue::S(sk))
        .update_expression("SET #count = if_not_exists(#count, :zero) + :delta")
        .expression_attribute_names("#count", "count")
        .expression_attribute_values(":zero", aws_sdk_dynamodb::types::AttributeValue::N("0".to_string()))
        .expression_attribute_values(":delta", aws_sdk_dynamodb::types::AttributeValue::N(delta.to_string()))
        .send()
        .await?;
    
    Ok(())
}
