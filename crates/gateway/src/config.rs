use serde::{Serialize, Deserialize};
use std::env;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GatewayConfig {
    pub cluster_id: String,
    pub mountpoint: String,
    pub hf3fs_binary: String,
    pub token_file: Option<String>,
    pub mgmtd_addresses: Option<String>,
    pub bind_addr: String,
    pub region: String,
    pub data_root: String,
    pub access_key: String,
    pub secret_key: String,
    pub use_usrbio: bool,
    pub auth_disabled: bool,
}

impl GatewayConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        let cluster_id = env::var("CLUSTER_ID").map_err(|_| anyhow::anyhow!("CLUSTER_ID required"))?;
        let default_mount = format!("/var/lib/3fs/mnt/{}", cluster_id);
        let mountpoint = env::var("MOUNTPOINT").unwrap_or(default_mount);
        let bind_addr = env::var("BIND_ADDRESS").unwrap_or(":9000".to_string());
        let region = env::var("REGION").unwrap_or("us-east-1".to_string());
        let data_root = env::var("DATA_ROOT").unwrap_or(format!("{}/buckets", mountpoint));
        let access_key = env::var("ACCESS_KEY").map_err(|_| anyhow::anyhow!("ACCESS_KEY required"))?;
        let secret_key = env::var("SECRET_KEY").map_err(|_| anyhow::anyhow!("SECRET_KEY required"))?;
        let hf3fs_binary = env::var("Hf3fsBinary").unwrap_or("/opt/3fs/bin/hf3fs_fuse_main".to_string());
        let token_file = env::var("TokenFile").ok();
        let mgmtd_addresses = env::var("MgmtdAddresses").ok();
        let use_usrbio = env::var("UseUsrBio").ok().map(|v| v == "1" || v.to_lowercase() == "true").unwrap_or(false);
        let auth_disabled = env::var("AUTH_DISABLED").ok().map(|v| v == "1" || v.to_lowercase() == "true").unwrap_or(false);
        Ok(Self { cluster_id, mountpoint, hf3fs_binary, token_file, mgmtd_addresses, bind_addr, region, data_root, access_key, secret_key, use_usrbio, auth_disabled })
    }
}

