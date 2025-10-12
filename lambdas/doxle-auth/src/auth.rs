use lambda_http::{Body, Error, Response, http::StatusCode};
use aws_sdk_cognitoidentityprovider::Client as CognitoClient;
use serde::{Deserialize, Serialize};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use base64::{Engine as _, engine::general_purpose};

#[derive(Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct LoginResponse {
    pub id_token: String,
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: i32,
}

#[derive(Serialize)]
pub struct ErrorResponse {
    pub error: String,
    pub message: String,
}

type HmacSha256 = Hmac<Sha256>;

/// Compute the SECRET_HASH for Cognito authentication
fn compute_secret_hash(username: &str, client_id: &str, client_secret: &str) -> String {
    let message = format!("{}{}", username, client_id);
    let mut mac = HmacSha256::new_from_slice(client_secret.as_bytes())
        .expect("HMAC can take key of any size");
    mac.update(message.as_bytes());
    let result = mac.finalize();
    general_purpose::STANDARD.encode(result.into_bytes())
}

/// Handle user login with Cognito
pub async fn login(
    cognito_client: &CognitoClient,
    client_id: &str,
    client_secret: &str,
    body: &Body,
) -> Result<Response<Body>, Error> {
    // Parse request body
    let body_str = match body {
        Body::Text(text) => text,
        Body::Binary(bytes) => std::str::from_utf8(bytes).unwrap_or(""),
        Body::Empty => "",
    };

    tracing::info!("Login request received");

    let login_request: LoginRequest = match serde_json::from_str(body_str) {
        Ok(req) => req,
        Err(e) => {
            tracing::error!("Failed to parse request body: {}", e);
            let error = ErrorResponse {
                error: "InvalidRequest".to_string(),
                message: format!("Invalid request body: {}", e),
            };
            return Ok(Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .header("Content-Type", "application/json")
                .header("Access-Control-Allow-Origin", "*")
                .body(serde_json::to_string(&error)?.into())
                .map_err(Box::new)?);
        }
    };

    tracing::info!("Authenticating user: {}", login_request.email);

    // Compute SECRET_HASH
    let secret_hash = compute_secret_hash(
        &login_request.email,
        client_id,
        client_secret,
    );

    // Authenticate with Cognito
    let auth_result = cognito_client
        .initiate_auth()
        .auth_flow(aws_sdk_cognitoidentityprovider::types::AuthFlowType::UserPasswordAuth)
        .client_id(client_id)
        .auth_parameters("USERNAME", &login_request.email)
        .auth_parameters("PASSWORD", &login_request.password)
        .auth_parameters("SECRET_HASH", &secret_hash)
        .send()
        .await;

    match auth_result {
        Ok(response) => {
            if let Some(auth_result) = response.authentication_result() {
                tracing::info!("Authentication successful for user: {}", login_request.email);
                
                let login_response = LoginResponse {
                    id_token: auth_result.id_token().unwrap_or_default().to_string(),
                    access_token: auth_result.access_token().unwrap_or_default().to_string(),
                    refresh_token: auth_result.refresh_token().unwrap_or_default().to_string(),
                    expires_in: auth_result.expires_in(),
                };

                Ok(Response::builder()
                    .status(StatusCode::OK)
                    .header("Content-Type", "application/json")
                    .header("Access-Control-Allow-Origin", "*")
                    .body(serde_json::to_string(&login_response)?.into())
                    .map_err(Box::new)?)
            } else {
                tracing::error!("No authentication result returned");
                let error = ErrorResponse {
                    error: "AuthenticationFailed".to_string(),
                    message: "No authentication result returned".to_string(),
                };
                Ok(Response::builder()
                    .status(StatusCode::UNAUTHORIZED)
                    .header("Content-Type", "application/json")
                    .header("Access-Control-Allow-Origin", "*")
                    .body(serde_json::to_string(&error)?.into())
                    .map_err(Box::new)?)
            }
        }
        Err(e) => {
            tracing::error!("Cognito authentication error: {:?}", e);
            let error = ErrorResponse {
                error: "AuthenticationFailed".to_string(),
                message: format!("Authentication failed: {}", e),
            };
            Ok(Response::builder()
                .status(StatusCode::UNAUTHORIZED)
                .header("Content-Type", "application/json")
                .header("Access-Control-Allow-Origin", "*")
                .body(serde_json::to_string(&error)?.into())
                .map_err(Box::new)?)
        }
    }
}
