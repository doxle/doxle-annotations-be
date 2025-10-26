#!/bin/bash

# Clean DynamoDB table - delete all items except admin users
# Usage: ./clean_dynamodb.sh

TABLE_NAME="doxle-annotations"
REGION="ap-southeast-2"

echo "🔍 Scanning DynamoDB table: $TABLE_NAME"
echo "⚠️  This will DELETE all items except USER records with role='admin'"
echo ""
read -p "Are you sure? (type 'yes' to confirm): " confirm

if [ "$confirm" != "yes" ]; then
    echo "❌ Aborted"
    exit 1
fi

echo ""
echo "📊 Fetching all items..."

# Scan table and get all items
aws dynamodb scan \
    --table-name "$TABLE_NAME" \
    --region "$REGION" \
    --output json > /tmp/dynamodb_scan.json

# Count total items
TOTAL=$(jq '.Items | length' /tmp/dynamodb_scan.json)
echo "Found $TOTAL total items"

# Extract items to delete (everything except admin users)
jq -r '.Items[] | 
    select(
        (.PK.S | startswith("USER#")) and (.SK.S | startswith("USER#")) and (.role.S == "admin")
        | not
    ) | 
    "{\"PK\": {\"S\": \"\(.PK.S)\"}, \"SK\": {\"S\": \"\(.SK.S)\"}}"' \
    /tmp/dynamodb_scan.json > /tmp/items_to_delete.json

DELETE_COUNT=$(wc -l < /tmp/items_to_delete.json | tr -d ' ')
KEEP_COUNT=$((TOTAL - DELETE_COUNT))

echo "📝 Will keep: $KEEP_COUNT admin user records"
echo "🗑️  Will delete: $DELETE_COUNT items"
echo ""

if [ "$DELETE_COUNT" -eq 0 ]; then
    echo "✅ Nothing to delete!"
    exit 0
fi

echo "🔄 Starting deletion in batches of 25..."

# Process in batches of 25 (DynamoDB batch limit)
split -l 25 /tmp/items_to_delete.json /tmp/batch_

BATCH_NUM=0
for batch_file in /tmp/batch_*; do
    BATCH_NUM=$((BATCH_NUM + 1))
    
    # Build batch delete request
    echo "[" > /tmp/delete_batch.json
    first=true
    while IFS= read -r item; do
        if [ "$first" = true ]; then
            first=false
        else
            echo "," >> /tmp/delete_batch.json
        fi
        echo "{\"DeleteRequest\": {\"Key\": $item}}" >> /tmp/delete_batch.json
    done < "$batch_file"
    echo "]" >> /tmp/delete_batch.json
    
    # Execute batch delete
    echo "  Batch $BATCH_NUM..."
    aws dynamodb batch-write-item \
        --region "$REGION" \
        --request-items "{\"$TABLE_NAME\": $(cat /tmp/delete_batch.json)}" \
        --output json > /tmp/delete_result.json
    
    # Check for unprocessed items
    UNPROCESSED=$(jq -r ".UnprocessedItems | length" /tmp/delete_result.json)
    if [ "$UNPROCESSED" != "0" ]; then
        echo "    ⚠️  Some items unprocessed, retrying..."
        sleep 1
        aws dynamodb batch-write-item \
            --region "$REGION" \
            --request-items "$(jq -c .UnprocessedItems /tmp/delete_result.json)" \
            --output json > /dev/null
    fi
done

# Cleanup temp files
rm -f /tmp/batch_* /tmp/delete_batch.json /tmp/delete_result.json /tmp/items_to_delete.json /tmp/dynamodb_scan.json

echo ""
echo "✅ Cleanup complete!"
echo "🔍 Verifying..."

# Final count
FINAL_COUNT=$(aws dynamodb scan --table-name "$TABLE_NAME" --region "$REGION" --select COUNT --output json | jq -r '.Count')
echo "📊 Remaining items: $FINAL_COUNT (should be $KEEP_COUNT admin users)"
