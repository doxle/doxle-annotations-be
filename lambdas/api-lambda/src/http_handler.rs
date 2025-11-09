use aws_sdk_dynamodb::{types::AttributeValue, Client as DynamoClient};
use aws_sdk_s3::Client as S3Client;
use doxle_shared::{
    annotations, auth, blocks, classes, cloudfront, image_proxy, images, invites, projects,
    s3_multipart, users, AppState,
};
use lambda_http::{
    http::{Method, StatusCode},
    Body, Error, Request, RequestExt, Response,
};
use serde::Deserialize;
use std::env;
use std::sync::Arc;

#[derive(Deserialize)]
struct AbortUploadRequest {
    project_id: String,
    block_id: String,
    image_id: String,
    upload_id: String,
    extension: String,
}

/// Main Lambda handler - routes requests to auth or user endpoints
pub(crate) async fn function_handler(
    event: Request,
    state: Arc<AppState>,
) -> Result<Response<Body>, Error> {
    let method = event.method();
    let path = event.uri().path();
    let body = event.body();
    tracing::info!(
        "🚀 API Lambda v2.1.0 invoked - Method: {} Path: {}",
        method,
        path
    );

    // Handle CORS preflight
    if method == "OPTIONS" {
        return Ok(Response::builder()
            .status(StatusCode::OK)
            .header("Access-Control-Allow-Origin", "*")
            .header(
                "Access-Control-Allow-Methods",
                "GET,POST,PUT,PATCH,DELETE,OPTIONS",
            )
            .header(
                "Access-Control-Allow-Headers",
                "Content-Type,Authorization,X-User-Id",
            )
            .body(Body::Empty)
            .map_err(Box::new)?);
    }

    // Route to auth endpoints (no JWT validation)
    if path.starts_with("/login") {
        let client_id = env::var("COGNITO_CLIENT_ID").expect("COGNITO_CLIENT_ID must be set");
        let client_secret =
            env::var("COGNITO_CLIENT_SECRET").expect("COGNITO_CLIENT_SECRET must be set");

        return match method {
            &Method::POST => {
                auth::login(&state.cognito_client, &client_id, &client_secret, body).await
            }
            _ => {
                let resp = Response::builder()
                    .status(StatusCode::METHOD_NOT_ALLOWED)
                    .header("Content-Type", "application/json")
                    .header("Access-Control-Allow-Origin", "*")
                    .body(
                        serde_json::json!({"error": "Method not allowed"})
                            .to_string()
                            .into(),
                    )
                    .map_err(Box::new)?;
                Ok(resp)
            }
        };
    }

    if path.starts_with("/signup") {
        let client_id = env::var("COGNITO_CLIENT_ID").expect("COGNITO_CLIENT_ID must be set");
        let client_secret =
            env::var("COGNITO_CLIENT_SECRET").expect("COGNITO_CLIENT_SECRET must be set");
        let table_name = env::var("TABLE_NAME").unwrap_or_else(|_| "doxle-annotations".to_string());

        return match method {
            &Method::POST => {
                auth::signup(
                    &state.cognito_client,
                    &state.dynamo_client,
                    &table_name,
                    &client_id,
                    &client_secret,
                    body,
                )
                .await
            }
            _ => {
                let resp = Response::builder()
                    .status(StatusCode::METHOD_NOT_ALLOWED)
                    .header("Content-Type", "application/json")
                    .header("Access-Control-Allow-Origin", "*")
                    .body(
                        serde_json::json!({"error": "Method not allowed"})
                            .to_string()
                            .into(),
                    )
                    .map_err(Box::new)?;
                Ok(resp)
            }
        };
    }

    if path.starts_with("/refresh") {
        let client_id = env::var("COGNITO_CLIENT_ID").expect("COGNITO_CLIENT_ID must be set");
        let client_secret =
            env::var("COGNITO_CLIENT_SECRET").expect("COGNITO_CLIENT_SECRET must be set");

        return match method {
            &Method::POST => {
                auth::refresh_token(&state.cognito_client, &client_id, &client_secret, body).await
            }
            _ => {
                let resp = Response::builder()
                    .status(StatusCode::METHOD_NOT_ALLOWED)
                    .header("Content-Type", "application/json")
                    .header("Access-Control-Allow-Origin", "*")
                    .body(
                        serde_json::json!({"error": "Method not allowed"})
                            .to_string()
                            .into(),
                    )
                    .map_err(Box::new)?;
                Ok(resp)
            }
        };
    }

    // CloudFront signed cookies endpoint (requires JWT auth)
    if path == "/auth/cloudfront-cookies" {
        if method != &Method::POST {
            return Ok(Response::builder()
                .status(StatusCode::METHOD_NOT_ALLOWED)
                .header("Content-Type", "application/json")
                .header("Access-Control-Allow-Origin", "*")
                .body(
                    serde_json::json!({"error": "Method not allowed"})
                        .to_string()
                        .into(),
                )
                .map_err(Box::new)?);
        }

        // Validate Authorization header is present
        let auth_header = event.headers().get("Authorization");
        if auth_header.is_none() {
            return Ok(Response::builder()
                .status(StatusCode::UNAUTHORIZED)
                .header("Content-Type", "application/json")
                .header("Access-Control-Allow-Origin", "*")
                .body(
                    serde_json::json!({"error": "Missing Authorization header"})
                        .to_string()
                        .into(),
                )
                .map_err(Box::new)?);
        }

        // Extract user ID from JWT (API Gateway should have validated the token)
        let user_id = event
            .headers()
            .get("X-User-Id")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
            .or_else(|| {
                event
                    .request_context()
                    .authorizer()
                    .and_then(|auth| auth.jwt.as_ref())
                    .and_then(|jwt| jwt.claims.get("sub"))
                    .map(|s| s.to_string())
            })
            .unwrap_or_else(|| "authenticated-user".to_string()); // Fallback - cookies still work

        // Issue CloudFront signed cookies (valid for 12 hours)
        let origin_header = event.headers().get("Origin").and_then(|v| v.to_str().ok());
        return cloudfront::issue_signed_cookies_response(&user_id, 43200, origin_header);
    }

    // Image proxy route (public - serves images from S3)
    if path.starts_with("/proxy-image/") {
        // URL format: /proxy-image/projects/{pid}/blocks/{bid}/{image}.ext
        let image_path = path.strip_prefix("/proxy-image/").unwrap_or("");
        return image_proxy::proxy_image(&state.s3_client, "doxle-annotations", image_path).await;
    }

    // Invites routes (public GET, authenticated POST)
    if path.starts_with("/invites") {
        let table_name = env::var("TABLE_NAME").unwrap_or_else(|_| "doxle-annotations".to_string());
        let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

        return match (method, parts.as_slice()) {
            // GET /invites/{code} - public endpoint to view invite details
            (&Method::GET, ["invites", invite_code]) => {
                invites::get_invite(&state.dynamo_client, &table_name, invite_code).await
            }
            // POST /invites - create invite (requires auth)
            (&Method::POST, ["invites"]) => {
                // Get user ID from JWT for admin check
                let user_id = event
                    .headers()
                    .get("X-User-Id")
                    .and_then(|v| v.to_str().ok())
                    .map(|s| s.to_string())
                    .or_else(|| {
                        event
                            .request_context()
                            .authorizer()
                            .and_then(|auth| auth.jwt.as_ref())
                            .and_then(|jwt| jwt.claims.get("sub"))
                            .map(|s| s.to_string())
                    })
                    .unwrap_or_else(|| "anonymous".to_string());

                invites::create_invite(
                    &state.dynamo_client,
                    &state.ses_client,
                    &table_name,
                    &user_id,
                    body,
                )
                .await
            }
            _ => not_found(),
        };
    }

    // Route to user endpoints (JWT validated by API Gateway)
    if path.starts_with("/users") {
        let table_name = env::var("TABLE_NAME").unwrap_or_else(|_| "doxle-annotations".to_string());

        // Get user ID from JWT claims (HTTP API passes JWT claims in request context)
        // For HTTP APIs with JWT authorizer, claims are in requestContext.authorizer.jwt.claims
        // In local development, allow override with X-User-Id header
        let user_id = event
            .headers()
            .get("X-User-Id")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
            .or_else(|| {
                event
                    .request_context()
                    .authorizer()
                    .and_then(|auth| {
                        tracing::info!("Authorizer context: {:?}", auth);
                        auth.jwt.as_ref()
                    })
                    .and_then(|jwt| jwt.claims.get("sub"))
                    .map(|s| s.to_string())
            })
            .unwrap_or_else(|| {
                tracing::warn!("Could not extract user ID from JWT or header, using fallback");
                "test-user-123".to_string()
            });

        tracing::info!("User ID from JWT: {}", user_id);

        return match (method, path) {
            (&Method::POST, "/users") => {
                users::create_user(&state.dynamo_client, &table_name, &user_id, body).await
            }
            (&Method::GET, "/users/me") => {
                users::get_user(&state.dynamo_client, &table_name, &user_id).await
            }
            (&Method::PATCH, "/users/me") => {
                users::update_user(&state.dynamo_client, &table_name, &user_id, body).await
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

    // All other routes require auth
    let table_name = env::var("TABLE_NAME").unwrap_or_else(|_| "doxle-annotations".to_string());

    // Allow X-User-Id header override for local development
    let user_id = event
        .headers()
        .get("X-User-Id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .or_else(|| {
            event
                .request_context()
                .authorizer()
                .and_then(|auth| auth.jwt.as_ref())
                .and_then(|jwt| jwt.claims.get("sub"))
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| "test-user-123".to_string());

    // Projects routes
    if path.starts_with("/projects") {
        let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

        return match (method, parts.as_slice()) {
            // --- PROJECTS ---
            // POST /projects - create project
            (&Method::POST, ["projects"]) => {
                projects::create_project(&state.dynamo_client, &table_name, &user_id, body).await
            }
            // GET /projects - list user's projects
            (&Method::GET, ["projects"]) => {
                projects::list_user_projects(&state.dynamo_client, &table_name, &user_id).await
            }
            // GET /projects/{id} - get project
            (&Method::GET, ["projects", project_id]) => {
                projects::get_project(&state.dynamo_client, &table_name, project_id).await
            }
            // PATCH /projects/{id} - update project
            (&Method::PATCH, ["projects", project_id]) => {
                projects::update_project(&state.dynamo_client, &table_name, project_id, body).await
            }
            // DELETE /projects/{id} - delete project
            (&Method::DELETE, ["projects", project_id]) => {
                projects::delete_project(
                    &state.dynamo_client,
                    &state.s3_client,
                    &table_name,
                    project_id,
                    &user_id,
                )
                .await
            }

            // --- BLOCKS ---
            // GET /projects/{id}/blocks - list project blocks
            (&Method::GET, ["projects", project_id, "blocks"]) => {
                blocks::list_project_blocks(&state.dynamo_client, &table_name, project_id).await
            }
            // POST /projects/{id}/blocks - create block
            (&Method::POST, ["projects", project_id, "blocks"]) => {
                blocks::create_block(&state.dynamo_client, &table_name, project_id, body).await
            }
            //GET /projects/{pid}/blocks/{bid} - get specific block
            (&Method::GET, ["projects", project_id, "blocks", block_id]) => {
                blocks::get_block(&state.dynamo_client, &table_name, project_id, block_id).await
            }
            // PATCH /projects/{pid}/blocks/{bid} - update block
            (&Method::PATCH, ["projects", project_id, "blocks", block_id]) => {
                blocks::update_block(
                    &state.dynamo_client,
                    &table_name,
                    project_id,
                    block_id,
                    body,
                )
                .await
            }
            // DELETE /projects/{pid}/blocks/{bid} - delete block
            (&Method::DELETE, ["projects", project_id, "blocks", block_id]) => {
                blocks::delete_block(
                    &state.dynamo_client,
                    &state.s3_client,
                    &table_name,
                    project_id,
                    block_id,
                )
                .await
            }

            // --- IMAGES ---
            // GET /projects/{pid}/blocks/{bid}/images - list images for a block
            (&Method::GET, ["projects", _project_id, "blocks", block_id, "images"]) => {
                images::list_block_images(&state.dynamo_client, &table_name, block_id).await
            }
            // POST /projects/{pid}/blocks/{bid}/images - create image in  block
            (&Method::POST, ["projects", _project_id, "blocks", block_id, "images"]) => {
                images::create_image(&state.dynamo_client, &table_name, block_id, body).await
            }

            // --- CLASSES ---
            // GET /projects/{id}/classes - list project classes
            (&Method::GET, ["projects", project_id, "classes"]) => {
                classes::list_project_classes(&state.dynamo_client, &table_name, project_id).await
            }
            // POST /projects/{id}/classes - create class
            (&Method::POST, ["projects", project_id, "classes"]) => {
                classes::create_class(&state.dynamo_client, &table_name, project_id, body).await
            }
            // GET /projects/{pid}/classes/{cid} - get class
            (&Method::GET, ["projects", project_id, "classes", class_id]) => {
                classes::get_class(&state.dynamo_client, &table_name, project_id, class_id).await
            }
            // PATCH /projects/{pid}/classes/{cid} - update class
            (&Method::PATCH, ["projects", project_id, "classes", class_id]) => {
                classes::update_class(
                    &state.dynamo_client,
                    &table_name,
                    project_id,
                    class_id,
                    body,
                )
                .await
            }
            // DELETE /projects/{pid}/classes/{cid} - delete class
            (&Method::DELETE, ["projects", project_id, "classes", class_id]) => {
                classes::delete_class(&state.dynamo_client, &table_name, project_id, class_id).await
            }
            _ => not_found(),
        };
    }

    // Upload routes (S3) images
    if path.starts_with("/annotate/upload") {
        let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        tracing::info!("📎 Upload route matched - Parts: {:?}", parts);

        return match (method, parts.as_slice()) {
            // POST /annotate/upload/initiate - initiate upload (single or multipart)
            (&Method::POST, ["annotate", "upload", "initiate"]) => {
                let request: s3_multipart::InitiateUploadRequest = serde_json::from_slice(body)?;
                s3_multipart::initiate_upload(&state.s3_client, request).await
            }
            // POST /annotate/upload/complete - complete multipart upload
            (&Method::POST, ["annotate", "upload", "complete"]) => {
                let request: s3_multipart::CompleteMultipartRequest = serde_json::from_slice(body)?;
                s3_multipart::complete_multipart_upload(&state.s3_client, request).await
            }
            // DELETE /annotate/upload/abort - abort multipart upload
            (&Method::DELETE, ["annotate", "upload", "abort"]) => {
                let request: AbortUploadRequest = serde_json::from_slice(body)?;
                s3_multipart::abort_multipart_upload(
                    &state.s3_client,
                    request.project_id,
                    request.block_id,
                    request.image_id,
                    request.upload_id,
                    request.extension,
                )
                .await
            }
            _ => not_found(),
        };
    }

    // Images routes
    if path.starts_with("/images") {
        let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

        return match (method, parts.as_slice()) {
            // GET /images/{id} - get image
            (&Method::GET, ["images", image_id]) => {
                let block_id = event
                    .query_string_parameters_ref()
                    .and_then(|params| params.first("block_id"))
                    .ok_or("Missing block id query parameter")?;
                images::get_image(&state.dynamo_client, &table_name, block_id, image_id).await
            }
            // PATCH /images/{id} - update image
            (&Method::PATCH, ["images", image_id]) => {
                let block_id = event
                    .query_string_parameters_ref()
                    .and_then(|params| params.first("block_id"))
                    .ok_or("Missing block id query parameter")?;
                images::update_image(&state.dynamo_client, &table_name, block_id, image_id, body)
                    .await
            }
            // DELETE /images/{id} - delete image
            (&Method::DELETE, ["images", image_id]) => {
                let block_id = event
                    .query_string_parameters_ref()
                    .and_then(|params| params.first("block_id"))
                    .ok_or("Missing block id query parameter")?;
                images::delete_image(&state.dynamo_client, &table_name, block_id, image_id).await
            }
            // GET /images/{id}/annotations - list image annotations
            (&Method::GET, ["images", image_id, "annotations"]) => {
                annotations::list_image_annotations(&state.dynamo_client, &table_name, image_id)
                    .await
            }
            // POST /images/{id}/annotations - create annotation (requires ?project_id)
            (&Method::POST, ["images", image_id, "annotations"]) => {
                let project_id = event
                    .query_string_parameters_ref()
                    .and_then(|params| params.first("project_id"))
                    .unwrap_or("unknown");
                annotations::create_annotation(
                    &state.dynamo_client,
                    &table_name,
                    &user_id,
                    image_id,
                    project_id,
                    body,
                )
                .await
            }
            // POST /images/{id}/annotations/batch - batch create annotations
            (&Method::POST, ["images", image_id, "annotations", "batch"]) => {
                let project_id = event
                    .query_string_parameters_ref()
                    .and_then(|params| params.first("project_id"))
                    .unwrap_or("unknown");
                annotations::batch_create_annotations(
                    &state.dynamo_client,
                    &table_name,
                    &user_id,
                    image_id,
                    project_id,
                    body,
                )
                .await
            }
            // GET /images/{iid}/annotations/{aid} - get annotation
            (&Method::GET, ["images", image_id, "annotations", annotation_id]) => {
                annotations::get_annotation(
                    &state.dynamo_client,
                    &table_name,
                    image_id,
                    annotation_id,
                )
                .await
            }
            // PATCH /images/{iid}/annotations/{aid} - update annotation
            (&Method::PATCH, ["images", image_id, "annotations", annotation_id]) => {
                let project_id = event
                    .query_string_parameters_ref()
                    .and_then(|params| params.first("project_id"))
                    .unwrap_or("unknown");
                annotations::update_annotation(
                    &state.dynamo_client,
                    &table_name,
                    image_id,
                    annotation_id,
                    project_id,
                    body,
                )
                .await
            }
            // DELETE /images/{iid}/annotations/{aid} - delete annotation
            (&Method::DELETE, ["images", image_id, "annotations", annotation_id]) => {
                let project_id = event
                    .query_string_parameters_ref()
                    .and_then(|params| params.first("project_id"))
                    .unwrap_or("unknown");
                annotations::delete_annotation(
                    &state.dynamo_client,
                    &table_name,
                    image_id,
                    annotation_id,
                    project_id,
                )
                .await
            }
            _ => not_found(),
        };
    }

    // No matching route
    tracing::warn!("⚠️ No route matched - Method: {} Path: {}", method, path);
    not_found()
}

// Helper: parse bucket and key from an S3 URL like https://bucket.s3.amazonaws.com/key or https://s3.<region>.amazonaws.com/bucket/key
fn _parse_bucket_and_key(url: &str) -> Option<(String, String)> {
    let no_scheme = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);
    let (host, path) = no_scheme.split_once('/')?;

    // Handle both formats:
    // 1. bucket.s3.amazonaws.com/key
    // 2. s3.region.amazonaws.com/bucket/key
    let (bucket, key) = if host.starts_with("s3.") {
        // Format: s3.region.amazonaws.com/bucket/key
        let parts: Vec<&str> = path.splitn(2, '/').collect();
        if parts.len() == 2 {
            (parts[0].to_string(), parts[1].to_string())
        } else {
            return None;
        }
    } else {
        // Format: bucket.s3.amazonaws.com/key
        (host.split(".s3").next()?.to_string(), path.to_string())
    };

    Some((bucket, key))
}

async fn _list_block_images_signed(
    dynamo: &DynamoClient,
    _s3: &S3Client,
    table_name: &str,
    block_id: &str,
) -> Result<Response<Body>, Error> {
    let pk = format!("BLOCK#{}", block_id);

    let result = dynamo
        .query()
        .table_name(table_name)
        .key_condition_expression("PK = :pk AND begins_with(SK, :sk_prefix)")
        .expression_attribute_values(":pk", AttributeValue::S(pk))
        .expression_attribute_values(":sk_prefix", AttributeValue::S("IMAGE#".to_string()))
        .send()
        .await?;

    let mut images_json = Vec::new();

    for item in result.items() {
        if let Some(sk) = item.get("SK").and_then(|v| v.as_s().ok()) {
            if let Some(image_id) = sk.strip_prefix("IMAGE#") {
                let url_str = item
                    .get("url")
                    .and_then(|v| v.as_s().ok())
                    .map(|s| s.to_string())
                    .unwrap_or_default();

                // Generate Lambda proxy URL
                let final_url = if let Some((_bucket, key)) = _parse_bucket_and_key(&url_str) {
                    // Return URL that goes through Lambda proxy
                    format!("https://api.doxle.ai/proxy-image/{}", key)
                } else {
                    url_str.clone()
                };

                let locked = item
                    .get("locked")
                    .and_then(|v| v.as_bool().ok())
                    .copied()
                    .unwrap_or(false);
                let order = item
                    .get("order")
                    .and_then(|v| v.as_n().ok())
                    .and_then(|n| n.parse::<i32>().ok());
                let uploaded_at = item
                    .get("uploaded_at")
                    .and_then(|v| v.as_s().ok())
                    .map(|s| s.to_string())
                    .unwrap_or_default();

                images_json.push(serde_json::json!({
                    "image_id": image_id,
                    "block_id": block_id,
                    "url": final_url,
                    "locked": locked,
                    "order": order,
                    "uploaded_at": uploaded_at,
                }));
            }
        }
    }

    // Sort by order like shared implementation
    images_json.sort_by(|a, b| {
        let ao = a.get("order").and_then(|v| v.as_i64());
        let bo = b.get("order").and_then(|v| v.as_i64());
        match (ao, bo) {
            (Some(a), Some(b)) => a.cmp(&b),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        }
    });

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .header("Access-Control-Allow-Origin", "*")
        .body(serde_json::to_string(&images_json)?.into())
        .map_err(Box::new)?)
}

fn not_found() -> Result<Response<Body>, Error> {
    Ok(Response::builder()
        .status(StatusCode::NOT_FOUND)
        .header("Content-Type", "application/json")
        .header("Access-Control-Allow-Origin", "*")
        .body(serde_json::json!({"error": "Not found"}).to_string().into())
        .map_err(Box::new)?)
}
