use lambda_http::{Body, Error, Response, http::StatusCode};
use aws_sdk_s3::Client as S3Client;
use aws_sdk_s3::primitives::ByteStream;

const BUCKET_NAME: &str = "doxle-annotations";
const MULTIPART_THRESHOLD: usize = 5 * 1024 * 1024; // 5MB
const CHUNK_SIZE: usize = 5 * 1024 * 1024; // 5MB chunks

#[derive(serde::Deserialize)]
pub struct UploadImageRequest {
    pub project_id: String,
    pub block_id: String,
    pub file_name: String,
    pub content_type: String,
    pub file_data: String, // base64 encoded
}

#[derive(serde::Serialize)]
pub struct UploadImageResponse {
    pub image_id: String,
    pub url: String,
}

/// Upload an image to S3 and return the URL
pub async fn upload_image(
    s3_client: &S3Client,
    request: UploadImageRequest,
) -> Result<Response<Body>, Error> {
    // Generate unique image ID
    let image_id = uuid::Uuid::new_v4().to_string();
    
    // Get file extension from filename
    let extension = request.file_name
        .split('.')
        .last()
        .unwrap_or("jpg");
    
    // S3 key: projects/{project_id}/blocks/{block_id}/{image_id}.{ext}
    let s3_key = format!(
        "projects/{}/blocks/{}/{}.{}",
        request.project_id,
        request.block_id,
        image_id,
        extension
    );
    
    // Decode base64 file data
    use base64::Engine;
    let file_bytes = base64::engine::general_purpose::STANDARD
        .decode(&request.file_data)
        .map_err(|e| format!("Failed to decode base64: {}", e))?;
    
    // Upload to S3
    s3_client
        .put_object()
        .bucket(BUCKET_NAME)
        .key(&s3_key)
        .body(ByteStream::from(file_bytes))
        .content_type(&request.content_type)
        .send()
        .await
        .map_err(|e| format!("Failed to upload to S3: {}", e))?;
    
    // Generate public URL
    let url = format!(
        "https://{}.s3.amazonaws.com/{}",
        BUCKET_NAME,
        s3_key
    );
    
    let response = UploadImageResponse {
        image_id: image_id.clone(),
        url,
    };
    
    Ok(Response::builder()
        .status(StatusCode::CREATED)
        .header("Content-Type", "application/json")
        .header("Access-Control-Allow-Origin", "*")
        .body(serde_json::to_string(&response)?.into())
        .map_err(Box::new)?)
}

/// Generate a presigned URL for direct upload (alternative approach)
pub async fn generate_presigned_upload_url(
    s3_client: &S3Client,
    project_id: String,
    block_id: String,
    file_name: String,
    content_type: String,
) -> Result<Response<Body>, Error> {
    let image_id = uuid::Uuid::new_v4().to_string();
    
    let extension = file_name
        .split('.')
        .last()
        .unwrap_or("jpg");
    
    let s3_key = format!(
        "projects/{}/blocks/{}/{}.{}",
        project_id,
        block_id,
        image_id,
        extension
    );
    
    // Generate presigned URL (expires in 1 hour)
    let presigned_request = s3_client
        .put_object()
        .bucket(BUCKET_NAME)
        .key(&s3_key)
        .content_type(&content_type)
        .presigned(
            aws_sdk_s3::presigning::PresigningConfig::expires_in(
                std::time::Duration::from_secs(3600)
            )?
        )
        .await
        .map_err(|e| format!("Failed to generate presigned URL: {}", e))?;
    
    let response = serde_json::json!({
        "image_id": image_id,
        "upload_url": presigned_request.uri(),
        "method": "PUT",
        "headers": {
            "Content-Type": content_type
        }
    });
    
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .header("Access-Control-Allow-Origin", "*")
        .body(response.to_string().into())
        .map_err(Box::new)?)
}
