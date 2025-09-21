use crate::config::GatewayConfig;
use fs_err as fs;
use std::path::Path;
use tokio::process::Command;
use tokio::time::{sleep, Duration};

pub async fn is_mounted_and_writeable(mountpoint: &str) -> bool {
    let p = Path::new(mountpoint);
    if !p.exists() { return false; }
    let test_path = p.join(".gw_ready");
    match fs::write(&test_path, b"ok") {
        Ok(_) => {
            let _ = fs::remove_file(&test_path);
            true
        }
        Err(_) => false,
    }
}

fn launcher_toml(cfg: &GatewayConfig) -> String {
    let addr_list = cfg.mgmtd_addresses.clone().unwrap_or_default();
    format!(
        "cluster_id = \"{}\"\nmountpoint = \"{}\"\ntoken_file = \"{}\"\n[mgmtd_client]\nmgmtd_server_addresses = [{}]\n",
        cfg.cluster_id,
        cfg.mountpoint,
        cfg.token_file.clone().unwrap_or_default(),
        addr_list.split(',').filter(|s| !s.is_empty()).map(|s| format!("\"{}\"", s.trim())).collect::<Vec<_>>().join(", ")
    )
}

pub async fn ensure_mount(cfg: &GatewayConfig) -> anyhow::Result<()> {
    fs::create_dir_all(&cfg.mountpoint)?;
    // Render launcher TOML
    let etc_dir = Path::new(&cfg.mountpoint).parent().unwrap_or(Path::new("/var/lib/3fs")).join("etc");
    fs::create_dir_all(&etc_dir)?;
    let toml_path = etc_dir.join("hf3fs_fuse_main_launcher.toml");
    fs::write(&toml_path, launcher_toml(cfg))?;

    if is_mounted_and_writeable(&cfg.mountpoint).await {
        return Ok(());
    }

    if !Path::new(&cfg.hf3fs_binary).exists() {
        return Ok(());
    }
    let mut cmd = Command::new(&cfg.hf3fs_binary);
    cmd.arg("-cfg").arg(&toml_path);
    cmd.kill_on_drop(true);
    let _child = cmd.spawn()?;

    for _ in 0..60 {
        if is_mounted_and_writeable(&cfg.mountpoint).await { return Ok(()); }
        sleep(Duration::from_secs(1)).await;
    }
    anyhow::bail!("3FS mount not ready at {}", cfg.mountpoint)
}


