#!/bin/bash

# Deploy monitoring stack for critical infrastructure changes

STACK_NAME="doxle-monitoring"
TEMPLATE_FILE="monitoring-stack.yaml"
EMAIL="sel@doxle.com"

echo "🚀 Deploying monitoring stack..."
echo "Email alerts will be sent to: $EMAIL"

aws cloudformation deploy \
  --stack-name $STACK_NAME \
  --template-file $TEMPLATE_FILE \
  --parameter-overrides AlertEmail=$EMAIL \
  --capabilities CAPABILITY_IAM \
  --region ap-southeast-2

if [ $? -eq 0 ]; then
  echo "✅ Monitoring stack deployed successfully!"
  echo ""
  echo "⚠️  IMPORTANT: Check your email ($EMAIL) and confirm the SNS subscription!"
  echo ""
  echo "📊 Monitoring enabled for:"
  echo "  • S3 bucket policy changes (doxle-annotations)"
  echo "  • Lambda config changes (doxle-annotations-api)"
  echo "  • S3 upload 403 errors"
else
  echo "❌ Deployment failed"
  exit 1
fi
