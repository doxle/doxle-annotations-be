# CloudFront Setup Guide

Complete guide to deploy blazing-fast image delivery with CloudFront, Origin Access Control, and signed cookies.

## 📋 Prerequisites

- AWS CLI configured with appropriate credentials
- OpenSSL installed
- Existing S3 bucket: `doxle-annotations`
- DynamoDB table for your application

## 🔐 Step 1: Generate RSA Key Pair

Generate the RSA key pair for CloudFront signed cookies:

```bash
chmod +x scripts/generate-cloudfront-keys.sh
./scripts/generate-cloudfront-keys.sh
```

This creates:
- `keys/cloudfront-private-key.pem` - Private key (keep secure!)
- `keys/cloudfront-public-key.pem` - Public key
- `keys/cloudfront-public-key-no-headers.txt` - Public key for CloudFormation

**⚠️ IMPORTANT**: The private key is sensitive. Store it securely in AWS Secrets Manager or Lambda environment variables.

## ☁️ Step 2: Deploy CloudFormation Stack

Deploy the CloudFront distribution:

```bash
# Copy the public key (no headers) from the previous step
PUBLIC_KEY=$(cat keys/cloudfront-public-key-no-headers.txt)

# Deploy the stack
aws cloudformation create-stack \
  --stack-name doxle-cloudfront \
  --template-body file://cloudfront-stack.yaml \
  --parameters \
    ParameterKey=S3BucketName,ParameterValue=doxle-annotations \
    ParameterKey=TrustedKeyGroupPublicKey,ParameterValue="$PUBLIC_KEY" \
  --capabilities CAPABILITY_IAM

# Wait for stack to complete (takes ~10-15 minutes)
aws cloudformation wait stack-create-complete --stack-name doxle-cloudfront
```

### Get CloudFront Details

```bash
# Get CloudFront domain
CLOUDFRONT_DOMAIN=$(aws cloudformation describe-stacks \
  --stack-name doxle-cloudfront \
  --query 'Stacks[0].Outputs[?OutputKey==`CloudFrontDomainName`].OutputValue' \
  --output text)

# Get Public Key ID
PUBLIC_KEY_ID=$(aws cloudformation describe-stacks \
  --stack-name doxle-cloudfront \
  --query 'Stacks[0].Outputs[?OutputKey==`PublicKeyId`].OutputValue' \
  --output text)

echo "CloudFront Domain: $CLOUDFRONT_DOMAIN"
echo "Public Key ID: $PUBLIC_KEY_ID"
```

## 🔑 Step 3: Store Private Key in AWS Secrets Manager

```bash
aws secretsmanager create-secret \
  --name doxle/cloudfront-private-key \
  --description "CloudFront private key for signed cookies" \
  --secret-string file://keys/cloudfront-private-key.pem
```

## 🚀 Step 4: Update Lambda Environment Variables

Add these environment variables to your API Lambda:

```bash
aws lambda update-function-configuration \
  --function-name doxle-api-lambda \
  --environment Variables="{
    CLOUDFRONT_DOMAIN=$CLOUDFRONT_DOMAIN,
    CLOUDFRONT_KEY_PAIR_ID=$PUBLIC_KEY_ID,
    CLOUDFRONT_PRIVATE_KEY=$(cat keys/cloudfront-private-key.pem | tr '\n' ' ')
  }"
```

Or use AWS Secrets Manager (recommended):

```bash
# Grant Lambda permission to read the secret
aws secretsmanager resource-policy put \
  --secret-id doxle/cloudfront-private-key \
  --resource-policy '{
    "Version": "2012-10-17",
    "Statement": [{
      "Effect": "Allow",
      "Principal": {
        "Service": "lambda.amazonaws.com"
      },
      "Action": "secretsmanager:GetSecretValue",
      "Resource": "*"
    }]
  }'

# Update Lambda to fetch from Secrets Manager
aws lambda update-function-configuration \
  --function-name doxle-api-lambda \
  --environment Variables="{
    CLOUDFRONT_DOMAIN=$CLOUDFRONT_DOMAIN,
    CLOUDFRONT_KEY_PAIR_ID=$PUBLIC_KEY_ID,
    CLOUDFRONT_PRIVATE_KEY_SECRET=doxle/cloudfront-private-key
  }"
```

## 📦 Step 5: Deploy Backend Code

Build and deploy your updated Lambda:

```bash
# Build the Lambda
cargo lambda build --release --arm64

# Deploy
cargo lambda deploy doxle-api-lambda
```

## 🌐 Step 6: Update Frontend

Update your frontend to:

1. **Request signed cookies after login**:
```typescript
// After successful login
await fetch('https://your-api.com/auth/cloudfront-cookies', {
  method: 'POST',
  headers: {
    'Authorization': `Bearer ${accessToken}`,
  },
  credentials: 'include', // Important! Includes cookies
});
```

2. **Use CloudFront URLs for images**:
```typescript
// Before (presigned S3 URL)
const imageUrl = `https://doxle-annotations.s3.amazonaws.com/projects/${projectId}/blocks/${blockId}/${imageId}.jpg`;

// After (CloudFront URL)
const imageUrl = `https://${CLOUDFRONT_DOMAIN}/projects/${projectId}/blocks/${blockId}/${imageId}.jpg`;
```

3. **Add img attributes for performance**:
```tsx
<img
  key={imageId}  // Force remount on image change
  src={cloudfrontUrl}
  alt="annotation"
  decoding="async"
  fetchpriority="high"
  loading="lazy"
/>
```

## 🧪 Step 7: Test Locally

For local development, you have two options:

### Option A: Mock CloudFront (Recommended)
```bash
# Set env vars to skip CloudFront in local dev
export CLOUDFRONT_DOMAIN=""
```

The backend will skip cookie signing if `CLOUDFRONT_DOMAIN` is empty. Use presigned S3 URLs locally.

### Option B: Test with Real CloudFront
```bash
# Use real CloudFront domain locally
export CLOUDFRONT_DOMAIN="d123abc.cloudfront.net"
export CLOUDFRONT_KEY_PAIR_ID="K1234567890ABC"
export CLOUDFRONT_PRIVATE_KEY="$(cat keys/cloudfront-private-key.pem)"

# Run your Lambda locally
cargo lambda watch
```

## ✅ Step 8: Verify Setup

### Test Cookie Issuance
```bash
curl -X POST https://your-api.com/auth/cloudfront-cookies \
  -H "Authorization: Bearer YOUR_JWT_TOKEN" \
  -v
```

Look for `Set-Cookie` headers:
- `CloudFront-Policy`
- `CloudFront-Signature`
- `CloudFront-Key-Pair-Id`

### Test Image Access
```bash
# Without cookies (should fail)
curl -I https://${CLOUDFRONT_DOMAIN}/projects/test/blocks/test/test.jpg

# With cookies (should succeed)
curl -I https://${CLOUDFRONT_DOMAIN}/projects/test/blocks/test/test.jpg \
  -H "Cookie: CloudFront-Policy=...; CloudFront-Signature=...; CloudFront-Key-Pair-Id=..."
```

## 🎯 Performance Optimizations

### Already Enabled
✅ HTTP/3 (QUIC)  
✅ Gzip/Brotli compression  
✅ Origin Shield (reduces S3 requests)  
✅ Range requests (partial content)  
✅ Immutable cache (1 year TTL)  
✅ Long-lived signed cookies (12 hours)

### Next Steps (Optional)

#### 1. Add Custom Domain
```bash
# Request ACM certificate in us-east-1
aws acm request-certificate \
  --domain-name images.yourdomain.com \
  --validation-method DNS \
  --region us-east-1
```

Then update CloudFormation stack with `Aliases` and `ViewerCertificate`.

#### 2. Generate Image Variants (WebP/AVIF)
See `lambdas/image-processor-lambda/` (to be created) for automatic variant generation.

#### 3. Implement Tiling for Large Images
See `lambdas/tile-generator-lambda/` (to be created) for deep-zoom pyramids.

## 🔄 Update Existing Images

Existing images in S3 are already accessible via CloudFront. Update DynamoDB records to use CloudFront URLs:

```bash
# Script to update URLs (run once)
aws dynamodb scan --table-name doxle-annotations | \
  jq -r '.Items[] | select(.url.S) | .url.S' | \
  grep "s3.amazonaws.com" | \
  # Replace with CloudFront domain and update DynamoDB
```

Or let the backend handle it dynamically (recommended): always construct CloudFront URLs from stored S3 keys.

## 🚨 Troubleshooting

### Images return 403 Forbidden
- Check that cookies are being sent with requests (`credentials: 'include'`)
- Verify `Access-Control-Allow-Credentials: true` in API responses
- Ensure frontend and API share same domain or use proper CORS

### Cookies not being set
- Check that `Secure` flag matches (HTTPS in prod, HTTP in local)
- Verify `Domain` attribute matches your frontend domain
- Check `SameSite=None` with `Secure` for cross-origin

### CloudFront returns stale content
- Invalidate cache: `aws cloudfront create-invalidation --distribution-id D123 --paths "/*"`
- Or use versioned S3 keys: `image-v2.jpg` instead of `image.jpg`

## 📊 Monitoring

### CloudFront Metrics (CloudWatch)
- `Requests` - Total requests
- `BytesDownloaded` - Data transfer
- `4xxErrorRate` / `5xxErrorRate` - Error rates
- `CacheHitRate` - Cache efficiency

### Enable Real-Time Logs (Optional)
```bash
aws cloudfront create-realtime-log-config \
  --name doxle-cf-logs \
  --sampling-rate 100 \
  --end-points Type=Kinesis,KinesisStreamArn=arn:aws:kinesis:... \
  --fields timestamp c-ip cs-method cs-uri-stem
```

## 💰 Cost Optimization

- **Origin Shield**: ~$0.01/10k requests (worth it for high traffic)
- **PriceClass_100**: North America + Europe only (cheapest)
- **Long TTL**: Fewer S3 requests = lower costs
- **Compression**: Smaller transfers = lower data transfer costs

Estimated cost for 1M image requests/month: ~$10-20

## 🔒 Security Best Practices

1. ✅ S3 bucket is private (no public access)
2. ✅ CloudFront uses Origin Access Control (OAC)
3. ✅ Signed cookies expire (12 hours)
4. ✅ Private key stored in Secrets Manager
5. ✅ HTTPS only (`redirect-to-https`)
6. ✅ HttpOnly cookies (no JS access)

## 📚 Additional Resources

- [CloudFront Signed Cookies](https://docs.aws.amazon.com/AmazonCloudFront/latest/DeveloperGuide/private-content-signed-cookies.html)
- [Origin Access Control](https://docs.aws.amazon.com/AmazonCloudFront/latest/DeveloperGuide/private-content-restricting-access-to-s3.html)
- [CloudFront Performance](https://docs.aws.amazon.com/AmazonCloudFront/latest/DeveloperGuide/distribution-web-values-specify.html)
