use lambda_http::{Body, Error, Request, Response, http::{Method, StatusCode}};
use aws_sdk_cognitoidentityprovider::Client as CognitoClient;
use std::env;
use crate::auth;

/// Main Lambda handler - routes requests to appropriate functions
pub(crate) async fn function_handler(event: Request) -> Result<Response<Body>, Error> {
    tracing::info!("Auth Lambda invoked");
    
    // Handle CORS preflight
    if event.method() == "OPTIONS" {
        return Ok(Response::builder()
            .status(StatusCode::OK)
            .header("Access-Control-Allow-Origin", "*")
            .header("Access-Control-Allow-Methods", "POST, OPTIONS")
            .header("Access-Control-Allow-Headers", "Content-Type")
            .body(Body::Empty)
            .map_err(Box::new)?);
    }

    // Initialize Cognito client
    let config = aws_config::load_from_env().await;
    let cognito_client = CognitoClient::new(&config);
    
    // Get Cognito configuration from environment
    let client_id = env::var("COGNITO_CLIENT_ID")
        .expect("COGNITO_CLIENT_ID must be set");
    let client_secret = env::var("COGNITO_CLIENT_SECRET")
        .expect("COGNITO_CLIENT_SECRET must be set");

    let method = event.method();
    let path = event.uri().path();
    let body = event.body();

    // Route requests to appropriate handlers
    match (method, path) {
        (&Method::POST, "/auth/login") => {
            auth::login(&cognito_client, &client_id, &client_secret, body).await
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use lambda_http::{Request, RequestExt};

    #[tokio::test]
    async fn test_generic_http_handler() {
        let request = Request::default();

        let response = function_handler(request).await.unwrap();
        assert_eq!(response.status(), 200);

        let body_bytes = response.body().to_vec();
        let body_string = String::from_utf8(body_bytes).unwrap();

        assert_eq!(
            body_string,
            "Hello world, this is an AWS Lambda HTTP request"
        );
    }

    #[tokio::test]
    async fn test_http_handler_with_query_string() {
        let mut query_string_parameters: HashMap<String, String> = HashMap::new();
        query_string_parameters.insert("name".into(), "doxle-auth".into());

        let request = Request::default()
            .with_query_string_parameters(query_string_parameters);

        let response = function_handler(request).await.unwrap();
        assert_eq!(response.status(), 200);

        let body_bytes = response.body().to_vec();
        let body_string = String::from_utf8(body_bytes).unwrap();

        assert_eq!(
            body_string,
            "Hello doxle-auth, this is an AWS Lambda HTTP request"
        );
    }
}
