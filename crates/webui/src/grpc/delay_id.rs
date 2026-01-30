use crate::{grpc::grpc_client, util::color::generate_color_from_hash};
use std::fmt::Display;

impl Display for grpc_client::DelayId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.id)
    }
}

impl grpc_client::DelayId {
    pub fn color(&self) -> String {
        use std::hash::Hasher as _;
        use std::hash::{DefaultHasher, Hash};

        let mut hasher = DefaultHasher::new();
        self.id.hash(&mut hasher);
        let hash = hasher.finish();
        generate_color_from_hash(hash)
    }
}
