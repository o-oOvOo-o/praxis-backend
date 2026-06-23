use super::*;

pub(super) enum Endpoint {
    SpawnPraxis(PathBuf),
    ConnectWs(String),
}

pub(super) struct BackgroundAppGateway {
    process: Child,
    pub(super) url: String,
}

pub(super) fn resolve_endpoint(
    praxis_bin: Option<PathBuf>,
    url: Option<String>,
) -> Result<Endpoint> {
    if praxis_bin.is_some() && url.is_some() {
        bail!("--praxis-bin and --url are mutually exclusive");
    }
    if let Some(praxis_bin) = praxis_bin {
        return Ok(Endpoint::SpawnPraxis(praxis_bin));
    }
    if let Some(url) = url {
        return Ok(Endpoint::ConnectWs(url));
    }
    Ok(Endpoint::ConnectWs("ws://127.0.0.1:4222".to_string()))
}

pub(super) fn resolve_shared_websocket_url(
    praxis_bin: Option<PathBuf>,
    url: Option<String>,
    command: &str,
) -> Result<String> {
    if praxis_bin.is_some() {
        bail!(
            "{command} requires --url or an already-running websocket app-gateway; --praxis-bin would spawn a private stdio app-gateway instead"
        );
    }

    Ok(url.unwrap_or_else(|| "ws://127.0.0.1:4222".to_string()))
}

impl BackgroundAppGateway {
    pub(super) fn spawn(praxis_bin: &Path, config_overrides: &[String]) -> Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")
            .context("failed to reserve a local port for websocket app-gateway")?;
        let addr = listener.local_addr()?;
        drop(listener);

        let url = format!("ws://{addr}");
        let mut cmd = Command::new(praxis_bin);
        if let Some(praxis_bin_parent) = praxis_bin.parent() {
            let mut path = OsString::from(praxis_bin_parent.as_os_str());
            if let Some(existing_path) = std::env::var_os("PATH") {
                path.push(":");
                path.push(existing_path);
            }
            cmd.env("PATH", path);
        }
        for override_kv in config_overrides {
            cmd.arg("--config").arg(override_kv);
        }
        let process = cmd
            .arg("app-gateway")
            .arg("--listen")
            .arg(&url)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .with_context(|| format!("failed to start `{}` app-gateway", praxis_bin.display()))?;

        Ok(Self { process, url })
    }
}

impl Drop for BackgroundAppGateway {
    fn drop(&mut self) {
        if let Ok(Some(status)) = self.process.try_wait() {
            println!("[background app-gateway exited: {status}]");
            return;
        }

        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}

pub(super) fn serve(
    praxis_bin: &Path,
    config_overrides: &[String],
    listen: &str,
    kill: bool,
) -> Result<()> {
    let runtime_dir = PathBuf::from("/tmp/praxis-app-gateway-test-client");
    fs::create_dir_all(&runtime_dir)
        .with_context(|| format!("failed to create runtime dir {}", runtime_dir.display()))?;
    let log_path = runtime_dir.join("app-gateway.log");
    if kill {
        kill_listeners_on_same_port(listen)?;
    }

    let log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .with_context(|| format!("failed to open log file {}", log_path.display()))?;
    let log_file_stderr = log_file
        .try_clone()
        .with_context(|| format!("failed to clone log file handle {}", log_path.display()))?;

    let mut cmdline = format!(
        "tail -f /dev/null | RUST_BACKTRACE=full RUST_LOG=warn,praxis_=trace {}",
        shell_quote(&praxis_bin.display().to_string())
    );
    for override_kv in config_overrides {
        cmdline.push_str(&format!(" --config {}", shell_quote(override_kv)));
    }
    cmdline.push_str(&format!(" app-gateway --listen {}", shell_quote(listen)));

    let child = Command::new("nohup")
        .arg("sh")
        .arg("-c")
        .arg(cmdline)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log_file))
        .stderr(Stdio::from(log_file_stderr))
        .spawn()
        .with_context(|| format!("failed to start `{}` app-gateway", praxis_bin.display()))?;

    let pid = child.id();

    println!("started {PRIMARY_CLI_COMMAND} app-gateway");
    println!("listen: {listen}");
    println!("pid: {pid} (launcher process)");
    println!("log: {}", log_path.display());

    Ok(())
}

pub(super) fn kill_listeners_on_same_port(listen: &str) -> Result<()> {
    let url = Url::parse(listen).with_context(|| format!("invalid --listen URL `{listen}`"))?;
    let port = url
        .port_or_known_default()
        .with_context(|| format!("unable to infer port from --listen URL `{listen}`"))?;

    let output = Command::new("lsof")
        .arg("-nP")
        .arg(format!("-tiTCP:{port}"))
        .arg("-sTCP:LISTEN")
        .output()
        .with_context(|| format!("failed to run lsof for port {port}"))?;

    if !output.status.success() {
        return Ok(());
    }

    let pids: Vec<u32> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.trim().parse::<u32>().ok())
        .collect();

    if pids.is_empty() {
        return Ok(());
    }

    for pid in pids {
        println!("killing listener pid {pid} on port {port}");
        let pid_str = pid.to_string();
        let term_status = Command::new("kill")
            .arg(&pid_str)
            .status()
            .with_context(|| format!("failed to send SIGTERM to pid {pid}"))?;
        if !term_status.success() {
            continue;
        }
    }

    thread::sleep(Duration::from_millis(300));

    let output = Command::new("lsof")
        .arg("-nP")
        .arg(format!("-tiTCP:{port}"))
        .arg("-sTCP:LISTEN")
        .output()
        .with_context(|| format!("failed to re-check listeners on port {port}"))?;
    if !output.status.success() {
        return Ok(());
    }
    let remaining: Vec<u32> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.trim().parse::<u32>().ok())
        .collect();
    for pid in remaining {
        println!("force killing remaining listener pid {pid} on port {port}");
        let _ = Command::new("kill").arg("-9").arg(pid.to_string()).status();
    }

    Ok(())
}

pub(super) fn shell_quote(input: &str) -> String {
    format!("'{}'", input.replace('\'', "'\\''"))
}
