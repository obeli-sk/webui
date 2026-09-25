use chrono::{DateTime, Utc};

#[derive(Clone, Copy, PartialEq, Eq, Hash, prost::Message)]
pub struct Timestamp {
    #[prost(int64, tag = "1")]
    pub seconds: i64,
    #[prost(int32, tag = "2")]
    pub nanos: i32,
}

impl From<DateTime<Utc>> for Timestamp {
    fn from(value: DateTime<Utc>) -> Self {
        Self {
            seconds: value.timestamp(),
            nanos: value.timestamp_subsec_nanos() as i32,
        }
    }
}

impl From<Timestamp> for DateTime<Utc> {
    fn from(value: Timestamp) -> Self {
        DateTime::from_timestamp(value.seconds, value.nanos as u32)
            .expect("protobuf timestamp must be a valid UTC instant")
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, prost::Message)]
pub struct Duration {
    #[prost(int64, tag = "1")]
    pub seconds: i64,
    #[prost(int32, tag = "2")]
    pub nanos: i32,
}

#[derive(Clone, PartialEq, Eq, Hash, prost::Message)]
pub struct Any {
    #[prost(string, tag = "1")]
    pub type_url: String,
    #[prost(bytes = "vec", tag = "2")]
    pub value: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamp_round_trips_across_unix_epoch() {
        for (seconds, nanos) in [(-1, 750_000_000), (0, 250_000_000), (1_780_000_000, 123)] {
            let original = DateTime::<Utc>::from_timestamp(seconds, nanos).unwrap();
            let encoded = Timestamp::from(original);
            assert_eq!((encoded.seconds, encoded.nanos), (seconds, nanos as i32));
            assert_eq!(DateTime::<Utc>::from(encoded), original);
        }
    }
}
