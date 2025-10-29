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

#[derive(Deserialize)]
pub struct SignupRequest {
    pub email: String,
    pub password: String,
    pub invite_code: String,
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
            let error_message = format!("{:?}", e);
            tracing::error!("Cognito authentication error: {}", error_message);
            
            // Extract user-friendly error message
            let user_message = if error_message.contains("NotAuthorizedException") {
                "Incorrect email or password".to_string()
            } else if error_message.contains("UserNotConfirmedException") {
                "Please verify your email before logging in".to_string()
            } else if error_message.contains("UserNotFoundException") {
                "No account found with this email".to_string()
            } else if error_message.contains("PasswordResetRequiredException") {
                "Password reset required".to_string()
            } else if error_message.contains("TooManyRequestsException") {
                "Too many login attempts. Please try again later".to_string()
            } else {
                "Login failed. Please check your credentials".to_string()
            };
            
            let error = ErrorResponse {
                error: "AuthenticationFailed".to_string(),
                message: user_message,
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

/// Handle user signup with Cognito
pub async fn signup(
    cognito_client: &CognitoClient,
    dynamo_client: &aws_sdk_dynamodb::Client,
    table_name: &str,
    client_id: &str,
    client_secret: &str,
    body: &Body,
) -> Result<Response<Body>, Error> {
    let body_str = match body {
        Body::Text(text) => text,
        Body::Binary(bytes) => std::str::from_utf8(bytes).unwrap_or(""),
        Body::Empty => "",
    };

    tracing::info!("Signup request received");

    let signup_request: SignupRequest = match serde_json::from_str(body_str) {
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

    tracing::info!("Signing up user: {}", signup_request.email);

    // Validate invite code
    if let Err(e) = crate::invites::validate_invite(
        dynamo_client,
        table_name,
        &signup_request.invite_code,
        &signup_request.email,
    )
    .await
    {
        let error = ErrorResponse {
            error: "InvalidInvite".to_string(),
            message: e,
        };
        return Ok(Response::builder()
            .status(StatusCode::FORBIDDEN)
            .header("Content-Type", "application/json")
            .header("Access-Control-Allow-Origin", "*")
            .body(serde_json::to_string(&error)?.into())
            .map_err(Box::new)?);
    }

    let secret_hash = compute_secret_hash(
        &signup_request.email,
        client_id,
        client_secret,
    );

    let signup_result = cognito_client
        .sign_up()
        .client_id(client_id)
        .username(&signup_request.email)
        .password(&signup_request.password)
        .secret_hash(&secret_hash)
        .user_attributes(
            aws_sdk_cognitoidentityprovider::types::AttributeType::builder()
                .name("email")
                .value(&signup_request.email)
                .build()?
        )
        .send()
        .await;

    match signup_result {
        Ok(_response) => {
            tracing::info!("Signup successful for user: {}", signup_request.email);
            
            // Auto-confirm user since they used a valid invite (email already verified)
            if let Ok(user_pool_id) = std::env::var("COGNITO_USER_POOL_ID") {
                if let Err(e) = cognito_client
                    .admin_confirm_sign_up()
                    .user_pool_id(&user_pool_id)
                    .username(&signup_request.email)
                    .send()
                    .await
                {
                    tracing::error!("Failed to auto-confirm user: {:?}", e);
                    // Don't fail signup, user can still verify via email
                } else {
                    tracing::info!("User auto-confirmed: {}", signup_request.email);
                }
            } else {
                tracing::warn!("COGNITO_USER_POOL_ID not set; skipping auto-confirm");
            }
            
            // Mark invite as used
            if let Err(e) = crate::invites::mark_invite_used(
                dynamo_client,
                table_name,
                &signup_request.invite_code,
            )
            .await
            {
                tracing::error!("Failed to mark invite as used: {}", e);
                // Don't fail the signup if we can't mark invite as used
            }
            
            Ok(Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "application/json")
                .header("Access-Control-Allow-Origin", "*")
                .body(serde_json::json!({"message": "Signup successful"}).to_string().into())
                .map_err(Box::new)?)
        }
        Err(e) => {
            let error_message = format!("{:?}", e);
            tracing::error!("Cognito signup error: {}", error_message);
            
            // Extract user-friendly error message (only send this to frontend)
            let user_message = if error_message.contains("InvalidPasswordException") {
                "Password must contain at least 8 characters with uppercase, lowercase, number, and special character".to_string()
            } else if error_message.contains("UsernameExistsException") {
                "An account with this email already exists".to_string()
            } else if error_message.contains("InvalidParameterException") {
                "Invalid email or password format".to_string()
            } else {
                "Signup failed. Please check your credentials and try again.".to_string()
            };
            
            let error = ErrorResponse {
                error: "SignupFailed".to_string(),
                message: user_message,
            };
            Ok(Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .header("Content-Type", "application/json")
                .header("Access-Control-Allow-Origin", "*")
                .body(serde_json::to_string(&error)?.into())
                .map_err(Box::new)?)
        }
    }
}
