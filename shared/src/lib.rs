pub mod types;
pub mod auth;
pub mod users;
pub mod projects;
pub mod blocks;
pub mod images;
pub mod annotations;
pub mod classes;
pub mod sockets;
pub mod s3;
pub mod s3_multipart;
pub mod invites;
pub mod email;
pub mod cloudfront;

use aws_sdk_apigatewaymanagement::Client as ApiGatewayManagementClient;
use aws_sdk_cognitoidentityprovider::Client as CognitoClient;
use aws_sdk_dynamodb::Client as DynamoClient;
use aws_sdk_s3::Client as S3Client;
use aws_sdk_sesv2::Client as SesClient;
use std::sync::Arc;

/// Shared application state
pub struct AppState {
    pub cognito_client: CognitoClient,
    pub dynamo_client: DynamoClient,
    pub s3_client: S3Client,
    pub ses_client: SesClient,
    pub api_gateway_client: Option<ApiGatewayManagementClient>,
}

impl AppState {
    pub fn new(
        cognito_client: CognitoClient,
        dynamo_client: DynamoClient,
        s3_client: S3Client,
        ses_client: SesClient,
        api_gateway_client: Option<ApiGatewayManagementClient>,
    ) -> Arc<Self> {
        Arc::new(Self {
            cognito_client,
            dynamo_client,
            s3_client,
            ses_client,
            api_gateway_client,
        })
    }
}
