use serde::{Serialize, Deserialize};

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct ListBucketsResult {
    pub Owner: Owner,
    pub Buckets: Buckets,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Owner { pub ID: String, pub DisplayName: String }

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Buckets { pub Bucket: Vec<Bucket> }

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Bucket { pub Name: String, pub CreationDate: String }

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct ListObjectsV2Result {
    pub Name: String,
    pub Prefix: Option<String>,
    pub Delimiter: Option<String>,
    pub KeyCount: i32,
    pub MaxKeys: i32,
    pub IsTruncated: bool,
    pub Contents: Vec<Object>,
    pub CommonPrefixes: Option<Vec<CommonPrefix>>,    
    pub NextContinuationToken: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct Object {
    pub Key: String,
    pub LastModified: String,
    pub ETag: String,
    pub Size: u64,
    pub StorageClass: String,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct CommonPrefix {
    pub Prefix: String,
}


