use uuid::Uuid;

pub trait IdGenerator: Send + Sync {
    fn generate(&self) -> Uuid;
}

#[derive(Clone, Default)]
pub struct UuidV7Generator;

impl IdGenerator for UuidV7Generator {
    fn generate(&self) -> Uuid {
        Uuid::now_v7()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uuid_v7_generator() {
        let generator = UuidV7Generator;
        let id1 = generator.generate();
        let id2 = generator.generate();
        assert_ne!(id1, id2);
        assert_eq!(id1.get_version_num(), 7);
        assert_eq!(id2.get_version_num(), 7);
    }
}
