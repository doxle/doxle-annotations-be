use lambda_http::{Body, Error, Response, http::StatusCode};
use aws_sdk_dynamodb::Client as DynamoClient;
use crate::types::{Annotation, CreateAnnotationRequest, UpdateAnnotationRequest, Geometry, BatchCreateAnnotationsRequest};

/// Create a new annotation for an image
pub async fn create_annotation(
    client: &DynamoClient,
    table_name: &str,
    user_id: &str,
    image_id: &str,
    project_id: &str,
    body: &[u8],
) -> Result<Response<Body>, Error> {
    let req: CreateAnnotationRequest = serde_json::from_slice(body)?;
    
    let annotation_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let pk = format!("IMAGE#{}", image_id);
    let sk = format!("ANNOTATION#{}", annotation_id);
    
    // Serialize geometry to JSON string
    let geometry_json = serde_json::to_string(&req.geometry)?;
    
    // Store annotation
    client
        .put_item()
        .table_name(table_name)
        .item("PK", aws_sdk_dynamodb::types::AttributeValue::S(pk))
        .item("SK", aws_sdk_dynamodb::types::AttributeValue::S(sk))
        .item("class_id", aws_sdk_dynamodb::types::AttributeValue::S(req.class_id.clone()))
        .item("geometry", aws_sdk_dynamodb::types::AttributeValue::S(geometry_json))
        .item("created_by", aws_sdk_dynamodb::types::AttributeValue::S(format!("USER#{}", user_id)))
        .item("created_at", aws_sdk_dynamodb::types::AttributeValue::S(now.clone()))
        .send()
        .await?;
    
    // Increment class count
    let _ = crate::classes::increment_class_count(client, table_name, project_id, &req.class_id, 1).await;
    
    let annotation = Annotation {
        annotation_id: annotation_id.clone(),
        image_id: image_id.to_string(),
        class_id: req.class_id,
        geometry: req.geometry,
        created_by: format!("USER#{}", user_id),
        created_at: now,
        updated_at: None,
    };
    
    Ok(Response::builder()
        .status(StatusCode::CREATED)
        .header("Content-Type", "application/json")
        .header("Access-Control-Allow-Origin", "*")
        .body(serde_json::to_string(&annotation)?.into())
        .map_err(Box::new)?)
}

/// Batch create annotations (for performance)
pub async fn batch_create_annotations(
    client: &DynamoClient,
    table_name: &str,
    user_id: &str,
    image_id: &str,
    project_id: &str,
    body: &[u8],
) -> Result<Response<Body>, Error> {
    let req: BatchCreateAnnotationsRequest = serde_json::from_slice(body)?;
    
    let mut annotations = Vec::new();
    let now = chrono::Utc::now().to_rfc3339();
    
    for ann_req in req.annotations {
        let annotation_id = uuid::Uuid::new_v4().to_string();
        let pk = format!("IMAGE#{}", image_id);
        let sk = format!("ANNOTATION#{}", annotation_id);
        let geometry_json = serde_json::to_string(&ann_req.geometry)?;
        
        client
            .put_item()
            .table_name(table_name)
            .item("PK", aws_sdk_dynamodb::types::AttributeValue::S(pk))
            .item("SK", aws_sdk_dynamodb::types::AttributeValue::S(sk))
            .item("class_id", aws_sdk_dynamodb::types::AttributeValue::S(ann_req.class_id.clone()))
            .item("geometry", aws_sdk_dynamodb::types::AttributeValue::S(geometry_json))
            .item("created_by", aws_sdk_dynamodb::types::AttributeValue::S(format!("USER#{}", user_id)))
            .item("created_at", aws_sdk_dynamodb::types::AttributeValue::S(now.clone()))
            .send()
            .await?;
        
        // Increment class count
        let _ = crate::classes::increment_class_count(client, table_name, project_id, &ann_req.class_id, 1).await;
        
        annotations.push(Annotation {
            annotation_id,
            image_id: image_id.to_string(),
            class_id: ann_req.class_id,
            geometry: ann_req.geometry,
            created_by: format!("USER#{}", user_id),
            created_at: now.clone(),
            updated_at: None,
        });
    }
    
    Ok(Response::builder()
        .status(StatusCode::CREATED)
        .header("Content-Type", "application/json")
        .header("Access-Control-Allow-Origin", "*")
        .body(serde_json::to_string(&annotations)?.into())
        .map_err(Box::new)?)
}

/// Get a specific annotation
pub async fn get_annotation(
    client: &DynamoClient,
    table_name: &str,
    image_id: &str,
    annotation_id: &str,
) -> Result<Response<Body>, Error> {
    let pk = format!("IMAGE#{}", image_id);
    let sk = format!("ANNOTATION#{}", annotation_id);
    
    let result = client
        .get_item()
        .table_name(table_name)
        .key("PK", aws_sdk_dynamodb::types::AttributeValue::S(pk))
        .key("SK", aws_sdk_dynamodb::types::AttributeValue::S(sk))
        .send()
        .await?;
    
    if let Some(item) = result.item() {
        let geometry_str = item.get("geometry").and_then(|v| v.as_s().ok()).map(|s| s.as_str()).unwrap_or("{}");
        let geometry: Geometry = serde_json::from_str(geometry_str).unwrap_or(Geometry::Polygon { points: vec![] });
        
        let annotation = Annotation {
            annotation_id: annotation_id.to_string(),
            image_id: image_id.to_string(),
            class_id: item.get("class_id").and_then(|v| v.as_s().ok()).map(|s| s.to_string()).unwrap_or_default(),
            geometry,
            created_by: item.get("created_by").and_then(|v| v.as_s().ok()).map(|s| s.to_string()).unwrap_or_default(),
            created_at: item.get("created_at").and_then(|v| v.as_s().ok()).map(|s| s.to_string()).unwrap_or_default(),
            updated_at: item.get("updated_at").and_then(|v| v.as_s().ok()).map(|s| s.to_string()),
        };
        
        Ok(Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "application/json")
            .header("Access-Control-Allow-Origin", "*")
            .body(serde_json::to_string(&annotation)?.into())
            .map_err(Box::new)?)
    } else {
        Ok(Response::builder()
            .status(StatusCode::NOT_FOUND)
            .header("Content-Type", "application/json")
            .header("Access-Control-Allow-Origin", "*")
            .body(serde_json::json!({"error": "Annotation not found"}).to_string().into())
            .map_err(Box::new)?)
    }
}

/// List all annotations for an image
pub async fn list_image_annotations(
    client: &DynamoClient,
    table_name: &str,
    image_id: &str,
) -> Result<Response<Body>, Error> {
    let pk = format!("IMAGE#{}", image_id);
    
    let result = client
        .query()
        .table_name(table_name)
        .key_condition_expression("PK = :pk AND begins_with(SK, :sk_prefix)")
        .expression_attribute_values(":pk", aws_sdk_dynamodb::types::AttributeValue::S(pk))
        .expression_attribute_values(":sk_prefix", aws_sdk_dynamodb::types::AttributeValue::S("ANNOTATION#".to_string()))
        .send()
        .await?;
    
    let mut annotations = Vec::new();
    
    for item in result.items() {
            if let Some(sk) = item.get("SK").and_then(|v| v.as_s().ok()) {
                if let Some(annotation_id) = sk.strip_prefix("ANNOTATION#") {
                    let geometry_str = item.get("geometry").and_then(|v| v.as_s().ok()).map(|s| s.as_str()).unwrap_or("{}");
                    let geometry: Geometry = serde_json::from_str(geometry_str).unwrap_or(Geometry::Polygon { points: vec![] });
                    
                    let annotation = Annotation {
                        annotation_id: annotation_id.to_string(),
                        image_id: image_id.to_string(),
                        class_id: item.get("class_id").and_then(|v| v.as_s().ok()).map(|s| s.to_string()).unwrap_or_default(),
                        geometry,
                        created_by: item.get("created_by").and_then(|v| v.as_s().ok()).map(|s| s.to_string()).unwrap_or_default(),
                        created_at: item.get("created_at").and_then(|v| v.as_s().ok()).map(|s| s.to_string()).unwrap_or_default(),
                        updated_at: item.get("updated_at").and_then(|v| v.as_s().ok()).map(|s| s.to_string()),
                    };
                    annotations.push(annotation);
                }
            }
    }
    
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .header("Access-Control-Allow-Origin", "*")
        .body(serde_json::to_string(&annotations)?.into())
        .map_err(Box::new)?)
}

/// Update an annotation
pub async fn update_annotation(
    client: &DynamoClient,
    table_name: &str,
    image_id: &str,
    annotation_id: &str,
    project_id: &str,
    body: &[u8],
) -> Result<Response<Body>, Error> {
    let req: UpdateAnnotationRequest = serde_json::from_slice(body)?;
    let pk = format!("IMAGE#{}", image_id);
    let sk = format!("ANNOTATION#{}", annotation_id);
    
    // Get old annotation to check class change
    let old_result = client
        .get_item()
        .table_name(table_name)
        .key("PK", aws_sdk_dynamodb::types::AttributeValue::S(pk.clone()))
        .key("SK", aws_sdk_dynamodb::types::AttributeValue::S(sk.clone()))
        .send()
        .await?;
    
    let old_class_id = old_result
        .item()
        .and_then(|item| item.get("class_id"))
        .and_then(|v| v.as_s().ok())
        .map(|s| s.to_string());
    
    let mut update_expr = vec!["#updated_at = :updated_at"];
    let mut expr_names = std::collections::HashMap::new();
    let mut expr_values = std::collections::HashMap::new();
    
    expr_names.insert("#updated_at".to_string(), "updated_at".to_string());
    expr_values.insert(":updated_at".to_string(), 
        aws_sdk_dynamodb::types::AttributeValue::S(chrono::Utc::now().to_rfc3339()));
    
    if let Some(class_id) = &req.class_id {
        update_expr.push("#class_id = :class_id");
        expr_names.insert("#class_id".to_string(), "class_id".to_string());
        expr_values.insert(":class_id".to_string(), aws_sdk_dynamodb::types::AttributeValue::S(class_id.clone()));
        
        // Update class counts if class changed
        if let Some(old_id) = &old_class_id {
            if old_id != class_id {
                let _ = crate::classes::increment_class_count(client, table_name, project_id, old_id, -1).await;
                let _ = crate::classes::increment_class_count(client, table_name, project_id, class_id, 1).await;
            }
        }
    }
    
    if let Some(geometry) = req.geometry {
        update_expr.push("#geometry = :geometry");
        expr_names.insert("#geometry".to_string(), "geometry".to_string());
        expr_values.insert(":geometry".to_string(), 
            aws_sdk_dynamodb::types::AttributeValue::S(serde_json::to_string(&geometry)?));
    }
    
    let mut builder = client
        .update_item()
        .table_name(table_name)
        .key("PK", aws_sdk_dynamodb::types::AttributeValue::S(pk))
        .key("SK", aws_sdk_dynamodb::types::AttributeValue::S(sk))
        .update_expression(format!("SET {}", update_expr.join(", ")));
    
    for (k, v) in expr_names {
        builder = builder.expression_attribute_names(k, v);
    }
    
    for (k, v) in expr_values {
        builder = builder.expression_attribute_values(k, v);
    }
    
    builder.send().await?;
    
    get_annotation(client, table_name, image_id, annotation_id).await
}

/// Delete an annotation
pub async fn delete_annotation(
    client: &DynamoClient,
    table_name: &str,
    image_id: &str,
    annotation_id: &str,
    project_id: &str,
) -> Result<Response<Body>, Error> {
    let pk = format!("IMAGE#{}", image_id);
    let sk = format!("ANNOTATION#{}", annotation_id);
    
    // Get annotation to decrement class count
    let result = client
        .get_item()
        .table_name(table_name)
        .key("PK", aws_sdk_dynamodb::types::AttributeValue::S(pk.clone()))
        .key("SK", aws_sdk_dynamodb::types::AttributeValue::S(sk.clone()))
        .send()
        .await?;
    
    if let Some(item) = result.item() {
        if let Some(class_id) = item.get("class_id").and_then(|v| v.as_s().ok()) {
            let _ = crate::classes::increment_class_count(client, table_name, project_id, class_id, -1).await;
        }
    }
    
    client
        .delete_item()
        .table_name(table_name)
        .key("PK", aws_sdk_dynamodb::types::AttributeValue::S(pk))
        .key("SK", aws_sdk_dynamodb::types::AttributeValue::S(sk))
        .send()
        .await?;
    
    Ok(Response::builder()
        .status(StatusCode::NO_CONTENT)
        .header("Access-Control-Allow-Origin", "*")
        .body(Body::Empty)
        .map_err(Box::new)?)
}
