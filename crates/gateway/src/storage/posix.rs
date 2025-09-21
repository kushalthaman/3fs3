use crate::config::GatewayConfig;
use fs_err as fs;
use std::path::{Path, PathBuf};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::fs as tfs;
use anyhow::Context;

pub async fn ensure_roots(cfg: &GatewayConfig) -> anyhow::Result<()> {
    fs::create_dir_all(&cfg.data_root)?;
    let multipart = Path::new(&cfg.mountpoint).join(".multipart");
    fs::create_dir_all(multipart)?;
    Ok(())
}

pub fn bucket_dir(cfg: &GatewayConfig, bucket: &str) -> PathBuf {
    Path::new(&cfg.data_root).join(bucket)
}

pub fn object_paths(cfg: &GatewayConfig, bucket: &str, key: &str) -> (PathBuf, PathBuf) {
    let data = bucket_dir(cfg, bucket).join(key);
    let meta = Path::new(&format!("{}.meta.json", data.display())).to_path_buf();
    (data, meta)
}

pub async fn ensure_parent_dirs(p: &Path) -> anyhow::Result<()> {
    if let Some(parent) = p.parent() { tfs::create_dir_all(parent).await?; }
    Ok(())
}

pub async fn write_file_atomic(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    ensure_parent_dirs(path).await?;
    let tmp = path.with_extension(".tmp");
    {
        let mut f = tfs::File::create(&tmp).await?;
        f.write_all(bytes).await?;
        f.flush().await?;
    }
    tfs::rename(&tmp, path).await?;
    Ok(())
}

pub async fn read_file(path: &Path) -> anyhow::Result<Vec<u8>> {
    let mut f = tfs::File::open(path).await.with_context(|| format!("open {}", path.display()))?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf).await?;
    Ok(buf)
}

pub async fn delete_if_exists(path: &Path) -> anyhow::Result<()> {
    if tfs::metadata(path).await.is_ok() {
        tfs::remove_file(path).await.ok();
    }
    Ok(())
}


