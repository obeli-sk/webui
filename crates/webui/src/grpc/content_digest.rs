use crate::grpc::grpc_client::ContentDigest;

impl From<String> for ContentDigest {
    fn from(digest: String) -> Self {
        ContentDigest { digest }
    }
}
