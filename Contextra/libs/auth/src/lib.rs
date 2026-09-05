use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
};
use chrono::{Duration as ChronoDuration, Utc};
use errors::ContextraError;
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use storage::api_key::{ApiKeyRecord, ApiKeyStore};
use types::{OrgId, UserId};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthContext {
    pub user_id: UserId,
    pub org_id: OrgId,
    #[serde(default)]
    pub scopes: Vec<String>,
}

impl AuthContext {
    pub fn new(user_id: UserId, org_id: OrgId, scopes: Vec<String>) -> Self {
        Self {
            user_id,
            org_id,
            scopes,
        }
    }

    pub fn authorizer(&self) -> ScopeAuthorizer {
        ScopeAuthorizer::new(self.scopes.clone())
    }
}

impl From<ApiKeyRecord> for AuthContext {
    fn from(record: ApiKeyRecord) -> Self {
        Self {
            user_id: record.user_id,
            org_id: record.org_id,
            scopes: record.scopes,
        }
    }
}

pub trait Authorizer {
    fn can(&self, action: &str, resource: &str) -> bool;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeAuthorizer {
    scopes: Vec<String>,
}

impl ScopeAuthorizer {
    pub fn new(scopes: Vec<String>) -> Self {
        Self { scopes }
    }
}

impl Authorizer for ScopeAuthorizer {
    fn can(&self, action: &str, resource: &str) -> bool {
        self.scopes
            .iter()
            .any(|scope| scope_matches(scope, action, resource))
    }
}

fn scope_matches(scope: &str, action: &str, resource: &str) -> bool {
    let Some((scope_action, scope_resource)) = scope.split_once(':') else {
        return false;
    };

    let action_matches = scope_action == "*" || scope_action == action;
    let resource_matches = scope_resource == "*" || scope_resource == resource;

    action_matches && resource_matches
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Claims {
    sub: String,
    user_id: Uuid,
    org_id: Uuid,
    scopes: Vec<String>,
    iss: String,
    iat: usize,
    exp: usize,
}

#[derive(Debug, Clone)]
pub struct JwtSessionManager {
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
    issuer: String,
    token_ttl: ChronoDuration,
}

impl JwtSessionManager {
    pub fn new(
        secret: impl AsRef<[u8]>,
        issuer: impl Into<String>,
        token_ttl: ChronoDuration,
    ) -> Self {
        let secret = secret.as_ref();
        Self {
            encoding_key: EncodingKey::from_secret(secret),
            decoding_key: DecodingKey::from_secret(secret),
            issuer: issuer.into(),
            token_ttl,
        }
    }

    pub fn issue_session_token(&self, context: &AuthContext) -> Result<String, ContextraError> {
        let issued_at = Utc::now();
        let expires_at = issued_at + self.token_ttl;
        let claims = Claims {
            sub: context.user_id.to_string(),
            user_id: Uuid::from(context.user_id),
            org_id: Uuid::from(context.org_id),
            scopes: context.scopes.clone(),
            iss: self.issuer.clone(),
            iat: issued_at.timestamp() as usize,
            exp: expires_at.timestamp() as usize,
        };

        let mut header = Header::new(Algorithm::HS256);
        header.typ = Some("JWT".to_string());

        encode(&header, &claims, &self.encoding_key)
            .map_err(|e| ContextraError::Internal(format!("Failed to issue session token: {e}")))
    }

    pub fn verify_session_token(&self, token: &str) -> Result<AuthContext, ContextraError> {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.set_issuer(&[self.issuer.as_str()]);

        let token_data = decode::<Claims>(token, &self.decoding_key, &validation)
            .map_err(|e| ContextraError::Unauthorized(format!("Invalid session token: {e}")))?;

        Ok(AuthContext {
            user_id: UserId::from(token_data.claims.user_id),
            org_id: OrgId::from(token_data.claims.org_id),
            scopes: token_data.claims.scopes,
        })
    }
}

pub fn hash_api_key(api_key: &str) -> Result<String, ContextraError> {
    let salt = SaltString::generate(&mut rand_core::OsRng);
    let argon2 = Argon2::default();

    argon2
        .hash_password(api_key.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|e| ContextraError::Internal(format!("Failed to hash API key: {e}")))
}

pub fn verify_api_key(api_key: &str, stored_hash: &str) -> Result<bool, ContextraError> {
    let parsed_hash = PasswordHash::new(stored_hash)
        .map_err(|e| ContextraError::Internal(format!("Failed to parse API key hash: {e}")))?;

    Ok(Argon2::default()
        .verify_password(api_key.as_bytes(), &parsed_hash)
        .is_ok())
}

pub fn split_api_key(raw_key: &str) -> Result<(&str, &str), ContextraError> {
    raw_key.split_once('.').ok_or_else(|| {
        ContextraError::Unauthorized("Malformed API key; expected '<key_id>.<secret>'".to_string())
    })
}

pub async fn authenticate_api_key<S>(
    store: &S,
    raw_key: &str,
) -> Result<AuthContext, ContextraError>
where
    S: ApiKeyStore,
{
    let (key_id, secret) = split_api_key(raw_key)?;
    let record = store
        .find_by_key_id(key_id)
        .await?
        .ok_or_else(|| ContextraError::Unauthorized("API key not found".to_string()))?;

    if !verify_api_key(secret, &record.key_hash)? {
        return Err(ContextraError::Unauthorized(
            "Invalid API key secret".to_string(),
        ));
    }

    Ok(record.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn auth_context() -> AuthContext {
        AuthContext::new(
            UserId::new(),
            OrgId::new(),
            vec![
                "read:documents".to_string(),
                "write:messages".to_string(),
                "*:profile".to_string(),
            ],
        )
    }

    #[test]
    fn jwt_round_trip() -> Result<(), Box<dyn std::error::Error>> {
        let manager =
            JwtSessionManager::new("session-secret", "contextra", ChronoDuration::minutes(30));
        let context = auth_context();

        let token = manager.issue_session_token(&context)?;
        let decoded = manager.verify_session_token(&token)?;

        assert_eq!(decoded, context);

        Ok(())
    }

    #[test]
    fn api_key_hashing_and_verification() -> Result<(), Box<dyn std::error::Error>> {
        let raw_key = "super-secret-api-key";
        let hash = hash_api_key(raw_key)?;

        assert_ne!(hash, raw_key);
        assert!(hash.starts_with("$argon2"));
        assert!(verify_api_key(raw_key, &hash)?);
        assert!(!verify_api_key("incorrect-key", &hash)?);

        Ok(())
    }

    #[test]
    fn scope_authorizer_matches_wildcards() {
        let authorizer = ScopeAuthorizer::new(vec![
            "read:documents".to_string(),
            "write:*".to_string(),
            "*:profile".to_string(),
        ]);

        assert!(authorizer.can("read", "documents"));
        assert!(authorizer.can("write", "messages"));
        assert!(authorizer.can("delete", "profile"));
        assert!(!authorizer.can("delete", "documents"));
    }
}
