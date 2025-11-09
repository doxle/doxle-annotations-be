# Image Pyramid Testing Checklist

## Pre-Test
- [x] Backend code deployed to GitHub
- [ ] Lambda redeployed (run SAM/CDK deployment)
- [ ] Cleared all old blocks/images from DB & S3

---

## Test 1: Small Image (< 1MB, < 2048px)

**Upload:** Image that's 904×1280, ~291KB

**Expected S3 structure:**
```
s3://doxle-annotations/projects/{pid}/blocks/{bid}/{img_id}.jpg
```
(Flat file, no pyramid)

**Verification:**
```bash
aws s3 ls s3://doxle-annotations/projects/ --recursive
```

Should see: Single file, no folder

---

## Test 2: Large Image (>= 1MB or >= 2048px)

**Upload:** Image that's 4955×3503, ~1.8MB

**Expected S3 structure:**
```
s3://doxle-annotations/projects/{pid}/blocks/{bid}/{img_id}/
  ├── 4955w.png              # Full resolution
  ├── 2477w.jpg              # Half-width preview
  └── metadata.json          # Metadata
```

**Verification:**
```bash
# List files
aws s3 ls s3://doxle-annotations/projects/{pid}/blocks/{bid}/{img_id}/

# Download metadata
aws s3 cp s3://doxle-annotations/projects/{pid}/blocks/{bid}/{img_id}/metadata.json - | jq
```

**Expected metadata.json:**
```json
{
  "original_width": 4955,
  "original_height": 3503,
  "file_size": 1870256,
  "format": "png",
  "levels": [
    {
      "width": 4955,
      "height": 3503,
      "path": "4955w.png",
      "size": 1870256,
      "purpose": "full"
    },
    {
      "width": 2477,
      "height": 1751,
      "path": "2477w.jpg",
      "size": ~420000,
      "purpose": "preview"
    }
  ]
}
```

---

## Check Lambda Logs

```bash
# Get recent logs
aws logs tail /aws/lambda/api-lambda --follow --since 5m
```

**Look for:**
- ✅ "📥 Downloading image from S3"
- ✅ "📐 Image dimensions: 4955x3503"
- ✅ "🔄 Generating half-width version..."
- ✅ "📤 Uploading full resolution to..."
- ✅ "📤 Uploading half-width to..."
- ✅ "📤 Uploading metadata to..."
- ✅ "✅ Image processing complete"

**Or errors:**
- ❌ "Failed to download image"
- ❌ "Failed to generate half-width"
- ❌ "Failed to upload..."

---

## Common Issues

### Issue: No pyramid generated for large image
**Check:** Lambda timeout (increase to 30s+)
**Check:** Lambda memory (increase to 512MB+)

### Issue: Image processing fails
**Check:** Lambda logs for specific error
**Check:** Image format supported (JPG, PNG)

### Issue: Metadata not created
**Check:** S3 permissions (Lambda needs PutObject)
**Check:** Serialization error in logs

---

## Next Steps After Testing

Once verified working:
1. ✅ Add metadata endpoint (GET /images/{id}/metadata)
2. ✅ Update frontend to fetch metadata
3. ✅ Load preview first, then full resolution
4. ✅ Test end-to-end with real annotations workflow
