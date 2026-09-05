use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

macro_rules! define_id {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        pub struct $name(pub Uuid);

        impl $name {
            pub fn new() -> Self {
                Self(Uuid::now_v7())
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl FromStr for $name {
            type Err = uuid::Error;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                let id = Uuid::parse_str(s)?;
                Ok(Self(id))
            }
        }

        impl From<Uuid> for $name {
            fn from(id: Uuid) -> Self {
                Self(id)
            }
        }

        impl From<$name> for Uuid {
            fn from(id: $name) -> Self {
                id.0
            }
        }
    };
}

define_id!(DocumentId);
define_id!(CollectionId);
define_id!(UserId);
define_id!(OrgId);
define_id!(ConversationId);

pub type Metadata = HashMap<String, serde_json::Value>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Collection {
    pub id: CollectionId,
    pub name: String,
    #[serde(default)]
    pub metadata: Metadata,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub id: DocumentId,
    pub collection_id: CollectionId,
    pub content: String,
    #[serde(default)]
    pub metadata: Metadata,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Chunk {
    pub id: Uuid,
    pub document_id: DocumentId,
    pub content: String,
    #[serde(default)]
    pub metadata: Metadata,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub id: Uuid,
    pub conversation_id: ConversationId,
    pub role: Role,
    pub content: String,
    #[serde(default)]
    pub metadata: Metadata,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_id_display_and_parse() -> Result<(), Box<dyn std::error::Error>> {
        let original_uuid = Uuid::now_v7();
        let doc_id = DocumentId(original_uuid);

        let id_str = doc_id.to_string();
        assert_eq!(id_str, original_uuid.to_string());

        let parsed_id = DocumentId::from_str(&id_str)?;
        assert_eq!(doc_id, parsed_id);

        Ok(())
    }

    #[test]
    fn test_role_serialization() -> Result<(), Box<dyn std::error::Error>> {
        let role = Role::Assistant;

        let serialized = serde_json::to_string(&role)?;
        assert_eq!(serialized, "\"assistant\"");

        let deserialized: Role = serde_json::from_str(&serialized)?;
        assert_eq!(role, deserialized);

        Ok(())
    }

    #[test]
    fn test_document_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
        let doc = Document {
            id: DocumentId::new(),
            collection_id: CollectionId::new(),
            content: "Hello world".to_string(),
            metadata: {
                let mut map = HashMap::new();
                map.insert("author".to_string(), json!("Alice"));
                map
            },
        };

        let serialized = serde_json::to_string(&doc)?;
        let deserialized: Document = serde_json::from_str(&serialized)?;

        assert_eq!(doc, deserialized);

        Ok(())
    }

    #[test]
    fn test_chunk_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
        let chunk = Chunk {
            id: Uuid::now_v7(),
            document_id: DocumentId::new(),
            content: "Hello".to_string(),
            metadata: HashMap::new(),
        };

        let serialized = serde_json::to_string(&chunk)?;
        let deserialized: Chunk = serde_json::from_str(&serialized)?;

        assert_eq!(chunk, deserialized);

        Ok(())
    }

    #[test]
    fn test_message_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
        let message = Message {
            id: Uuid::now_v7(),
            conversation_id: ConversationId::new(),
            role: Role::User,
            content: "What's the weather?".to_string(),
            metadata: HashMap::new(),
        };

        let serialized = serde_json::to_string(&message)?;
        let deserialized: Message = serde_json::from_str(&serialized)?;

        assert_eq!(message, deserialized);

        Ok(())
    }
}
