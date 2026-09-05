//! JSON Schema definitions for tool inputs

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A JSON Schema for describing tool input requirements.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolSchema {
    /// JSON Schema type (always "object" for tool inputs)
    #[serde(rename = "type")]
    pub schema_type: String,
    /// Human-readable title
    pub title: String,
    /// Description of what the input does
    pub description: String,
    /// Required field names
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub required: Vec<String>,
    /// Properties/fields of the input object
    #[serde(skip_serializing_if = "HashMap::is_empty", default)]
    pub properties: HashMap<String, JsonSchema>,
    /// Additional properties allowed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additional_properties: Option<bool>,
}

impl ToolSchema {
    /// Create a new tool schema builder
    pub fn builder(title: impl Into<String>, description: impl Into<String>) -> ToolSchemaBuilder {
        ToolSchemaBuilder::new(title, description)
    }

    /// Check if a field is required
    pub fn is_required(&self, field: &str) -> bool {
        self.required.contains(&field.to_string())
    }

    /// Get a property by name
    pub fn get_property(&self, name: &str) -> Option<&JsonSchema> {
        self.properties.get(name)
    }
}

impl Default for ToolSchema {
    fn default() -> Self {
        Self {
            schema_type: "object".to_string(),
            title: String::new(),
            description: String::new(),
            required: Vec::new(),
            properties: HashMap::new(),
            additional_properties: Some(false),
        }
    }
}

/// Builder for constructing tool schemas
pub struct ToolSchemaBuilder {
    schema: ToolSchema,
}

impl ToolSchemaBuilder {
    /// Create a new schema builder
    pub fn new(title: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            schema: ToolSchema {
                schema_type: "object".to_string(),
                title: title.into(),
                description: description.into(),
                required: Vec::new(),
                properties: HashMap::new(),
                additional_properties: Some(false),
            },
        }
    }

    /// Add a required string property
    pub fn required_string(mut self, name: impl Into<String>, description: impl Into<String>) -> Self {
        let name = name.into();
        self.schema.required.push(name.clone());
        self.schema.properties.insert(
            name,
            JsonSchema::String {
                description: description.into(),
            },
        );
        self
    }

    /// Add an optional string property
    pub fn optional_string(mut self, name: impl Into<String>, description: impl Into<String>) -> Self {
        let name = name.into();
        self.schema.properties.insert(
            name,
            JsonSchema::String {
                description: description.into(),
            },
        );
        self
    }

    /// Add a required property with any JSON schema type
    pub fn required_property(mut self, name: impl Into<String>, schema: JsonSchema) -> Self {
        let name = name.into();
        self.schema.required.push(name.clone());
        self.schema.properties.insert(name, schema);
        self
    }

    /// Add an optional property
    pub fn optional_property(mut self, name: impl Into<String>, schema: JsonSchema) -> Self {
        self.schema.properties.insert(name.into(), schema);
        self
    }

    /// Build and return the schema
    pub fn build(self) -> ToolSchema {
        self.schema
    }
}

/// Simplified JSON Schema types for tool inputs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum JsonSchema {
    /// String type
    String {
        /// Description of the string field
        description: String,
    },
    /// Integer type
    Integer {
        /// Description of the integer field
        description: String,
    },
    /// Number type (float)
    Number {
        /// Description of the number field
        description: String,
    },
    /// Boolean type
    Boolean {
        /// Description of the boolean field
        description: String,
    },
    /// Array type
    Array {
        /// Description of the array field
        description: String,
        /// Schema for array items
        items: Box<JsonSchema>,
    },
    /// Object type
    Object {
        /// Description of the object field
        description: String,
        /// Properties of the object
        #[serde(skip_serializing_if = "HashMap::is_empty", default)]
        properties: HashMap<String, JsonSchema>,
    },
}

impl JsonSchema {
    /// Create a string schema
    pub fn string(description: impl Into<String>) -> Self {
        Self::String {
            description: description.into(),
        }
    }

    /// Create an integer schema
    pub fn integer(description: impl Into<String>) -> Self {
        Self::Integer {
            description: description.into(),
        }
    }

    /// Create a number schema
    pub fn number(description: impl Into<String>) -> Self {
        Self::Number {
            description: description.into(),
        }
    }

    /// Create a boolean schema
    pub fn boolean(description: impl Into<String>) -> Self {
        Self::Boolean {
            description: description.into(),
        }
    }

    /// Create an array schema
    pub fn array(description: impl Into<String>, items: JsonSchema) -> Self {
        Self::Array {
            description: description.into(),
            items: Box::new(items),
        }
    }

    /// Create an object schema
    pub fn object(description: impl Into<String>) -> Self {
        Self::Object {
            description: description.into(),
            properties: HashMap::new(),
        }
    }

    /// Add a property to an object schema
    pub fn with_property(mut self, name: impl Into<String>, schema: JsonSchema) -> Self {
        if let Self::Object { ref mut properties, .. } = self {
            properties.insert(name.into(), schema);
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_builder_basic() {
        let schema = ToolSchema::builder("Echo", "Echo a message back")
            .required_string("message", "The message to echo")
            .build();

        assert_eq!(schema.title, "Echo");
        assert_eq!(schema.description, "Echo a message back");
        assert!(schema.is_required("message"));
        assert!(!schema.is_required("optional"));
    }

    #[test]
    fn schema_serialization_roundtrip() {
        let schema = ToolSchema::builder("Test", "Test schema")
            .required_string("name", "A name")
            .optional_string("description", "A description")
            .build();

        let json = serde_json::to_string(&schema).unwrap();
        let deserialized: ToolSchema = serde_json::from_str(&json).unwrap();
        assert_eq!(schema, deserialized);
    }

    #[test]
    fn json_schema_string() {
        let schema = JsonSchema::string("A string field");
        if let JsonSchema::String { description } = schema {
            assert_eq!(description, "A string field");
        } else {
            panic!("Expected String variant");
        }
    }

    #[test]
    fn get_property_returns_expected() {
        let schema = ToolSchema::builder("Test", "Test")
            .required_string("field", "A field")
            .build();

        assert!(schema.get_property("field").is_some());
        assert!(schema.get_property("missing").is_none());
    }
}
