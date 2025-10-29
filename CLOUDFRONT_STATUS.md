# CloudFront Setup Status

## ✅ Configuration Complete

### Infrastructure
- **Frontend Domain**: `annotate.doxle.ai` (Amplify)
- **Images Domain**: `images.doxle.ai` (CloudFront)
- **CloudFront Distribution ID**: `E2ISJJ9IE221LF`
- **S3 Origin**: `doxle-annotations.s3.ap-southeast-2.amazonaws.com`

### CloudFront Settings
✅ **Signed Cookies Enabled**: Distribution requires trusted key group  
✅ **CORS Configured**: Added CORS headers policy with:
- `Access-Control-Allow-Origin: https://annotate.doxle.ai`
- `Access-Control-Allow-Credentials: true`
- `Access-Control-Allow-Methods: GET, HEAD`
- `Access-Control-Allow-Headers: Origin, Accept, Content-Type, Authorization`

### Backend Configuration (Lambda)
✅ **Environment Variables**:
```
CLOUDFRONT_DOMAIN=images.doxle.ai
CLOUDFRONT_KEY_PAIR_ID=K2GG6JANWG7Z83
CLOUDFRONT_PRIVATE_KEY=[configured]
CLOUDFRONT_COOKIE_DOMAIN=.doxle.ai
```

✅ **Endpoints**:
- `/auth/cloudfront-cookies` - Issues signed cookies after login
- `/blocks/{id}/images` - Returns image URLs using CloudFront domain

### Frontend Configuration
✅ **Login Flow**: Calls `/auth/cloudfront-cookies` after successful authentication  
✅ **Image Loading**: Uses `crossorigin="use-credentials"` to send cookies with image requests  
✅ **Cookie Storage**: Browser stores cookies with `Domain=.doxle.ai` (works across subdomains)

## How It Works

1. **User logs in** → Frontend calls `/login` endpoint
2. **Frontend requests cookies** → Calls `/auth/cloudfront-cookies` with JWT token
3. **Backend sets signed cookies** → Lambda generates CloudFront policy and signature, sets 3 cookies:
   - `CloudFront-Policy`
   - `CloudFront-Signature`
   - `CloudFront-Key-Pair-Id`
4. **Frontend loads images** → Browser automatically includes cookies with requests to `images.doxle.ai`
5. **CloudFront validates** → Checks cookie signature against trusted key group
6. **Image served** → CloudFront serves from cache or fetches from S3

## Testing

### Verify Cookies Are Set
Open browser DevTools → Application → Cookies → Check for CloudFront cookies on `.doxle.ai` domain

### Verify Image Loading
1. Login to `https://annotate.doxle.ai`
2. Open a canvas page with images
3. Check Network tab - images should load from `https://images.doxle.ai/projects/...`
4. Response headers should include:
   - `Access-Control-Allow-Origin: https://annotate.doxle.ai`
   - `Access-Control-Allow-Credentials: true`

### Troubleshooting

**Images show broken icon:**
- Check if CloudFront cookies are present in DevTools
- Verify Network tab shows 200 response (not 403 Forbidden)
- Check CORS error in console
- Verify image URL uses `images.doxle.ai` (not S3 domain)

**403 Forbidden on images:**
- Cookies might not be set → Check `/auth/cloudfront-cookies` was called after login
- Cookies expired → Re-login to get fresh cookies (12 hour expiry)
- Wrong domain → Verify cookies have `Domain=.doxle.ai`

**CORS error:**
- Verify CloudFront response headers policy is applied
- Check Origin header matches `https://annotate.doxle.ai` exactly
- Ensure `crossorigin="use-credentials"` is set on img tag

## Performance Benefits

With CloudFront properly configured:
- ⚡ **Edge caching** - Images served from nearest CloudFront edge location
- 🔒 **Secure** - Signed cookies prevent unauthorized access
- 🚀 **Fast** - Immutable cache (1 year) + HTTP/3 + Brotli compression
- 💰 **Cost-effective** - Fewer S3 requests, lower bandwidth costs

## Next Steps

If images still not loading:
1. Clear browser cache and cookies
2. Re-login to get fresh CloudFront cookies
3. Check browser console for detailed error messages
4. Verify Lambda logs show successful cookie generation
5. Test image URL directly in browser (should work after login)
