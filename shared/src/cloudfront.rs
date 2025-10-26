use lambda_http::{Body, Error, Response, http::StatusCode};
use rsa::{RsaPrivateKey, pkcs1v15::SigningKey, signature::SignatureEncoding, signature::Signer};
use rsa::pkcs8::DecodePrivateKey;
use rsa::pkcs1::DecodeRsaPrivateKey;
use sha2::Sha256;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use std::time::{SystemTime, UNIX_EPOCH};

const CLOUDFRONT_DOMAIN: &str = "CLOUDFRONT_DOMAIN"; // Set via env var
const CLOUDFRONT_KEY_PAIR_ID: &str = "CLOUDFRONT_KEY_PAIR_ID"; // Set via env var
const CLOUDFRONT_PRIVATE_KEY: &str = "CLOUDFRONT_PRIVATE_KEY"; // Set via env var (PEM format)
const CLOUDFRONT_COOKIE_DOMAIN: &str = "CLOUDFRONT_COOKIE_DOMAIN"; // Optional explicit cookie domain

#[derive(serde::Serialize)]
struct CloudFrontPolicy {
    #[serde(rename = "Statement")]
    statement: Vec<PolicyStatement>,
}

#[derive(serde::Serialize)]
struct PolicyStatement {
    #[serde(rename = "Resource")]
    resource: String,
    #[serde(rename = "Condition")]
    condition: PolicyCondition,
}

#[derive(serde::Serialize)]
struct PolicyCondition {
    #[serde(rename = "DateLessThan")]
    date_less_than: DateLessThan,
}

#[derive(serde::Serialize)]
struct DateLessThan {
    #[serde(rename = "AWS:EpochTime")]
    aws_epoch_time: i64,
}

/// Generate CloudFront signed cookies for the user session
pub fn generate_signed_cookies(
    duration_seconds: i64,
) -> Result<Vec<(String, String)>, Box<dyn std::error::Error>> {
    let domain = std::env::var(CLOUDFRONT_DOMAIN)
        .map_err(|_| "CLOUDFRONT_DOMAIN not set")?;
    let key_pair_id = std::env::var(CLOUDFRONT_KEY_PAIR_ID)
        .map_err(|_| "CLOUDFRONT_KEY_PAIR_ID not set")?;
    let private_key_pem = std::env::var(CLOUDFRONT_PRIVATE_KEY)
        .map_err(|_| "CLOUDFRONT_PRIVATE_KEY not set")?;
    
    // Calculate expiration time
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_secs() as i64;
    let expiration = now + duration_seconds;
    
    // Build policy
    let policy = CloudFrontPolicy {
        statement: vec![PolicyStatement {
            resource: format!("https://{}/*", domain),
            condition: PolicyCondition {
                date_less_than: DateLessThan {
                    aws_epoch_time: expiration,
                },
            },
        }],
    };
    
    // Serialize policy to JSON (compact, no whitespace)
    let policy_json = serde_json::to_string(&policy)?;
    
    // Sign the policy
    let signature = sign_policy(&policy_json, &private_key_pem)?;
    
    // Base64-encode policy and signature (URL-safe, no padding)
    let policy_b64 = URL_SAFE_NO_PAD.encode(policy_json.as_bytes());
    let signature_b64 = URL_SAFE_NO_PAD.encode(&signature);
    
    // Return cookies as key-value pairs
    Ok(vec![
        ("CloudFront-Policy".to_string(), policy_b64),
        ("CloudFront-Signature".to_string(), signature_b64),
        ("CloudFront-Key-Pair-Id".to_string(), key_pair_id),
    ])
}

/// Sign the CloudFront policy with RSA-SHA256
fn sign_policy(
    policy_json: &str,
    private_key_pem: &str,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    // Parse RSA private key from PEM (try PKCS#8 first, then PKCS#1)
    let private_key = match RsaPrivateKey::from_pkcs8_pem(private_key_pem) {
        Ok(k) => k,
        Err(_) => RsaPrivateKey::from_pkcs1_pem(private_key_pem)?,
    };
    let signing_key = SigningKey::<Sha256>::new(private_key);
    
    // Sign the policy
    let signature = signing_key.sign(policy_json.as_bytes());
    
    Ok(signature.to_vec())
}

/// Format Set-Cookie header for CloudFront signed cookies
pub fn format_cookie_headers(
    cookies: Vec<(String, String)>,
    domain: Option<&str>,
    secure: bool,
    duration_seconds: i64,
) -> Vec<String> {
    let max_age = duration_seconds;
    let secure_flag = if secure { "; Secure" } else { "" };
    
    cookies
        .into_iter()
        .map(|(name, value)| {
            match domain {
                Some(d) if !d.is_empty() => format!(
                    "{}={}; Domain={}; Path=/; Max-Age={}; HttpOnly{}; SameSite=None",
                    name, value, d, max_age, secure_flag
                ),
                _ => format!(
                    "{}={}; Path=/; Max-Age={}; HttpOnly{}; SameSite=None",
                    name, value, max_age, secure_flag
                ),
            }
        })
        .collect()
}

/// Issue CloudFront signed cookies on successful authentication
pub fn issue_signed_cookies_response(
    user_id: &str,
    duration_seconds: i64,
    request_origin: Option<&str>,
) -> Result<Response<Body>, Error> {
    let cookies = generate_signed_cookies(duration_seconds)
        .map_err(|e| format!("Failed to generate signed cookies: {}", e))?;
    
    // Decide cookie Domain
    let explicit_cookie_domain = std::env::var(CLOUDFRONT_COOKIE_DOMAIN).ok();
    let cookie_domain = explicit_cookie_domain.as_deref().or_else(|| {
        request_origin.and_then(|o| {
            // Extract host from origin like https://host:port
            let host = o.trim_start_matches("http://").trim_start_matches("https://");
            let host = host.split('/').next().unwrap_or("");
            let host = host.split(':').next().unwrap_or("");
            if host == "localhost" { None } else { Some(host) }
        })
    });

    let cookie_headers = format_cookie_headers(
        cookies,
        cookie_domain,
        true,  // secure=true in production
        duration_seconds,
    );
    
    let response_body = serde_json::json!({
        "user_id": user_id,
        "cloudfront_cookies_set": true,
        "expires_in_seconds": duration_seconds,
    });
    
    let mut response = Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .header(
            "Access-Control-Allow-Origin",
            request_origin.unwrap_or("*")
        )
        .header("Access-Control-Allow-Credentials", "true")
        .body(response_body.to_string().into())
        .map_err(Box::new)?;
    
    // Add Set-Cookie headers
    let headers = response.headers_mut();
    for cookie in cookie_headers {
        headers.append("Set-Cookie", cookie.parse()?);
    }
    
    Ok(response)
}
