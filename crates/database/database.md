# Doxle Database Design

Single-table DynamoDB design for the annotation platform.

## Visual Hierarchy

```
User (admin | annotator | builder)
│
├─→ Project (building | annotation) [can be locked]
    │
    ├─→ Classes (per project)
    │   └─→ name, color, properties
    │
    ├─→ Tasks [can be locked]
    │   │   states: draft → current → review → complete → paid
    │   │   assigned_to: User
    │   │
    │   └─→ Images [can be locked]
    │       │
    │       ├─→ Annotations
    │       │   └─→ class, geometry, created_by
    │       │
    │       └─→ Comments
    │           └─→ user, text, resolved
```

## User Roles
- **admin** - Full system access
- **annotator** - Works on annotation tasks
- **builder** - TBC (building projects)

## Relationships
- User ↔ Project: many-to-many
- Project → Classes: one-to-many
- Project → Tasks: one-to-many
- Task → Images: one-to-many (1 image = 1 task only)
- Image → Annotations: one-to-many
- Image → Comments: one-to-many

## DynamoDB Table Structure

### Users
```
PK: USER#123
SK: USER#123
---
email: string
role: admin | annotator | builder
created_at: timestamp
```

### User ↔ Project Access
```
PK: USER#123
SK: PROJECT#456
---
joined_at: timestamp
```

```
PK: PROJECT#456
SK: USER#123
---
joined_at: timestamp
```
(Both directions for easy queries)

### Projects
```
PK: PROJECT#456
SK: PROJECT#456
---
name: string
type: building | annotation
locked: boolean
created_at: timestamp
```

### Classes (per project)
```
PK: PROJECT#456
SK: CLASS#789
---
name: string
color: string (optional)
properties: json (optional)
```

### Tasks
```
PK: PROJECT#456
SK: TASK#abc
---
name: string
state: draft | current | review | complete | paid
locked: boolean
assigned_to: USER#123 (optional)
created_at: timestamp
```

```
PK: TASK#abc
SK: TASK#abc
---
project_id: PROJECT#456
name: string
state: draft | current | review | complete | paid
locked: boolean
assigned_to: USER#123 (optional)
created_at: timestamp
```

### Images
```
PK: TASK#abc
SK: IMAGE#001
---
url: string
locked: boolean
order: number (optional)
uploaded_at: timestamp
```

```
PK: IMAGE#001
SK: IMAGE#001
---
task_id: TASK#abc
url: string
locked: boolean
order: number (optional)
uploaded_at: timestamp
```

### Annotations
```
PK: IMAGE#001
SK: ANNOTATION#111
---
class_id: CLASS#789
geometry: json (points, polygons, etc.)
created_by: USER#123
created_at: timestamp
updated_at: timestamp (optional)
```

### Comments (on images)
```
PK: IMAGE#001
SK: COMMENT#xyz
---
user_id: USER#123
text: string
resolved: boolean
created_at: timestamp
```

## Common Query Patterns

1. **Get user profile**: `PK=USER#123, SK=USER#123`
2. **Get user's projects**: `PK=USER#123, SK begins_with PROJECT#`
3. **Get project's users**: `PK=PROJECT#456, SK begins_with USER#`
4. **Get project's classes**: `PK=PROJECT#456, SK begins_with CLASS#`
5. **Get project's tasks**: `PK=PROJECT#456, SK begins_with TASK#`
6. **Get task's images**: `PK=TASK#abc, SK begins_with IMAGE#`
7. **Get image's annotations**: `PK=IMAGE#001, SK begins_with ANNOTATION#`
8. **Get image's comments**: `PK=IMAGE#001, SK begins_with COMMENT#`

## Workflow

1. Admin creates project (type: annotation)
2. Admin adds classes to project
3. Admin creates tasks in project
4. Admin uploads images to tasks
5. Admin assigns task to annotator
6. Annotator creates annotations on images
7. Task state → "review", assigned to another annotator
8. Reviewer adds comments on images, can edit annotations
9. Task state → "complete" → "paid"

## Notes

- States are sequential but can jump if needed (flexible)
- Images belong to exactly one task
- Classes are per-project (not global)
- Project/Task/Image can all be locked independently
- Review is done via comments + direct annotation edits
