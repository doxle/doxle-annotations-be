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
    client
        .put_item()
        .table_name(table_name)
        .item("PK", aws_sdk_dynamodb::types::AttributeValue::S(pk.clone()))
        .item("SK", aws_sdk_dynamodb::types::AttributeValue::S(pk.clone()))
        .item("email", aws_sdk_dynamodb::types::AttributeValue::S(req.email.clone()))
        .item("role", aws_sdk_dynamodb::types::AttributeValue::S(req.role.clone())) // admin | annotator | builder
        .item("created_at", aws_sdk_dynamodb::types::AttributeValue::S(now.clone()))
        .send()
        .await?;

    let user = User {
        user_id: user_id.to_string(),
        email: req.email,
        role: req.role,
        created_at: now,
    };

    let resp = Response::builder()
        .status(201)
        .header("content-type", "application/json")
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
        let email = item.get("email").and_then(|v| v.as_s().ok()).map(|s| s.to_string()).unwrap_or_default();
        let role = item.get("role").and_then(|v| v.as_s().ok()).map(|s| s.to_string()).unwrap_or_default(); // admin | annotator | builder
        let created_at = item.get("created_at").and_then(|v| v.as_s().ok()).map(|s| s.to_string()).unwrap_or_default();

        let user = User {
            user_id: user_id.to_string(),
            email,
            role,
            created_at,
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

/// Update user (only role can be updated, email is immutable)
pub async fn update_user(
    client: &DynamoClient,
    table_name: &str,
    user_id: &str,
    body: &[u8],
) -> Result<Response<Body>, Error> {
    let req: UpdateUserRequest = serde_json::from_slice(body)?;

    let pk = format!("USER#{}", user_id);

    // Update role if provided (admin | annotator | builder)
    if let Some(role) = req.role {
        client
            .update_item()
            .table_name(table_name)
            .key("PK", aws_sdk_dynamodb::types::AttributeValue::S(pk.clone()))
            .key("SK", aws_sdk_dynamodb::types::AttributeValue::S(pk.clone()))
            .update_expression("SET #role = :role")
            .expression_attribute_names("#role", "role")
            .expression_attribute_values(":role", aws_sdk_dynamodb::types::AttributeValue::S(role))
            .send()
            .await?;
    }

    // Return updated user
    get_user(client, table_name, user_id).await
}
