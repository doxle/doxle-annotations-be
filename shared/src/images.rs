use crate::types::{CreateImageRequest, Image, UpdateImageRequest};
use aws_sdk_dynamodb::types::AttributeValue;
use aws_sdk_dynamodb::Client as DynamoClient;
use lambda_http::{http::StatusCode, Body, Error, Response};

/// Create a new image in a block
pub async fn create_image(
    client: &DynamoClient,
    table_name: &str,
    block_id: &str,
    body: &[u8],
) -> Result<Response<Body>, Error> {
    let req: CreateImageRequest = serde_json::from_slice(body)?;

    let image_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let pk = format!("BLOCK#{}", block_id);
    let sk = format!("IMAGE#{}", image_id);

    // Store image under block
    let mut builder = client
        .put_item()
        .table_name(table_name)
        .item("PK", AttributeValue::S(pk.clone()))
        .item("SK", AttributeValue::S(sk.clone()))
        .item("url", AttributeValue::S(req.url.clone()))
        .item("locked", AttributeValue::Bool(false))
        .item(
            "uploaded_at",
            aws_sdk_dynamodb::types::AttributeValue::S(now.clone()),
        );

    if let Some(order) = req.order {
        builder = builder.item("order", AttributeValue::N(order.to_string()));
    }

    builder.send().await?;

    let image = Image {
        image_id: image_id.clone(),
        block_id: block_id.to_string(),
        url: req.url,
        locked: false,
        order: req.order,
        uploaded_at: now,
    };

    Ok(Response::builder()
        .status(StatusCode::CREATED)
        .header("Content-Type", "application/json")
        .header("Access-Control-Allow-Origin", "*")
        .body(serde_json::to_string(&image)?.into())
        .map_err(Box::new)?)
}

/// Get a specific image
pub async fn get_image(
    client: &DynamoClient,
    table_name: &str,
    block_id: &str,
    image_id: &str,
) -> Result<Response<Body>, Error> {
    let pk = format!("BLOCK#{}", block_id);
    let sk = format!("IMAGE#{}", image_id);

    let result = client
        .get_item()
        .table_name(table_name)
        .key("PK", aws_sdk_dynamodb::types::AttributeValue::S(pk.clone()))
        .key("SK", aws_sdk_dynamodb::types::AttributeValue::S(sk))
        .send()
        .await?;

    if let Some(item) = result.item() {
        let image = Image {
            image_id: image_id.to_string(),
            block_id: block_id.to_string(),
            url: item
                .get("url")
                .and_then(|v| v.as_s().ok())
                .map(|s| s.to_string())
                .unwrap_or_default(),
            locked: item
                .get("locked")
                .and_then(|v| v.as_bool().ok())
                .copied()
                .unwrap_or(false),
            order: item
                .get("order")
                .and_then(|v| v.as_n().ok())
                .and_then(|n| n.parse().ok()),
            uploaded_at: item
                .get("uploaded_at")
                .and_then(|v| v.as_s().ok())
                .map(|s| s.to_string())
                .unwrap_or_default(),
        };

        Ok(Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "application/json")
            .header("Access-Control-Allow-Origin", "*")
            .body(serde_json::to_string(&image)?.into())
            .map_err(Box::new)?)
    } else {
        Ok(Response::builder()
            .status(StatusCode::NOT_FOUND)
            .header("Content-Type", "application/json")
            .header("Access-Control-Allow-Origin", "*")
            .body(
                serde_json::json!({"error": "Image not found"})
                    .to_string()
                    .into(),
            )
            .map_err(Box::new)?)
    }
}

/// List all images for a block
pub async fn list_block_images(
    client: &DynamoClient,
    table_name: &str,
    block_id: &str,
) -> Result<Response<Body>, Error> {
    let pk = format!("BLOCK#{}", block_id);

    let result = client
        .query()
        .table_name(table_name)
        .key_condition_expression("PK = :pk AND begins_with(SK, :sk_prefix)")
        .expression_attribute_values(":pk", AttributeValue::S(pk))
        .expression_attribute_values(":sk_prefix", AttributeValue::S("IMAGE#".to_string()))
        .send()
        .await?;

    let mut images = Vec::new();

    for item in result.items() {
        if let Some(sk) = item.get("SK").and_then(|v| v.as_s().ok()) {
            if let Some(image_id) = sk.strip_prefix("IMAGE#") {
                let image = Image {
                    image_id: image_id.to_string(),
                    block_id: block_id.to_string(),
                    url: item
                        .get("url")
                        .and_then(|v| v.as_s().ok())
                        .map(|s| s.to_string())
                        .unwrap_or_default(),
                    locked: item
                        .get("locked")
                        .and_then(|v| v.as_bool().ok())
                        .copied()
                        .unwrap_or(false),
                    order: item
                        .get("order")
                        .and_then(|v| v.as_n().ok())
                        .and_then(|n| n.parse().ok()),
                    uploaded_at: item
                        .get("uploaded_at")
                        .and_then(|v| v.as_s().ok())
                        .map(|s| s.to_string())
                        .unwrap_or_default(),
                };
                images.push(image);
            }
        }
    }

    // Sort by order
    images.sort_by(|a, b| match (a.order, b.order) {
        (Some(a_order), Some(b_order)) => a_order.cmp(&b_order),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    });

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .header("Access-Control-Allow-Origin", "*")
        .body(serde_json::to_string(&images)?.into())
        .map_err(Box::new)?)
}

/// Update an image
pub async fn update_image(
    client: &DynamoClient,
    table_name: &str,
    block_id: &str,
    image_id: &str,
    body: &[u8],
) -> Result<Response<Body>, Error> {
    let req: UpdateImageRequest = serde_json::from_slice(body)?;
    let pk = format!("BLOCK#{}", block_id);
    let sk = format!("IMAGE#{}", image_id);

    let mut update_expr = vec![];
    let mut expr_names = std::collections::HashMap::new();
    let mut expr_values = std::collections::HashMap::new();

    if let Some(locked) = req.locked {
        update_expr.push("#locked = :locked");
        expr_names.insert("#locked".to_string(), "locked".to_string());
        expr_values.insert(":locked".to_string(), AttributeValue::Bool(locked));
    }

    if let Some(order) = req.order {
        update_expr.push("#order = :order");
        expr_names.insert("#order".to_string(), "order".to_string());
        expr_values.insert(
            ":order".to_string(),
            aws_sdk_dynamodb::types::AttributeValue::N(order.to_string()),
        );
    }

    if !update_expr.is_empty() {
        let update_expression = format!("SET {}", update_expr.join(", "));

        let mut builder = client
            .update_item()
            .table_name(table_name)
            .key("PK", AttributeValue::S(pk))
            .key("SK", AttributeValue::S(sk))
            .update_expression(&update_expression);

        for (k, v) in expr_names {
            builder = builder.expression_attribute_names(k, v);
        }

        for (k, v) in expr_values {
            builder = builder.expression_attribute_values(k, v);
        }

        builder.send().await?;
    }

    get_image(client, table_name, block_id, image_id).await
}

/// Delete an image
pub async fn delete_image(
    client: &DynamoClient,
    table_name: &str,
    block_id: &str,
    image_id: &str,
) -> Result<Response<Body>, Error> {
    let pk = format!("BLOCK#{}", block_id);
    let sk = format!("IMAGE#{}", image_id);

    // Delete BLOCK#→IMAGE# row
    client
        .delete_item()
        .table_name(table_name)
        .key("PK", AttributeValue::S(pk))
        .key("SK", AttributeValue::S(sk))
        .send()
        .await?;

    Ok(Response::builder()
        .status(StatusCode::NO_CONTENT)
        .header("Access-Control-Allow-Origin", "*")
        .body(Body::Empty)
        .map_err(Box::new)?)
}
