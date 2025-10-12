use lambda_http::{Body, Error, Request, RequestExt, Response, http::{Method, StatusCode}};
use aws_sdk_cognitoidentityprovider::Client as CognitoClient;
use aws_sdk_dynamodb::Client as DynamoClient;
use std::env;
use crate::{auth, users};

/// Main Lambda handler - routes requests to auth or user endpoints
pub(crate) async fn function_handler(event: Request) -> Result<Response<Body>, Error> {
    tracing::info!("API Lambda invoked");
    
    // Handle CORS preflight
    if event.method() == "OPTIONS" {
        return Ok(Response::builder()
            .status(StatusCode::OK)
            .header("Access-Control-Allow-Origin", "*")
            .header("Access-Control-Allow-Methods", "GET,POST,PUT,PATCH,DELETE,OPTIONS")
            .header("Access-Control-Allow-Headers", "Content-Type,Authorization")
            .body(Body::Empty)
            .map_err(Box::new)?);
    }

    let method = event.method();
    let path = event.uri().path();
    let body = event.body();

    // Route to auth endpoints (no JWT validation)
    if path.starts_with("/login") {
        // Initialize Cognito client
        let config = aws_config::load_from_env().await;
        let cognito_client = CognitoClient::new(&config);
        
        let client_id = env::var("COGNITO_CLIENT_ID")
            .expect("COGNITO_CLIENT_ID must be set");
        let client_secret = env::var("COGNITO_CLIENT_SECRET")
            .expect("COGNITO_CLIENT_SECRET must be set");

        return match method {
            &Method::POST => {
                auth::login(&cognito_client, &client_id, &client_secret, body).await
            }
            _ => {
                let resp = Response::builder()
                    .status(StatusCode::METHOD_NOT_ALLOWED)
                    .header("Content-Type", "application/json")
                    .header("Access-Control-Allow-Origin", "*")
                    .body(serde_json::json!({"error": "Method not allowed"}).to_string().into())
                    .map_err(Box::new)?;
                Ok(resp)
            }
        };
    }

    // Route to user endpoints (JWT validated by API Gateway)
    if path.starts_with("/users") {
        // Initialize DynamoDB client
        let config = aws_config::load_from_env().await;
        let dynamo_client = DynamoClient::new(&config);
        let table_name = env::var("TABLE_NAME").unwrap_or_else(|_| "doxle".to_string());

        // Get user ID from JWT (API Gateway authorizer passes this)
        let user_id = event
            .query_string_parameters_ref()
            .and_then(|params| params.first("user_id"))
            .unwrap_or("test-user-123");

        return match (method, path) {
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
                    .status(StatusCode::NOT_FOUND)
                    .header("Content-Type", "application/json")
                    .header("Access-Control-Allow-Origin", "*")
                    .body(serde_json::json!({"error": "Not found"}).to_string().into())
                    .map_err(Box::new)?;
                Ok(resp)
            }
        };
    }

    // No matching route
    let resp = Response::builder()
        .status(StatusCode::NOT_FOUND)
        .header("Content-Type", "application/json")
        .header("Access-Control-Allow-Origin", "*")
        .body(serde_json::json!({"error": "Not found"}).to_string().into())
        .map_err(Box::new)?;
    Ok(resp)
}
