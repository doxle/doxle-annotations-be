# Image Pyramid Implementation Plan

## ✅ Completed

1. **Added image processing dependency** (`be/Cargo.toml`)
   - Added `image = "0.24"` crate

2. **Created image processing module** (`be/shared/src/image_processing.rs`)
   - `needs_half_width()` - Smart thresholding logic
   - `generate_half_width()` - Resizes to 50% with Lanczos3 filter
   - `get_dimensions()` - Gets dimensions without full load

3. **Registered module** (`be/shared/src/lib.rs`)

4. **Cleaned test data**
   - Deleted all blocks from DynamoDB
   - Deleted all images from S3

---

## 🚧 TODO: Backend Integration

### Step 1: Update Upload Handler
File: `be/shared/src/s3_multipart.rs` or upload logic

```rust
// After file is uploaded to S3:
1. Download image from S3
2. Get dimensions
3. Check if needs_half_width()
4. If yes:
   - Generate half-width version
   - Upload to S3 at: projects/{pid}/blocks/{bid}/{img_id}/{width}w.jpg
5. Generate metadata.json
6. Upload metadata to S3
```

### Step 2: Create Metadata Structure
File: `be/shared/src/images.rs`

```rust
#[derive(Serialize, Deserialize)]
pub struct ImageMetadata {
    pub original_width: u32,
    pub original_height: u32,
    pub file_size: usize,
    pub format: String,
    pub levels: Vec<ImageLevel>,
}

#[derive(Serialize, Deserialize)]
pub struct ImageLevel {
    pub width: u32,
    pub path: String,  // e.g. "4955w.png" or "2477w.jpg"
    pub size: usize,
    pub purpose: String,  // "full" or "preview"
}
```

### Step 3: Add Metadata Endpoint
File: `be/lambdas/api-lambda/src/http_handler.rs`

```rust
GET /images/{id}/metadata
→ Returns metadata.json
```

---

## 🚧 TODO: Frontend Integration

### Step 1: Add Metadata Fetching
File: `fe/src/api/images.rs`

```rust
pub async fn get_image_metadata(image_id: &str) -> Result<ImageMetadata, String>
```

### Step 2: Update Canvas Loading Logic
File: `fe/src/canvas/canvas_page.rs`

```rust
1. Fetch metadata
2. If metadata.levels.len() > 1:
   - Load preview level first (fast)
   - Preload full level in background
3. Else:
   - Load original directly
```

---

## Storage Structure

```
s3://doxle-annotations/projects/{pid}/blocks/{bid}/{img_id}/

Small image (< 1MB, < 2048px):
  ├── original.jpg          # Only file

Large image (>= 1MB or >= 2048px):
  ├── 4955w.png             # Full (lossless)
  ├── 2477w.jpg             # Half (JPEG quality 85)
  └── metadata.json         # Metadata
```

---

## Testing Plan

1. Upload small image (< 1MB) → Check only original stored
2. Upload large image (> 1MB) → Check both versions stored
3. Load small image → Fast, no pyramid
4. Load large image → Preview loads first, then full
5. Zoom into large image → Uses full resolution

---

## Performance Expectations

**Before (large image 4955×3503, 1.8MB):**
- First load: 3-4 seconds
- CloudFront cached: 200-500ms

**After (with half-width 2477×1751, ~450KB):**
- First load preview: 800ms-1.5s
- CloudFront cached: 50-150ms
- **4x faster initial display!**

---

## Next Steps

1. Implement upload integration (Step 1)
2. Test with sample uploads
3. Deploy and verify metadata generation
4. Update frontend loading logic
5. Test end-to-end

**Start with upload integration?**
