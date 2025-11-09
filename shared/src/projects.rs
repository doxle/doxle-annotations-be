async fn delete_project_s3_prefix(s3_client: &S3Client, project_id: &str) -> Result<(), Error> {
    const BUCKET_NAME: &str = "doxle-annotations";
    let prefix = format!("projects/{}/", project_id);

    let mut continuation: Option<String> = None;
    loop {
        let mut req = s3_client
            .list_objects_v2()
            .bucket(BUCKET_NAME)
            .prefix(&prefix);
        if let Some(token) = continuation.as_ref() {
            req = req.continuation_token(token);
        }
        let resp = req.send().await.map_err(|e| {
            tracing::error!("S3 list_objects_v2 failed for prefix {}: {}", prefix, e);
            format!("S3 list failed: {}", e)
        })?;

        let contents = resp.contents();
        let objects: Vec<_> = contents
            .iter()
            .filter_map(|o| o.key())
            .filter_map(|k| {
                aws_sdk_s3::types::ObjectIdentifier::builder()
                    .key(k)
                    .build()
                    .ok()
            })
            .collect();
        if objects.is_empty() {
            if resp.is_truncated().unwrap_or(false) {
                continuation = resp.next_continuation_token().map(|s| s.to_string());
                continue;
            } else {
                break;
            }
        }

        let delete_payload = aws_sdk_s3::types::Delete::builder()
            .set_objects(Some(objects))
            .build()
            .map_err(|e| format!("Failed to build S3 delete payload: {:?}", e))?;

        let _ = s3_client
            .delete_objects()
            .bucket(BUCKET_NAME)
            .delete(delete_payload)
            .send()
            .await;

        if resp.is_truncated().unwrap_or(false) {
            continuation = resp.next_continuation_token().map(|s| s.to_string());
        } else {
            break;
        }
    }
    Ok(())
}

use crate::types::{CreateProjectRequest, Project, UpdateProjectRequest};
use aws_sdk_dynamodb::Client as DynamoClient;
use aws_sdk_s3::Client as S3Client;
use lambda_http::{http::StatusCode, Body, Error, Response};

/// Create a new project
pub async fn create_project(
    client: &DynamoClient,
    table_name: &str,
    user_id: &str,
    body: &[u8],
) -> Result<Response<Body>, Error> {
    let start = std::time::Instant::now();
    let raw_body = String::from_utf8_lossy(body);
    tracing::info!("[CREATE] Raw body: {}", raw_body);

    let req: CreateProjectRequest = match serde_json::from_slice(body) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!("[CREATE] Parse error: {} | body: {}", e, raw_body);
            return Ok(Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .header("Content-Type", "application/json")
                .header("Access-Control-Allow-Origin", "*")
                .body(
                    serde_json::json!({
                        "error": format!("Invalid request body: {}", e)
                    })
                    .to_string()
                    .into(),
                )
                .map_err(Box::new)?);
        }
    };

    //Validate that at least one label exists
    if req.labels.is_empty() {
        return Ok(Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .header("Content-Type", "application/json")
            .header("Access-Control-Allow-Origin", "*")
            .body(
                serde_json::json!({
                    "error": "Project must have at least one label"
                })
                .to_string()
                .into(),
            )
            .map_err(Box::new)?);
    }

    let project_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let pk = format!("PROJECT#{}", project_id);

    println!("[CREATE] Starting project creation: {}", project_id);

    // Prepare all 3 items to write in a single batch
    let user_pk = format!("USER#{}", user_id);
    let project_sk = format!("PROJECT#{}", project_id);

    // Build the 3 items
    use std::collections::HashMap;

    // 1. Project record
    let mut project_item = HashMap::new();
    project_item.insert(
        "PK".to_string(),
        aws_sdk_dynamodb::types::AttributeValue::S(pk.clone()),
    );
    project_item.insert(
        "SK".to_string(),
        aws_sdk_dynamodb::types::AttributeValue::S(pk.clone()),
    );
    project_item.insert(
        "name".to_string(),
        aws_sdk_dynamodb::types::AttributeValue::S(req.name.clone()),
    );
    project_item.insert(
        "project_type".to_string(),
        aws_sdk_dynamodb::types::AttributeValue::S(req.project_type.clone()),
    );
    project_item.insert(
        "locked".to_string(),
        aws_sdk_dynamodb::types::AttributeValue::Bool(false),
    );
    project_item.insert(
        "labels".to_string(),
        aws_sdk_dynamodb::types::AttributeValue::S(serde_json::to_string(&req.labels)?),
    );
    project_item.insert(
        "created_at".to_string(),
        aws_sdk_dynamodb::types::AttributeValue::S(now.clone()),
    );

    // 2. USER -> PROJECT link
    let mut user_to_project = HashMap::new();
    user_to_project.insert(
        "PK".to_string(),
        aws_sdk_dynamodb::types::AttributeValue::S(user_pk.clone()),
    );
    user_to_project.insert(
        "SK".to_string(),
        aws_sdk_dynamodb::types::AttributeValue::S(project_sk.clone()),
    );
    user_to_project.insert(
        "joined_at".to_string(),
        aws_sdk_dynamodb::types::AttributeValue::S(now.clone()),
    );

    // 3. PROJECT -> USER link
    let mut project_to_user = HashMap::new();
    project_to_user.insert(
        "PK".to_string(),
        aws_sdk_dynamodb::types::AttributeValue::S(project_sk.clone()),
    );
    project_to_user.insert(
        "SK".to_string(),
        aws_sdk_dynamodb::types::AttributeValue::S(user_pk),
    );
    project_to_user.insert(
        "joined_at".to_string(),
        aws_sdk_dynamodb::types::AttributeValue::S(now.clone()),
    );

    // Write all 3 items in a single batch operation
    client
        .batch_write_item()
        .request_items(
            table_name,
            vec![
                aws_sdk_dynamodb::types::WriteRequest::builder()
                    .put_request(
                        aws_sdk_dynamodb::types::PutRequest::builder()
                            .set_item(Some(project_item))
                            .build()
                            .unwrap(),
                    )
                    .build(),
                aws_sdk_dynamodb::types::WriteRequest::builder()
                    .put_request(
                        aws_sdk_dynamodb::types::PutRequest::builder()
                            .set_item(Some(user_to_project))
                            .build()
                            .unwrap(),
                    )
                    .build(),
                aws_sdk_dynamodb::types::WriteRequest::builder()
                    .put_request(
                        aws_sdk_dynamodb::types::PutRequest::builder()
                            .set_item(Some(project_to_user))
                            .build()
                            .unwrap(),
                    )
                    .build(),
            ],
        )
        .send()
        .await?;

    println!(
        "[CREATE] Batch write complete: {}ms",
        start.elapsed().as_millis()
    );

    let project = Project {
        project_id: project_id.clone(),
        name: req.name,
        project_type: req.project_type,
        locked: false,
        labels: req.labels,
        created_at: now,
    };

    Ok(Response::builder()
        .status(StatusCode::CREATED)
        .header("Content-Type", "application/json")
        .header("Access-Control-Allow-Origin", "*")
        .body(serde_json::to_string(&project)?.into())
        .map_err(Box::new)?)
}

/// Get a specific project
pub async fn get_project(
    client: &DynamoClient,
    table_name: &str,
    project_id: &str,
) -> Result<Response<Body>, Error> {
    let pk = format!("PROJECT#{}", project_id);

    let result = client
        .get_item()
        .table_name(table_name)
        .key("PK", aws_sdk_dynamodb::types::AttributeValue::S(pk.clone()))
        .key("SK", aws_sdk_dynamodb::types::AttributeValue::S(pk))
        .send()
        .await?;

    if let Some(item) = result.item() {
        let project = Project {
            project_id: project_id.to_string(),
            name: item
                .get("name")
                .and_then(|v| v.as_s().ok())
                .map(|s| s.to_string())
                .unwrap_or_default(),
            project_type: item
                .get("project_type")
                .and_then(|v| v.as_s().ok())
                .map(|s| s.to_string())
                .unwrap_or_default(),
            locked: item
                .get("locked")
                .and_then(|v| v.as_bool().ok())
                .copied()
                .unwrap_or(false),
            labels: item
                .get("labels")
                .and_then(|v| v.as_s().ok())
                .map(|s| serde_json::from_str(s).unwrap_or_default())
                .unwrap_or_default(),
            created_at: item
                .get("created_at")
                .and_then(|v| v.as_s().ok())
                .map(|s| s.to_string())
                .unwrap_or_default(),
        };

        Ok(Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "application/json")
            .header("Access-Control-Allow-Origin", "*")
            .body(serde_json::to_string(&project)?.into())
            .map_err(Box::new)?)
    } else {
        Ok(Response::builder()
            .status(StatusCode::NOT_FOUND)
            .header("Content-Type", "application/json")
            .header("Access-Control-Allow-Origin", "*")
            .body(
                serde_json::json!({"error": "Project not found"})
                    .to_string()
                    .into(),
            )
            .map_err(Box::new)?)
    }
}

/// List all projects for a user
pub async fn list_user_projects(
    client: &DynamoClient,
    table_name: &str,
    user_id: &str,
) -> Result<Response<Body>, Error> {
    let pk = format!("USER#{}", user_id);

    let result = client
        .query()
        .table_name(table_name)
        .key_condition_expression("PK = :pk AND begins_with(SK, :sk_prefix)")
        .expression_attribute_values(":pk", aws_sdk_dynamodb::types::AttributeValue::S(pk))
        .expression_attribute_values(
            ":sk_prefix",
            aws_sdk_dynamodb::types::AttributeValue::S("PROJECT#".to_string()),
        )
        .send()
        .await?;

    let mut projects = Vec::new();
    let mut project_ids = Vec::new();

    // Collect all project IDs
    for item in result.items() {
        if let Some(sk) = item.get("SK").and_then(|v| v.as_s().ok()) {
            if let Some(project_id) = sk.strip_prefix("PROJECT#") {
                project_ids.push(project_id.to_string());
            }
        }
    }

    // If no projects, return empty list
    if project_ids.is_empty() {
        return Ok(Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "application/json")
            .header("Access-Control-Allow-Origin", "*")
            .body(serde_json::to_string(&projects)?.into())
            .map_err(Box::new)?);
    }

    // Batch fetch all projects (DynamoDB allows up to 100 items per batch)
    for chunk in project_ids.chunks(100) {
        let mut keys = Vec::new();
        for project_id in chunk {
            let pk = format!("PROJECT#{}", project_id);
            let mut key = std::collections::HashMap::new();
            key.insert(
                "PK".to_string(),
                aws_sdk_dynamodb::types::AttributeValue::S(pk.clone()),
            );
            key.insert(
                "SK".to_string(),
                aws_sdk_dynamodb::types::AttributeValue::S(pk),
            );
            keys.push(key);
        }

        let batch_result = client
            .batch_get_item()
            .request_items(
                table_name,
                aws_sdk_dynamodb::types::KeysAndAttributes::builder()
                    .set_keys(Some(keys))
                    .build()
                    .unwrap(),
            )
            .send()
            .await?;

        if let Some(responses) = batch_result.responses() {
            if let Some(items) = responses.get(table_name) {
                for item in items {
                    if let Some(project_id_attr) = item.get("PK").and_then(|v| v.as_s().ok()) {
                        if let Some(project_id) = project_id_attr.strip_prefix("PROJECT#") {
                            let project = Project {
                                project_id: project_id.to_string(),
                                name: item
                                    .get("name")
                                    .and_then(|v| v.as_s().ok())
                                    .map(|s| s.to_string())
                                    .unwrap_or_default(),
                                project_type: item
                                    .get("project_type")
                                    .and_then(|v| v.as_s().ok())
                                    .map(|s| s.to_string())
                                    .unwrap_or_default(),
                                locked: item
                                    .get("locked")
                                    .and_then(|v| v.as_bool().ok())
                                    .copied()
                                    .unwrap_or(false),
                                labels: item
                                    .get("labels")
                                    .and_then(|v| v.as_s().ok())
                                    .map(|s| serde_json::from_str(s).unwrap_or_default())
                                    .unwrap_or_default(),
                                created_at: item
                                    .get("created_at")
                                    .and_then(|v| v.as_s().ok())
                                    .map(|s| s.to_string())
                                    .unwrap_or_default(),
                            };
                            projects.push(project);
                        }
                    }
                }
            }
        }
    }

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .header("Access-Control-Allow-Origin", "*")
        .body(serde_json::to_string(&projects)?.into())
        .map_err(Box::new)?)
}

/// Update a project
pub async fn update_project(
    client: &DynamoClient,
    table_name: &str,
    project_id: &str,
    body: &[u8],
) -> Result<Response<Body>, Error> {
    println!("[UPDATE] Project: {}", project_id);
    let req: UpdateProjectRequest = serde_json::from_slice(body)?;
    let pk = format!("PROJECT#{}", project_id);

    let mut update_expr = vec![];
    let mut expr_names = std::collections::HashMap::new();
    let mut expr_values = std::collections::HashMap::new();

    if let Some(name) = req.name {
        update_expr.push("#name = :name");
        expr_names.insert("#name".to_string(), "name".to_string());
        expr_values.insert(
            ":name".to_string(),
            aws_sdk_dynamodb::types::AttributeValue::S(name),
        );
    }

    if let Some(locked) = req.locked {
        update_expr.push("#locked = :locked");
        expr_names.insert("#locked".to_string(), "locked".to_string());
        expr_values.insert(
            ":locked".to_string(),
            aws_sdk_dynamodb::types::AttributeValue::Bool(locked),
        );
    }

    if !update_expr.is_empty() {
        let mut builder = client
            .update_item()
            .table_name(table_name)
            .key("PK", aws_sdk_dynamodb::types::AttributeValue::S(pk.clone()))
            .key("SK", aws_sdk_dynamodb::types::AttributeValue::S(pk))
            .update_expression(format!("SET {}", update_expr.join(", ")));

        for (k, v) in expr_names {
            builder = builder.expression_attribute_names(k, v);
        }

        for (k, v) in expr_values {
            builder = builder.expression_attribute_values(k, v);
        }

        builder.send().await?;
        println!("[UPDATE] Success: {}", project_id);
    }

    get_project(client, table_name, project_id).await
}

/// Delete a project and all associated resources (blocks, images, annotations, classes)
pub async fn delete_project(
    client: &DynamoClient,
    s3_client: &S3Client,
    table_name: &str,
    project_id: &str,
    user_id: &str,
) -> Result<Response<Body>, Error> {
    let start = std::time::Instant::now();
    println!("[DELETE] Project: {} - Starting cascade delete", project_id);

    let pk = format!("PROJECT#{}", project_id);
    let user_pk = format!("USER#{}", user_id);
    let project_sk = format!("PROJECT#{}", project_id);

    use std::collections::HashMap;

    // Step 1: Query all blocks for this project
    println!("[DELETE] Step 1: Querying blocks...");
    let blocks_result = client
        .query()
        .table_name(table_name)
        .key_condition_expression("PK = :pk AND begins_with(SK, :sk_prefix)")
        .expression_attribute_values(
            ":pk",
            aws_sdk_dynamodb::types::AttributeValue::S(pk.clone()),
        )
        .expression_attribute_values(
            ":sk_prefix",
            aws_sdk_dynamodb::types::AttributeValue::S("BLOCK#".to_string()),
        )
        .send()
        .await?;

    let mut block_ids = Vec::new();
    for item in blocks_result.items() {
        if let Some(sk) = item.get("SK").and_then(|v| v.as_s().ok()) {
            if let Some(block_id) = sk.strip_prefix("BLOCK#") {
                block_ids.push(block_id.to_string());
            }
        }
    }
    println!("[DELETE] Found {} blocks to delete", block_ids.len());

    // Step 2: For each block, query images and annotations
    let mut all_delete_keys = Vec::new();

    for block_id in &block_ids {
        let block_pk = format!("BLOCK#{}", block_id);

        // Query images for this block
        let images_result = client
            .query()
            .table_name(table_name)
            .key_condition_expression("PK = :pk AND begins_with(SK, :sk_prefix)")
            .expression_attribute_values(
                ":pk",
                aws_sdk_dynamodb::types::AttributeValue::S(block_pk.clone()),
            )
            .expression_attribute_values(
                ":sk_prefix",
                aws_sdk_dynamodb::types::AttributeValue::S("IMAGE#".to_string()),
            )
            .send()
            .await?;

        let mut image_ids = Vec::new();
        for item in images_result.items() {
            if let Some(sk) = item.get("SK").and_then(|v| v.as_s().ok()) {
                if let Some(image_id) = sk.strip_prefix("IMAGE#") {
                    image_ids.push(image_id.to_string());
                    // Add BLOCK# -> IMAGE# record to delete
                    let mut key = HashMap::new();
                    key.insert(
                        "PK".to_string(),
                        aws_sdk_dynamodb::types::AttributeValue::S(block_pk.clone()),
                    );
                    key.insert(
                        "SK".to_string(),
                        aws_sdk_dynamodb::types::AttributeValue::S(sk.to_string()),
                    );
                    all_delete_keys.push(key);
                }
            }
        }

        // For each image, query annotations
        for image_id in &image_ids {
            let image_pk = format!("IMAGE#{}", image_id);

            let annotations_result = client
                .query()
                .table_name(table_name)
                .key_condition_expression("PK = :pk AND begins_with(SK, :sk_prefix)")
                .expression_attribute_values(
                    ":pk",
                    aws_sdk_dynamodb::types::AttributeValue::S(image_pk.clone()),
                )
                .expression_attribute_values(
                    ":sk_prefix",
                    aws_sdk_dynamodb::types::AttributeValue::S("ANNOTATION#".to_string()),
                )
                .send()
                .await?;

            for item in annotations_result.items() {
                if let Some(sk) = item.get("SK").and_then(|v| v.as_s().ok()) {
                    // Add IMAGE# -> ANNOTATION# record to delete
                    let mut key = HashMap::new();
                    key.insert(
                        "PK".to_string(),
                        aws_sdk_dynamodb::types::AttributeValue::S(image_pk.clone()),
                    );
                    key.insert(
                        "SK".to_string(),
                        aws_sdk_dynamodb::types::AttributeValue::S(sk.to_string()),
                    );
                    all_delete_keys.push(key);
                }
            }

            // Add IMAGE# -> IMAGE# record to delete
            let mut key = HashMap::new();
            key.insert(
                "PK".to_string(),
                aws_sdk_dynamodb::types::AttributeValue::S(image_pk.clone()),
            );
            key.insert(
                "SK".to_string(),
                aws_sdk_dynamodb::types::AttributeValue::S(image_pk),
            );
            all_delete_keys.push(key);
        }

        // Add PROJECT# -> BLOCK# record to delete
        let mut key = HashMap::new();
        key.insert(
            "PK".to_string(),
            aws_sdk_dynamodb::types::AttributeValue::S(pk.clone()),
        );
        key.insert(
            "SK".to_string(),
            aws_sdk_dynamodb::types::AttributeValue::S(block_pk.clone()),
        );
        all_delete_keys.push(key);

        // Add BLOCK# -> BLOCK# record to delete
        let mut key = HashMap::new();
        key.insert(
            "PK".to_string(),
            aws_sdk_dynamodb::types::AttributeValue::S(block_pk.clone()),
        );
        key.insert(
            "SK".to_string(),
            aws_sdk_dynamodb::types::AttributeValue::S(block_pk),
        );
        all_delete_keys.push(key);
    }

    // Step 3: Query and add classes to delete
    println!("[DELETE] Step 3: Querying classes...");
    let classes_result = client
        .query()
        .table_name(table_name)
        .key_condition_expression("PK = :pk AND begins_with(SK, :sk_prefix)")
        .expression_attribute_values(
            ":pk",
            aws_sdk_dynamodb::types::AttributeValue::S(pk.clone()),
        )
        .expression_attribute_values(
            ":sk_prefix",
            aws_sdk_dynamodb::types::AttributeValue::S("CLASS#".to_string()),
        )
        .send()
        .await?;

    for item in classes_result.items() {
        if let Some(sk) = item.get("SK").and_then(|v| v.as_s().ok()) {
            let mut key = HashMap::new();
            key.insert(
                "PK".to_string(),
                aws_sdk_dynamodb::types::AttributeValue::S(pk.clone()),
            );
            key.insert(
                "SK".to_string(),
                aws_sdk_dynamodb::types::AttributeValue::S(sk.to_string()),
            );
            all_delete_keys.push(key);
        }
    }

    // Step 4: Add project and link records to delete
    // 1. Project record key
    let mut project_key = HashMap::new();
    project_key.insert(
        "PK".to_string(),
        aws_sdk_dynamodb::types::AttributeValue::S(pk.clone()),
    );
    project_key.insert(
        "SK".to_string(),
        aws_sdk_dynamodb::types::AttributeValue::S(pk.clone()),
    );
    all_delete_keys.push(project_key);

    // 2. USER -> PROJECT link key
    let mut user_to_project_key = HashMap::new();
    user_to_project_key.insert(
        "PK".to_string(),
        aws_sdk_dynamodb::types::AttributeValue::S(user_pk.clone()),
    );
    user_to_project_key.insert(
        "SK".to_string(),
        aws_sdk_dynamodb::types::AttributeValue::S(project_sk.clone()),
    );
    all_delete_keys.push(user_to_project_key);

    // 3. PROJECT -> USER link key
    let mut project_to_user_key = HashMap::new();
    project_to_user_key.insert(
        "PK".to_string(),
        aws_sdk_dynamodb::types::AttributeValue::S(project_sk),
    );
    project_to_user_key.insert(
        "SK".to_string(),
        aws_sdk_dynamodb::types::AttributeValue::S(user_pk),
    );
    all_delete_keys.push(project_to_user_key);

    println!(
        "[DELETE] Total records to delete: {}",
        all_delete_keys.len()
    );

    // Step 5: Batch delete all records (DynamoDB allows max 25 items per batch)
    let batch_start = std::time::Instant::now();
    for chunk in all_delete_keys.chunks(25) {
        let delete_requests: Vec<_> = chunk
            .iter()
            .map(|key| {
                aws_sdk_dynamodb::types::WriteRequest::builder()
                    .delete_request(
                        aws_sdk_dynamodb::types::DeleteRequest::builder()
                            .set_key(Some(key.clone()))
                            .build()
                            .unwrap(),
                    )
                    .build()
            })
            .collect();

        let mut attempts = 0;
        let mut unprocessed = Some(delete_requests);

        while let Some(requests) = unprocessed {
            attempts += 1;
            if attempts > 5 {
                println!(
                    "[DELETE] Warning: Max retry attempts reached, {} items may not be deleted",
                    requests.len()
                );
                break;
            }

            let result = client
                .batch_write_item()
                .request_items(table_name, requests)
                .send()
                .await?;

            unprocessed = result
                .unprocessed_items()
                .and_then(|items| items.get(table_name))
                .map(|items| items.clone());

            if unprocessed.is_some() {
                println!(
                    "[DELETE] Retrying {} unprocessed items (attempt {})",
                    unprocessed.as_ref().unwrap().len(),
                    attempts
                );
                tokio::time::sleep(tokio::time::Duration::from_millis(100 * attempts as u64)).await;
            }
        }
    }

    let batch_time = batch_start.elapsed();
    let total_time = start.elapsed();
    println!(
        "[DELETE] Cascade delete complete: {} records (batch: {:?}, total: {:?})",
        all_delete_keys.len(),
        batch_time,
        total_time
    );

    // Step 6: Delete S3 objects under project prefix: projects/{project_id}/
    delete_project_s3_prefix(s3_client, project_id).await.ok();

    Ok(Response::builder()
        .status(StatusCode::NO_CONTENT)
        .header("Access-Control-Allow-Origin", "*")
        .body(Body::Empty)
        .map_err(Box::new)?)
}
