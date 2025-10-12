use lambda_http::{Body, Error, Request, RequestExt, Response, http::Method};
use aws_sdk_dynamodb::Client as DynamoClient;
use std::env;
use crate::users;

/// Main Lambda handler - routes requests to appropriate functions
pub(crate) async fn function_handler(event: Request) -> Result<Response<Body>, Error> {
    // Initialize DynamoDB client
    let config = aws_config::load_from_env().await;
    let dynamo_client = DynamoClient::new(&config);
    let table_name = env::var("TABLE_NAME").unwrap_or_else(|_| "doxle".to_string());

    // Get user ID from Cognito authorizer (when deployed with API Gateway)
    // For local testing, use query parameter "user_id"
    let user_id = event
        .query_string_parameters_ref()
        .and_then(|params| params.first("user_id"))
        .unwrap_or("test-user-123");

    let method = event.method();
    let path = event.uri().path();
    let body = event.body();

    // Route requests to appropriate handlers
    match (method, path) {
        (&Method::POST, "/users") => {
            users::create_user(&dynamo_client, &table_name, user_id, body).await
        }
        (&Method::GET, "/users/me") => {
            users::get_user(&dynamo_client, &table_name, user_id).await
        }
        (&Method::PATCH, "/users/me") => {
            users::update_user(&dynamo_client, &table_name, user_id, body).await
        }
        _ => {
            let resp = Response::builder()
                .status(404)
                .header("content-type", "application/json")
                .body(serde_json::json!({"error": "Not found"}).to_string().into())
                .map_err(Box::new)?;
            Ok(resp)
        }
    }
}