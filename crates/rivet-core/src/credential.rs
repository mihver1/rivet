use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::connection::AuthMethod;

/// A reusable authentication profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credential {
    pub id: Uuid,
    pub name: String,
    pub auth: AuthMethod,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Credential {
    pub fn new(name: impl Into<String>, auth: AuthMethod) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            auth,
            description: None,
            created_at: now,
            updated_at: now,
        }
    }
}

/// How a connection resolves its authentication.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", content = "data")]
pub enum AuthSource {
    /// Auth configured directly on the connection.
    Inline(AuthMethod),
    /// Reference to a credential profile.
    Profile { credential_id: Uuid },
}

impl<'de> serde::Deserialize<'de> for AuthSource {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::Error;

        let value = serde_json::Value::deserialize(deserializer)?;
        let obj = value
            .as_object()
            .ok_or_else(|| D::Error::custom("expected object"))?;

        let type_str = obj
            .get("type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| D::Error::custom("missing 'type' field"))?;

        match type_str {
            "Inline" => {
                let data = obj
                    .get("data")
                    .ok_or_else(|| D::Error::custom("missing 'data' for Inline"))?;
                let auth: AuthMethod =
                    serde_json::from_value(data.clone()).map_err(D::Error::custom)?;
                Ok(AuthSource::Inline(auth))
            }
            "Profile" => {
                let data = obj
                    .get("data")
                    .and_then(|d| d.as_object())
                    .ok_or_else(|| D::Error::custom("missing 'data' for Profile"))?;
                let id_str = data
                    .get("credential_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| D::Error::custom("missing 'credential_id'"))?;
                let credential_id = Uuid::parse_str(id_str)
                    .map_err(|e| D::Error::custom(format!("invalid UUID: {e}")))?;
                Ok(AuthSource::Profile { credential_id })
            }
            _ => {
                // Legacy format: bare AuthMethod (e.g. {"type":"Agent","data":{...}})
                let auth: AuthMethod =
                    serde_json::from_value(serde_json::Value::Object(obj.clone()))
                        .map_err(D::Error::custom)?;
                Ok(AuthSource::Inline(auth))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::AuthMethod;

    #[test]
    fn test_credential_new() {
        let cred = Credential::new("deploy-key", AuthMethod::Agent { socket_path: None });
        assert_eq!(cred.name, "deploy-key");
        assert!(matches!(cred.auth, AuthMethod::Agent { .. }));
        assert!(cred.description.is_none());
    }

    #[test]
    fn test_credential_serialization_roundtrip() {
        let cred = Credential::new("test", AuthMethod::Password("secret".into()));
        let json = serde_json::to_string(&cred).unwrap();
        let deserialized: Credential = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, cred.name);
        assert_eq!(deserialized.id, cred.id);
    }

    #[test]
    fn test_auth_source_inline_roundtrip() {
        let source = AuthSource::Inline(AuthMethod::Agent { socket_path: None });
        let json = serde_json::to_string(&source).unwrap();
        let deserialized: AuthSource = serde_json::from_str(&json).unwrap();
        assert!(matches!(
            deserialized,
            AuthSource::Inline(AuthMethod::Agent { .. })
        ));
    }

    #[test]
    fn test_auth_source_profile_roundtrip() {
        let id = Uuid::new_v4();
        let source = AuthSource::Profile { credential_id: id };
        let json = serde_json::to_string(&source).unwrap();
        let deserialized: AuthSource = serde_json::from_str(&json).unwrap();
        match deserialized {
            AuthSource::Profile { credential_id } => assert_eq!(credential_id, id),
            _ => panic!("expected Profile"),
        }
    }

    #[test]
    fn test_auth_source_legacy_deserialization() {
        let json = r#"{"type":"Agent","data":{"socket_path":null}}"#;
        let source: AuthSource = serde_json::from_str(json).unwrap();
        assert!(matches!(
            source,
            AuthSource::Inline(AuthMethod::Agent { .. })
        ));
    }

    #[test]
    fn test_auth_source_legacy_agent_no_data() {
        let json = r#"{"type":"Agent"}"#;
        let source: AuthSource = serde_json::from_str(json).unwrap();
        assert!(matches!(
            source,
            AuthSource::Inline(AuthMethod::Agent { socket_path: None })
        ));
    }

    #[test]
    fn test_auth_source_legacy_password() {
        let json = r#"{"type":"Password","data":"secret"}"#;
        let source: AuthSource = serde_json::from_str(json).unwrap();
        match source {
            AuthSource::Inline(AuthMethod::Password(pw)) => assert_eq!(pw, "secret"),
            _ => panic!("expected Inline(Password)"),
        }
    }
}
