#!/bin/bash
set -e

echo "🔐 Generating RSA key pair for CloudFront signed cookies..."

# Create keys directory if it doesn't exist
mkdir -p keys

# Generate private key (2048-bit RSA)
openssl genrsa -out keys/cloudfront-private-key.pem 2048
echo "✅ Private key generated: keys/cloudfront-private-key.pem"

# Extract public key
openssl rsa -pubout -in keys/cloudfront-private-key.pem -out keys/cloudfront-public-key.pem
echo "✅ Public key generated: keys/cloudfront-public-key.pem"

# Extract public key without headers (for CloudFormation)
openssl rsa -pubin -in keys/cloudfront-public-key.pem -outform PEM -RSAPublicKey_out | \
  grep -v "BEGIN" | grep -v "END" | tr -d '\n' > keys/cloudfront-public-key-no-headers.txt
echo "✅ Public key (no headers) for CloudFormation: keys/cloudfront-public-key-no-headers.txt"

# Show the public key for CloudFormation parameter
echo ""
echo "📋 Copy this public key for CloudFormation TrustedKeyGroupPublicKey parameter:"
echo "=================================================="
cat keys/cloudfront-public-key-no-headers.txt
echo ""
echo "=================================================="
echo ""
echo "⚠️  IMPORTANT: Store keys/cloudfront-private-key.pem securely!"
echo "   - Add it to AWS Secrets Manager or Lambda environment variables"
echo "   - DO NOT commit it to git (already in .gitignore)"
