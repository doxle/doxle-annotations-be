# CloudFront + API Gateway Architecture

## Visual Flow Diagram

```
┌─────────────────────────────────────────────────────────────────────┐
│                         USER BROWSER                                │
│                    (annotate.doxle.ai)                              │
└──────────────────────────┬──────────────────────────────────────────┘
                           │
                           │ Request: GET /proxy-image/projects/123/blocks/456/img.jpg
                           │ Headers: Authorization: Bearer <jwt>
                           │
                           ▼
┌─────────────────────────────────────────────────────────────────────┐
│                      CLOUDFRONT CDN                                 │
│                    (Global Edge Locations)                          │
│                                                                     │
│  ┌──────────────────────────────────────────────────────────────┐ │
│  │  CACHE CHECK: Is /proxy-image/.../img.jpg cached?           │ │
│  └──────────────────────────────────────────────────────────────┘ │
│                           │                                         │
│              ┌────────────┴────────────┐                           │
│              │                         │                           │
│         CACHE HIT ✓              CACHE MISS ✗                     │
│              │                         │                           │
│              │                         │                           │
│       ┌──────▼──────┐          ┌──────▼──────────────────┐       │
│       │ Return from │          │ Forward to Origin       │       │
│       │ Edge Cache  │          │ (API Gateway)           │       │
│       │ (~50ms)     │          │                         │       │
│       └─────────────┘          └─────────────────────────┘       │
│                                         │                          │
└─────────────────────────────────────────┼──────────────────────────┘
                                          │
                                          │ Forward Auth Headers
                                          │
                                          ▼
                        ┌──────────────────────────────────────┐
                        │        API GATEWAY                   │
                        │  (abc123.execute-api.amazonaws.com)  │
                        └──────────────┬───────────────────────┘
                                       │
                                       │ Invoke
                                       │
                                       ▼
                        ┌──────────────────────────────────────┐
                        │         LAMBDA FUNCTION              │
                        │    /proxy-image/* handler            │
                        │                                      │
                        │  1. Validate JWT (optional)          │
                        │  2. Fetch from S3                    │
                        │  3. Return image + headers           │
                        └──────────────┬───────────────────────┘
                                       │
                                       │ GetObject
                                       │
                                       ▼
                        ┌──────────────────────────────────────┐
                        │          S3 BUCKET                   │
                        │      doxle-annotations               │
                        │                                      │
                        │  projects/123/blocks/456/img.jpg     │
                        └──────────────────────────────────────┘
```

## How It Works

### First Request (Cache Miss) - Slow
**User in Sydney requests image:**

1. **Browser** → CloudFront Sydney Edge (~20ms latency)
2. **CloudFront Sydney** → No cache, forward to origin
3. **CloudFront** → API Gateway in ap-southeast-2 (~100ms)
4. **API Gateway** → Lambda (~50ms cold start)
5. **Lambda** → Validate JWT (~10ms)
6. **Lambda** → S3 GetObject (~200ms)
7. **Lambda** → Return image to CloudFront
8. **CloudFront** → Cache image at Sydney edge
9. **CloudFront** → Return to browser

**Total: ~500-800ms** ⏱️

---

### Second Request (Cache Hit) - BLAZING FAST
**Another user (or same user) in Sydney requests same image:**

1. **Browser** → CloudFront Sydney Edge (~20ms latency)
2. **CloudFront Sydney** → ✅ **HIT! Return from edge cache**
3. **Browser** receives image

**Total: ~20-50ms** ⚡ **10-40x faster!**

---

### Global Behavior

#### User in London requests image (first time in London):
1. **Browser** → CloudFront London Edge (~10ms latency)
2. **CloudFront London** → No cache locally, forward to origin
3. **CloudFront** → API Gateway in ap-southeast-2 (~300ms from London)
4. Full flow through Lambda + S3
5. **CloudFront** → Cache at **London edge**
6. Return to browser

**Total: ~800-1200ms** (slower due to distance to origin)

#### Next user in London requests same image:
1. **Browser** → CloudFront London Edge
2. **CloudFront London** → ✅ **HIT!**
3. Return from London edge cache

**Total: ~20-50ms** ⚡

---

## Key Points

### ✅ YES - Global Caching
- **Each CloudFront edge location caches independently**
- First user in each region is slow (cache miss)
- All subsequent users in that region are **blazing fast**
- Images cached for **1 year** (immutable)

### ⚡ Performance by Request Type

| Scenario | Latency | What Happens |
|----------|---------|--------------|
| First user globally | 500-800ms | Lambda + S3 + cache at edge |
| Same edge (cached) | 20-50ms | Served from edge cache |
| Different edge (first time) | 800-1200ms | Lambda + S3 + cache at new edge |
| Different edge (cached) | 20-50ms | Served from local edge |

### 🌍 Edge Locations
CloudFront has **450+ edge locations** worldwide:
- Sydney, Melbourne (Australia)
- Singapore, Tokyo, Seoul (Asia)
- London, Paris, Frankfurt (Europe)
- New York, LA, Chicago (US)
- etc.

Each edge caches independently!

### 🔐 Security
- JWT validation happens in Lambda (cache miss only)
- Cached responses are **public** (no user-specific data)
- If you need per-user images, we'd need a different approach

### 💰 Cost
- First request: Lambda + S3 + CloudFront
- Cached requests: Only CloudFront (very cheap)
- With 1-year cache, most requests = edge hits = huge savings!

---

## Summary

**Your understanding is CORRECT!** ✅

- **First load globally**: Slow (500-1200ms depending on region)
- **Every subsequent load**: Fast (~20-50ms from nearest edge)
- **Per edge location**: Each region caches independently
- **Cache duration**: 1 year = essentially permanent for immutable images

This is **exactly** how companies like Netflix, Spotify, Instagram serve images fast globally! 🚀
