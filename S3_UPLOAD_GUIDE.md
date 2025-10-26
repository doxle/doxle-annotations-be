# S3 Upload Implementation

## Backend (Completed)

### S3 Structure
```
s3://doxle-annotations/
  └── projects/{project_id}/
      └── blocks/{block_id}/
          └── {image_id}.{ext}
```

### New Files
- `be/shared/src/s3.rs` - S3 upload logic with two approaches:
  1. **Direct upload**: Send base64 data through Lambda (good for <6MB files)
  2. **Presigned URLs**: Get upload URL, upload directly to S3 from frontend (better for large files)

### Changes Made
1. Added `aws-sdk-s3 = "1.108"` to workspace dependencies
2. Added `S3Client` to `AppState`
3. Initialized S3 client in api-lambda

### TODO: Add Routes
Add to `be/lambdas/api-lambda/src/http_handler.rs`:
```rust
// POST /blocks/{id}/images/upload - direct upload
(&Method::POST, ["blocks", block_id, "images", "upload"]) => {
    let request: doxle_shared::s3::UploadImageRequest = serde_json::from_slice(body)?;
    doxle_shared::s3::upload_image(&state.s3_client, request).await
}

// POST /blocks/{id}/images/presigned - get presigned URL
(&Method::POST, ["blocks", block_id, "images", "presigned"]) => {
    #[derive(serde::Deserialize)]
    struct PresignedRequest {
        file_name: String,
        content_type: String,
    }
    let req: PresignedRequest = serde_json::from_slice(body)?;
    
    doxle_shared::s3::generate_presigned_upload_url(
        &state.s3_client,
        project_id.to_string(),
        block_id.to_string(),
        req.file_name,
        req.content_type,
    ).await
}
```

### TODO: Link Images to Blocks
After upload, call `images::create_image()` to store the image URL in DynamoDB

## Frontend (TODO)

### 1. Create Image Upload API Client
File: `fe/src/api/images.rs`
```rust
pub async fn upload_image(
    block_id: &str,
    project_id: &str,
    file_name: String,
    content_type: String,
    file_data: String, // base64
) -> Result<ImageUploadResponse, String>
```

### 2. Update AddBlockModal
- Read File objects from drag/drop
- Convert to base64
- Call upload API for each image
- Show progress
- Store returned URLs in images state

### 3. Handle File Reading in WASM
```rust
use web_sys::{File, FileReader};
use wasm_bindgen::closure::Closure;

// Convert File to base64 string
```

## AWS Setup Required

1. **Create S3 Bucket**:
   ```bash
   aws s3 mb s3://doxle-annotations
   ```

2. **Set CORS** (for presigned URLs):
   ```json
   {
     "CORSRules": [{
       "AllowedOrigins": ["*"],
       "AllowedMethods": ["PUT", "POST"],
       "AllowedHeaders": ["*"]
     }]
   }
   ```

3. **Set Bucket Policy** (public read for images):
   ```json
   {
     "Version": "2012-10-17",
     "Statement": [{
       "Effect": "Allow",
       "Principal": "*",
       "Action": "s3:GetObject",
       "Resource": "arn:aws:s3:::doxle-annotations/projects/*"
     }]
   }
   ```

4. **Lambda IAM Role** needs S3 permissions:
   ```json
   {
     "Effect": "Allow",
     "Action": [
       "s3:PutObject",
       "s3:GetObject",
       "s3:DeleteObject"
     ],
     "Resource": "arn:aws:s3:::doxle-annotations/*"
   }
   ```

## Recommendation

Use **presigned URLs** approach for better performance:
- Large files don't go through Lambda (6MB limit)
- Faster uploads (direct to S3)
- Lower Lambda costs
- Better UX with upload progress
