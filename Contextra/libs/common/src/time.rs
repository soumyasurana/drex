use chrono::{DateTime, Utc};

pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

#[derive(Clone, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockClock {
        time: DateTime<Utc>,
    }

    impl Clock for MockClock {
        fn now(&self) -> DateTime<Utc> {
            self.time
        }
    }

    #[test]
    fn test_system_clock() {
        let clock = SystemClock;
        let before = Utc::now();
        let now = clock.now();
        let after = Utc::now();
        assert!(before <= now);
        assert!(now <= after);
    }

    #[test]
    fn test_mock_clock() {
        let expected = Utc::now();
        let clock = MockClock { time: expected };
        assert_eq!(clock.now(), expected);
    }
}
