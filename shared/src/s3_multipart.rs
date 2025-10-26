use lambda_http::{Body, Error, Response, http::StatusCode};
use aws_sdk_s3::Client as S3Client;
use serde::{Deserialize, Serialize};

const BUCKET_NAME: &str = "doxle-annotations";
const MULTIPART_THRESHOLD: usize = 5 * 1024 * 1024; // 5MB

#[derive(Deserialize)]
pub struct InitiateUploadRequest {
    pub project_id: String,
    pub block_id: String,
    pub file_name: String,
    pub content_type: String,
    pub file_size: usize,
}

#[derive(Serialize)]
pub struct InitiateUploadResponse {
    pub image_id: String,
    pub upload_id: Option<String>, // For multipart
    pub upload_urls: Vec<UploadPart>,
    pub is_multipart: bool,
    pub extension: String,
}

#[derive(Serialize)]
pub struct UploadPart {
    pub part_number: i32,
    pub upload_url: String,
}

#[derive(Deserialize)]
pub struct CompleteMultipartRequest {
    pub project_id: String,
    pub block_id: String,
    pub image_id: String,
    pub upload_id: String,
    pub extension: String,
    pub parts: Vec<CompletedPart>,
}

#[derive(Deserialize, Serialize)]
pub struct CompletedPart {
    pub part_number: i32,
    pub etag: String,
}

#[derive(Serialize)]
pub struct UploadCompleteResponse {
    pub image_id: String,
    pub url: String,
}

/// Initiate upload - returns single or multipart presigned URLs
pub async fn initiate_upload(
    s3_client: &S3Client,
    request: InitiateUploadRequest,
) -> Result<Response<Body>, Error> {
    let image_id = uuid::Uuid::new_v4().to_string();
    
    let extension = request.file_name
        .split('.')
        .last()
        .unwrap_or("jpg")
        .to_string();
    
    let s3_key = format!(
        "projects/{}/blocks/{}/{}.{}",
        request.project_id,
        request.block_id,
        image_id,
        extension
    );
    
    let is_multipart = request.file_size >= MULTIPART_THRESHOLD;
    
    if is_multipart {
        // Multipart upload for files >= 5MB
        let num_parts = (request.file_size as f64 / MULTIPART_THRESHOLD as f64).ceil() as i32;
        
        // Initiate multipart upload
        let create_result = s3_client
            .create_multipart_upload()
            .bucket(BUCKET_NAME)
            .key(&s3_key)
            .content_type(&request.content_type)
            .send()
            .await
            .map_err(|e| format!("Failed to initiate multipart upload: {}", e))?;
        
        let upload_id = create_result.upload_id()
            .ok_or("No upload ID returned")?
            .to_string();
        
        // Generate presigned URLs for each part
        let mut upload_parts = Vec::new();
        
        for part_number in 1..=num_parts {
            let presigned = s3_client
                .upload_part()
                .bucket(BUCKET_NAME)
                .key(&s3_key)
                .upload_id(&upload_id)
                .part_number(part_number)
                .presigned(
                    aws_sdk_s3::presigning::PresigningConfig::expires_in(
                        std::time::Duration::from_secs(3600)
                    )?
                )
                .await
                .map_err(|e| format!("Failed to generate presigned URL for part {}: {}", part_number, e))?;
            
            upload_parts.push(UploadPart {
                part_number,
                upload_url: presigned.uri().to_string(),
            });
        }
        
        let response = InitiateUploadResponse {
            image_id: image_id.clone(),
            upload_id: Some(upload_id),
            upload_urls: upload_parts,
            is_multipart: true,
            extension: extension.clone(),
        };
        
        Ok(Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "application/json")
            .header("Access-Control-Allow-Origin", "*")
            .body(serde_json::to_string(&response)?.into())
            .map_err(Box::new)?)
            
    } else {
        // Single part upload for files < 5MB
        let presigned = s3_client
            .put_object()
            .bucket(BUCKET_NAME)
            .key(&s3_key)
            .content_type(&request.content_type)
            .presigned(
                aws_sdk_s3::presigning::PresigningConfig::expires_in(
                    std::time::Duration::from_secs(3600)
                )?
            )
            .await
            .map_err(|e| format!("Failed to generate presigned URL: {}", e))?;
        
        let response = InitiateUploadResponse {
            image_id: image_id.clone(),
            upload_id: None,
            upload_urls: vec![UploadPart {
                part_number: 1,
                upload_url: presigned.uri().to_string(),
            }],
            is_multipart: false,
            extension: extension.clone(),
        };
        
        Ok(Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "application/json")
            .header("Access-Control-Allow-Origin", "*")
            .body(serde_json::to_string(&response)?.into())
            .map_err(Box::new)?)
    }
}

/// Complete multipart upload
pub async fn complete_multipart_upload(
    s3_client: &S3Client,
    request: CompleteMultipartRequest,
) -> Result<Response<Body>, Error> {
    let s3_key = format!(
        "projects/{}/blocks/{}/{}.{}",
        request.project_id,
        request.block_id,
        request.image_id,
        request.extension
    );
    
    // Build completed parts
    let mut completed_parts = Vec::new();
    for part in &request.parts {
        let completed_part = aws_sdk_s3::types::CompletedPart::builder()
            .part_number(part.part_number)
            .e_tag(&part.etag)
            .build();
        completed_parts.push(completed_part);
    }
    
    let completed_upload = aws_sdk_s3::types::CompletedMultipartUpload::builder()
        .set_parts(Some(completed_parts))
        .build();
    
    // Complete the multipart upload
    s3_client
        .complete_multipart_upload()
        .bucket(BUCKET_NAME)
        .key(&s3_key)
        .upload_id(&request.upload_id)
        .multipart_upload(completed_upload)
        .send()
        .await
        .map_err(|e| format!("Failed to complete multipart upload: {}", e))?;
    
    // Generate public URL
    let url = format!(
        "https://{}.s3.amazonaws.com/{}",
        BUCKET_NAME,
        s3_key
    );
    
    let response = UploadCompleteResponse {
        image_id: request.image_id.clone(),
        url,
    };
    
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .header("Access-Control-Allow-Origin", "*")
        .body(serde_json::to_string(&response)?.into())
        .map_err(Box::new)?)
}

/// Abort multipart upload (cleanup on failure)
pub async fn abort_multipart_upload(
    s3_client: &S3Client,
    project_id: String,
    block_id: String,
    image_id: String,
    upload_id: String,
) -> Result<Response<Body>, Error> {
    let s3_key = format!(
        "projects/{}/blocks/{}/{}.jpg",
        project_id,
        block_id,
        image_id
    );
    
    s3_client
        .abort_multipart_upload()
        .bucket(BUCKET_NAME)
        .key(&s3_key)
        .upload_id(&upload_id)
        .send()
        .await
        .map_err(|e| format!("Failed to abort multipart upload: {}", e))?;
    
    Ok(Response::builder()
        .status(StatusCode::NO_CONTENT)
        .header("Access-Control-Allow-Origin", "*")
        .body(Body::Empty)
        .map_err(Box::new)?)
}
