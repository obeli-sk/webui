use super::grpc_client;
use crate::util::color::generate_color_from_hash;
use std::fmt::Display;

const JOIN_SET_ID_INFIX: char = ':';

impl Display for grpc_client::JoinSetId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let code = match self.kind() {
            grpc_client::join_set_id::JoinSetKind::OneOff => "o",
            grpc_client::join_set_id::JoinSetKind::Named => "n",
            grpc_client::join_set_id::JoinSetKind::Generated => "g",
        };
        write!(f, "{code}{JOIN_SET_ID_INFIX}{}", self.name)
    }
}

impl grpc_client::JoinSetId {
    pub fn color(&self) -> String {
        use std::hash::Hasher as _;
        use std::hash::{DefaultHasher, Hash};

        let mut hasher = DefaultHasher::new();
        self.kind.hash(&mut hasher);
        self.name.hash(&mut hasher);
        let hash = hasher.finish();
        generate_color_from_hash(hash)
    }
}
