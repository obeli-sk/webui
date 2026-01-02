use crate::grpc::grpc_client;
use std::fmt::Display;

impl Display for grpc_client::DelayId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Delay_{}", self.id)
    }
}
