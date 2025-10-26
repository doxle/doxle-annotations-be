use lambda_http::{Body, Error, Response};
use aws_sdk_dynamodb::Client as DynamoClient;
use crate::types::{User, CreateUserRequest, UpdateUserRequest};

/// Create user in DynamoDB after Cognito signup
/// This is called once after user signs up in Cognito
pub async fn create_user(
    client: &DynamoClient,
    table_name: &str,
    user_id: &str,
    body: &[u8],
) -> Result<Response<Body>, Error> {
    let req: CreateUserRequest = serde_json::from_slice(body)?;

    let now = chrono::Utc::now().to_rfc3339();
    let pk = format!("USER#{}", user_id);

    // Store user in DynamoDB with PK=USER#cognito-id, SK=USER#cognito-id
    let mut put_request = client
        .put_item()
        .table_name(table_name)
        .item("PK", aws_sdk_dynamodb::types::AttributeValue::S(pk.clone()))
        .item("SK", aws_sdk_dynamodb::types::AttributeValue::S(pk.clone()))
        .item("name", aws_sdk_dynamodb::types::AttributeValue::S(req.name.clone()))
        .item("email", aws_sdk_dynamodb::types::AttributeValue::S(req.email.clone()))
        .item("role", aws_sdk_dynamodb::types::AttributeValue::S(req.role.clone()))
        .item("created_at", aws_sdk_dynamodb::types::AttributeValue::S(now.clone()));
    
    if let Some(company) = &req.company {
        put_request = put_request.item("company", aws_sdk_dynamodb::types::AttributeValue::S(company.clone()));
    }
    
    put_request.send().await?;

    let user = User {
        user_id: user_id.to_string(),
        name: req.name,
        email: req.email,
        company: req.company,
        role: req.role,
        created_at: now,
        last_login: None,
    };

    let resp = Response::builder()
        .status(201)
        .header("content-type", "application/json")
        .header("Access-Control-Allow-Origin", "*")
        .body(serde_json::to_string(&user)?.into())
        .map_err(Box::new)?;
    Ok(resp)
}

/// Get current user from DynamoDB
pub async fn get_user(
    client: &DynamoClient,
    table_name: &str,
    user_id: &str,
) -> Result<Response<Body>, Error> {
    let pk = format!("USER#{}", user_id);

    let result = client
        .get_item()
        .table_name(table_name)
        .key("PK", aws_sdk_dynamodb::types::AttributeValue::S(pk.clone()))
        .key("SK", aws_sdk_dynamodb::types::AttributeValue::S(pk.clone()))
        .send()
        .await?;

    if let Some(item) = result.item() {
        let name = item.get("name").and_then(|v| v.as_s().ok()).map(|s| s.to_string()).unwrap_or_default();
        let email = item.get("email").and_then(|v| v.as_s().ok()).map(|s| s.to_string()).unwrap_or_default();
        let company = item.get("company").and_then(|v| v.as_s().ok()).map(|s| s.to_string());
        let role = item.get("role").and_then(|v| v.as_s().ok()).map(|s| s.to_string()).unwrap_or_default();
        let created_at = item.get("created_at").and_then(|v| v.as_s().ok()).map(|s| s.to_string()).unwrap_or_default();
        let _last_login = item.get("last_login").and_then(|v| v.as_s().ok()).map(|s| s.to_string());
        
        // Update last_login on every get
        let now = chrono::Utc::now().to_rfc3339();
        let _ = client
            .update_item()
            .table_name(table_name)
            .key("PK", aws_sdk_dynamodb::types::AttributeValue::S(pk.clone()))
            .key("SK", aws_sdk_dynamodb::types::AttributeValue::S(pk.clone()))
            .update_expression("SET last_login = :login")
            .expression_attribute_values(":login", aws_sdk_dynamodb::types::AttributeValue::S(now.clone()))
            .send()
            .await;

        let user = User {
            user_id: user_id.to_string(),
            name,
            email,
            company,
            role,
            created_at,
            last_login: Some(now),
        };

        let resp = Response::builder()
            .status(200)
            .header("content-type", "application/json")
            .body(serde_json::to_string(&user)?.into())
            .map_err(Box::new)?;
        Ok(resp)
    } else {
        let resp = Response::builder()
            .status(404)
            .header("content-type", "application/json")
            .body(serde_json::json!({"error": "User not found"}).to_string().into())
            .map_err(Box::new)?;
        Ok(resp)
    }
}

/// Update user
pub async fn update_user(
    client: &DynamoClient,
    table_name: &str,
    user_id: &str,
    body: &[u8],
) -> Result<Response<Body>, Error> {
    let req: UpdateUserRequest = serde_json::from_slice(body)?;
    let pk = format!("USER#{}", user_id);

    let mut update_expr = vec![];
    let mut expr_names = std::collections::HashMap::new();
    let mut expr_values = std::collections::HashMap::new();
    
    if let Some(name) = req.name {
        update_expr.push("#name = :name");
        expr_names.insert("#name".to_string(), "name".to_string());
        expr_values.insert(":name".to_string(), aws_sdk_dynamodb::types::AttributeValue::S(name));
    }
    
    if let Some(company) = req.company {
        update_expr.push("company = :company");
        expr_values.insert(":company".to_string(), aws_sdk_dynamodb::types::AttributeValue::S(company));
    }
    
    if let Some(role) = req.role {
        update_expr.push("#role = :role");
        expr_names.insert("#role".to_string(), "role".to_string());
        expr_values.insert(":role".to_string(), aws_sdk_dynamodb::types::AttributeValue::S(role));
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

    // Return updated user
    get_user(client, table_name, user_id).await
}
