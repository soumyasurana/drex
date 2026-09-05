use async_trait::async_trait;
use errors::ContextraError;
use qdrant_client::Qdrant;
use qdrant_client::qdrant::point_id::PointIdOptions;
use qdrant_client::qdrant::{
    CreateCollectionBuilder, DeletePointsBuilder, Distance, PointStruct, PointsIdsList,
    SearchPointsBuilder, UpsertPointsBuilder, VectorParamsBuilder,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use types::Metadata;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VectorRecord {
    pub id: Uuid,
    pub embedding: Vec<f32>,
    #[serde(default)]
    pub payload: Metadata,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchResult {
    pub id: Uuid,
    pub score: f32,
    #[serde(default)]
    pub payload: Metadata,
}

#[async_trait]
pub trait VectorStore: Send + Sync {
    async fn create_collection(
        &self,
        collection_name: &str,
        vector_size: usize,
    ) -> Result<(), ContextraError>;

    async fn upsert_vectors(
        &self,
        collection_name: &str,
        records: &[VectorRecord],
    ) -> Result<(), ContextraError>;

    async fn search(
        &self,
        collection_name: &str,
        query: &[f32],
        limit: usize,
    ) -> Result<Vec<SearchResult>, ContextraError>;

    async fn delete_by_id(&self, collection_name: &str, ids: &[Uuid])
    -> Result<(), ContextraError>;
}

#[derive(Clone)]
pub struct QdrantVectorStore {
    client: Qdrant,
}

impl QdrantVectorStore {
    pub fn connect(url: &str, api_key: Option<String>) -> Result<Self, ContextraError> {
        let mut builder = Qdrant::from_url(url);
        if let Some(api_key) = api_key {
            builder = builder.api_key(api_key);
        }

        let client = builder.build().map_err(|e| {
            ContextraError::StorageError(format!("Failed to build Qdrant client: {e}"))
        })?;

        Ok(Self { client })
    }

    fn payload_to_qdrant(payload: &Metadata) -> Result<qdrant_client::Payload, ContextraError> {
        let value = Value::Object(payload.clone().into_iter().collect());
        value.try_into().map_err(|e| {
            ContextraError::StorageError(format!(
                "Failed to convert payload to Qdrant payload: {e}"
            ))
        })
    }

    fn qdrant_payload_to_metadata(
        payload: qdrant_client::Payload,
    ) -> Result<Metadata, ContextraError> {
        let value = serde_json::to_value(payload).map_err(|e| {
            ContextraError::StorageError(format!("Failed to serialize Qdrant payload: {e}"))
        })?;

        serde_json::from_value(value).map_err(|e| {
            ContextraError::StorageError(format!(
                "Failed to convert Qdrant payload to metadata: {e}"
            ))
        })
    }

    fn map_point_id(id: Option<qdrant_client::qdrant::PointId>) -> Result<Uuid, ContextraError> {
        let id = id.ok_or_else(|| {
            ContextraError::StorageError("Qdrant result was missing a point id".to_string())
        })?;

        match id.point_id_options {
            Some(PointIdOptions::Uuid(value)) => Uuid::parse_str(&value).map_err(|e| {
                ContextraError::StorageError(format!("Failed to parse Qdrant UUID point id: {e}"))
            }),
            Some(PointIdOptions::Num(_)) => Err(ContextraError::StorageError(
                "Qdrant returned a numeric point id but this store uses UUID ids".to_string(),
            )),
            None => Err(ContextraError::StorageError(
                "Qdrant result was missing point id options".to_string(),
            )),
        }
    }
}

#[async_trait]
impl VectorStore for QdrantVectorStore {
    async fn create_collection(
        &self,
        collection_name: &str,
        vector_size: usize,
    ) -> Result<(), ContextraError> {
        if vector_size == 0 {
            return Err(ContextraError::Validation(
                "vector_size must be greater than zero".to_string(),
            ));
        }

        self.client
            .create_collection(
                CreateCollectionBuilder::new(collection_name).vectors_config(
                    VectorParamsBuilder::new(vector_size as u64, Distance::Cosine),
                ),
            )
            .await
            .map_err(|e| {
                ContextraError::StorageError(format!(
                    "Failed to create Qdrant collection '{collection_name}': {e}"
                ))
            })?;

        Ok(())
    }

    async fn upsert_vectors(
        &self,
        collection_name: &str,
        records: &[VectorRecord],
    ) -> Result<(), ContextraError> {
        let points: Result<Vec<PointStruct>, ContextraError> = records
            .iter()
            .map(|record| {
                let payload = Self::payload_to_qdrant(&record.payload)?;
                Ok(PointStruct::new(
                    record.id.to_string(),
                    record.embedding.clone(),
                    payload,
                ))
            })
            .collect();

        self.client
            .upsert_points(UpsertPointsBuilder::new(collection_name, points?).wait(true))
            .await
            .map_err(|e| {
                ContextraError::StorageError(format!(
                    "Failed to upsert vectors into Qdrant collection '{collection_name}': {e}"
                ))
            })?;

        Ok(())
    }

    async fn search(
        &self,
        collection_name: &str,
        query: &[f32],
        limit: usize,
    ) -> Result<Vec<SearchResult>, ContextraError> {
        let response = self
            .client
            .search_points(
                SearchPointsBuilder::new(collection_name, query.to_vec(), limit as u64)
                    .with_payload(true),
            )
            .await
            .map_err(|e| {
                ContextraError::StorageError(format!(
                    "Failed to search Qdrant collection '{collection_name}': {e}"
                ))
            })?;

        let results = response
            .result
            .into_iter()
            .map(|point| {
                Ok(SearchResult {
                    id: Self::map_point_id(point.id)?,
                    score: point.score,
                    payload: Self::qdrant_payload_to_metadata(point.payload.into())?,
                })
            })
            .collect::<Result<Vec<_>, ContextraError>>()?;

        Ok(results)
    }

    async fn delete_by_id(
        &self,
        collection_name: &str,
        ids: &[Uuid],
    ) -> Result<(), ContextraError> {
        let point_ids = PointsIdsList {
            ids: ids.iter().map(|id| id.to_string().into()).collect(),
        };

        self.client
            .delete_points(
                DeletePointsBuilder::new(collection_name)
                    .points(point_ids)
                    .wait(true),
            )
            .await
            .map_err(|e| {
                ContextraError::StorageError(format!(
                    "Failed to delete vectors from Qdrant collection '{collection_name}': {e}"
                ))
            })?;

        Ok(())
    }
}

#[derive(Clone, Default)]
pub struct InMemoryVectorStore {
    state: Arc<Mutex<HashMap<String, CollectionState>>>,
}

#[derive(Debug, Clone)]
struct CollectionState {
    vector_size: usize,
    records: HashMap<Uuid, VectorRecord>,
}

impl InMemoryVectorStore {
    pub fn new() -> Self {
        Self::default()
    }

    fn cosine_similarity(left: &[f32], right: &[f32]) -> f32 {
        let (dot, left_norm, right_norm) = left.iter().zip(right.iter()).fold(
            (0.0_f32, 0.0_f32, 0.0_f32),
            |(dot, left_norm, right_norm), (l, r)| {
                (dot + (l * r), left_norm + (l * l), right_norm + (r * r))
            },
        );

        if left_norm == 0.0 || right_norm == 0.0 {
            return 0.0;
        }

        dot / (left_norm.sqrt() * right_norm.sqrt())
    }
}

#[async_trait]
impl VectorStore for InMemoryVectorStore {
    async fn create_collection(
        &self,
        collection_name: &str,
        vector_size: usize,
    ) -> Result<(), ContextraError> {
        if vector_size == 0 {
            return Err(ContextraError::Validation(
                "vector_size must be greater than zero".to_string(),
            ));
        }

        let mut state = self.state.lock().await;
        match state.get(collection_name) {
            Some(existing) if existing.vector_size != vector_size => {
                Err(ContextraError::Conflict(format!(
                    "collection '{collection_name}' already exists with vector size {}",
                    existing.vector_size
                )))
            }
            Some(_) => Ok(()),
            None => {
                state.insert(
                    collection_name.to_string(),
                    CollectionState {
                        vector_size,
                        records: HashMap::new(),
                    },
                );
                Ok(())
            }
        }
    }

    async fn upsert_vectors(
        &self,
        collection_name: &str,
        records: &[VectorRecord],
    ) -> Result<(), ContextraError> {
        let mut state = self.state.lock().await;
        let collection = state.get_mut(collection_name).ok_or_else(|| {
            ContextraError::NotFound(format!("collection '{collection_name}' not found"))
        })?;

        for record in records {
            if record.embedding.len() != collection.vector_size {
                return Err(ContextraError::Validation(format!(
                    "record {} embedding length {} does not match collection vector size {}",
                    record.id,
                    record.embedding.len(),
                    collection.vector_size
                )));
            }
            collection.records.insert(record.id, record.clone());
        }

        Ok(())
    }

    async fn search(
        &self,
        collection_name: &str,
        query: &[f32],
        limit: usize,
    ) -> Result<Vec<SearchResult>, ContextraError> {
        let state = self.state.lock().await;
        let collection = state.get(collection_name).ok_or_else(|| {
            ContextraError::NotFound(format!("collection '{collection_name}' not found"))
        })?;

        if query.len() != collection.vector_size {
            return Err(ContextraError::Validation(format!(
                "query embedding length {} does not match collection vector size {}",
                query.len(),
                collection.vector_size
            )));
        }

        let mut results: Vec<SearchResult> = collection
            .records
            .values()
            .map(|record| SearchResult {
                id: record.id,
                score: Self::cosine_similarity(query, &record.embedding),
                payload: record.payload.clone(),
            })
            .collect();

        results.sort_by(|left, right| {
            right
                .score
                .partial_cmp(&left.score)
                .unwrap_or(Ordering::Equal)
                .then_with(|| left.id.cmp(&right.id))
        });

        results.truncate(limit);
        Ok(results)
    }

    async fn delete_by_id(
        &self,
        collection_name: &str,
        ids: &[Uuid],
    ) -> Result<(), ContextraError> {
        let mut state = self.state.lock().await;
        let collection = state.get_mut(collection_name).ok_or_else(|| {
            ContextraError::NotFound(format!("collection '{collection_name}' not found"))
        })?;

        for id in ids {
            collection.records.remove(id);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn in_memory_store_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
        let store = InMemoryVectorStore::new();
        let collection = "memory-test";
        store.create_collection(collection, 3).await?;

        let record_a = VectorRecord {
            id: Uuid::now_v7(),
            embedding: vec![1.0, 0.0, 0.0],
            payload: [("label".to_string(), json!("a"))].into_iter().collect(),
        };
        let record_b = VectorRecord {
            id: Uuid::now_v7(),
            embedding: vec![0.0, 1.0, 0.0],
            payload: [("label".to_string(), json!("b"))].into_iter().collect(),
        };

        store
            .upsert_vectors(collection, &[record_a.clone(), record_b.clone()])
            .await?;

        let results = store.search(collection, &[0.9, 0.1, 0.0], 2).await?;
        assert_eq!(results[0].id, record_a.id);
        assert_eq!(results[0].payload.get("label"), Some(&json!("a")));

        store.delete_by_id(collection, &[record_a.id]).await?;
        let results = store.search(collection, &[0.9, 0.1, 0.0], 2).await?;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, record_b.id);

        Ok(())
    }

    #[test]
    fn qdrant_types_are_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<QdrantVectorStore>();
        assert_send_sync::<InMemoryVectorStore>();
    }
}
