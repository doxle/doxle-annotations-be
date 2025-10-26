# DynamoDB Migration - Implementation Summary

## ✅ Backend Complete (doxle-core/lambdas/doxle-api)

### What Changed:
1. **Consolidated Lambda** - All functionality in `doxle-api` (can delete `doxle-auth` and `doxle-user`)
2. **New Modules Created:**
   - `projects.rs` - Project CRUD
   - `blocks.rs` - Block CRUD (renamed from tasks)
   - `images.rs` - Image CRUD
   - `annotations.rs` - Polygon/BBox annotations with batch support
   - `classes.rs` - Class management with auto-counting

3. **Updated Files:**
   - `types.rs` - Full data model (User, Project, Block, Image, Annotation, Class)
   - `http_handler.rs` - Complete REST API routing
   - `main.rs` - Module declarations
   - `Cargo.toml` - Added uuid dependency

### API Endpoints:

#### Auth (no JWT required):
- `POST /login` - Login with email/password

#### Users:
- `POST /users` - Create user profile
- `GET /users/me` - Get current user
- `PATCH /users/me` - Update user

#### Projects:
- `POST /projects` - Create project
- `GET /projects` - List user's projects
- `GET /projects/{id}` - Get project
- `PATCH /projects/{id}` - Update project
- `DELETE /projects/{id}` - Delete project

#### Blocks:
- `GET /projects/{id}/blocks` - List project blocks
- `POST /projects/{id}/blocks` - Create block
- `GET /blocks/{id}` - Get block
- `PATCH /blocks/{id}` - Update block
- `DELETE /blocks/{id}` - Delete block

#### Images:
- `GET /blocks/{id}/images` - List block images
- `POST /blocks/{id}/images` - Create image
- `GET /images/{id}` - Get image
- `PATCH /images/{id}` - Update image
- `DELETE /images/{id}` - Delete image

#### Annotations:
- `GET /images/{id}/annotations` - List all annotations for image
- `POST /images/{id}/annotations?project_id={pid}` - Create annotation
- `POST /images/{id}/annotations/batch?project_id={pid}` - Batch create
- `GET /images/{id}/annotations/{aid}` - Get annotation
- `PATCH /images/{id}/annotations/{aid}?project_id={pid}` - Update annotation
- `DELETE /images/{id}/annotations/{aid}?project_id={pid}` - Delete annotation

#### Classes:
- `GET /projects/{id}/classes` - List project classes
- `POST /projects/{id}/classes` - Create class
- `GET /projects/{id}/classes/{cid}` - Get class
- `PATCH /projects/{id}/classes/{cid}` - Update class
- `DELETE /projects/{id}/classes/{cid}` - Delete class

### DynamoDB Schema:

```
USER#{user_id} / USER#{user_id} - User profile
USER#{user_id} / PROJECT#{project_id} - User-Project relationship
PROJECT#{project_id} / PROJECT#{project_id} - Project
PROJECT#{project_id} / USER#{user_id} - Project-User relationship
PROJECT#{project_id} / CLASS#{class_id} - Class
PROJECT#{project_id} / BLOCK#{block_id} - Block under project
BLOCK#{block_id} / BLOCK#{block_id} - Block (for easy lookup)
BLOCK#{block_id} / IMAGE#{image_id} - Image under block
IMAGE#{image_id} / IMAGE#{image_id} - Image (for easy lookup)
IMAGE#{image_id} / ANNOTATION#{ann_id} - Annotation
```

---

## 🔄 Frontend TODO (doxle-web)

### Remaining Tasks:

1. **Create Unified API Client** (`src/api/`)
   - Move `home/api/*` → `src/api/`
   - Create modules: `annotations.rs`, `classes.rs`, `projects.rs`, `blocks.rs`, `images.rs`
   - Shared `client.rs` with auth token management

2. **Replace localStorage** 
   - `annotations/store.rs` → Use API for polygons/bboxes
   - `sidebar/storage.rs` → Use API for classes

3. **Update Imports**
   - Change `home::api` → `api` throughout codebase

4. **Test Auth Flow**
   - Ensure login still works with consolidated lambda

---

## 🚀 Deployment Steps:

### 1. Build Lambda:
```bash
cd /Users/doxle/Desktop/doxle-core/lambdas/doxle-api
cargo lambda build --release --arm64
```

### 2. Deploy:
```bash
cargo lambda deploy doxle-api \
  --region ap-southeast-2 \
  --env-var TABLE_NAME=doxle \
  --env-var COGNITO_CLIENT_ID=<your-client-id> \
  --env-var COGNITO_CLIENT_SECRET=<your-client-secret>
```

### 3. Enable Provisioned Concurrency (optional):
```bash
aws lambda put-provisioned-concurrency-config \
  --function-name doxle-api \
  --provisioned-concurrent-executions 1 \
  --region ap-southeast-2
```

### 4. Delete Old Lambdas:
```bash
aws lambda delete-function --function-name doxle-auth --region ap-southeast-2
aws lambda delete-function --function-name doxle-user --region ap-southeast-2
```

### 5. Update API Gateway Routes (if needed):
- Ensure API Gateway routes all paths to `doxle-api`
- Test with: `curl https://qxh6o5rw2b.execute-api.ap-southeast-2.amazonaws.com/login`

---

## 📝 Notes:

- **Backward Compatible**: Existing `/login` and `/users` routes unchanged
- **Block Terminology**: Used "blocks" instead of "tasks" throughout (task_id field kept for compatibility)
- **Class Counting**: Automatically increments/decrements when annotations are added/removed
- **Batch Operations**: `batch_create_annotations` for performance
- **Auth**: All routes except `/login` require JWT from Cognito

---

## 🎯 Cost Savings:

- **Before**: 3 separate Lambdas = 3x cold starts, 3x billing
- **After**: 1 Lambda with optional provisioned concurrency
- **Estimated**: ~$8-15/month for 1 warm instance vs unpredictable cold start costs
