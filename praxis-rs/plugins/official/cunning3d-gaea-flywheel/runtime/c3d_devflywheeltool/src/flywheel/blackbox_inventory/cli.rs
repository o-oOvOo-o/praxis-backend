#[derive(Debug)]
struct Cli {
    command: String,
    flags: BTreeMap<String, Vec<String>>,
    passthrough: Vec<String>,
}

impl Cli {
    fn parse(args: Vec<String>) -> Result<Self, String> {
        if args.is_empty() {
            return Ok(Self {
                command: "help".to_string(),
                flags: BTreeMap::new(),
                passthrough: Vec::new(),
            });
        }
        let command = args[0].clone();
        let mut flags: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut passthrough = Vec::new();
        let mut index = 1usize;
        if matches!(
            command.as_str(),
            "toolchain" | "toolchains" | "reverse-toolchain"
        ) && args
            .get(1)
            .map(|arg| !arg.starts_with("--"))
            .unwrap_or(false)
        {
            flags
                .entry("mode".to_string())
                .or_default()
                .push(args[1].clone());
            index = 2;
        }
        while index < args.len() {
            let arg = &args[index];
            if arg == "--" {
                passthrough.extend(args[index + 1..].iter().cloned());
                break;
            }
            if !arg.starts_with("--") {
                return Err(format!("Unexpected positional argument '{arg}'."));
            }
            let key = arg.trim_start_matches("--").to_string();
            let takes_value = !matches!(
                key.as_str(),
                "json"
                    | "run"
                    | "dry-run"
                    | "stage-report"
                    | "classic-stage-report"
                    | "shadow-focused"
                    | "first"
                    | "all"
                    | "help"
                    | "require-exact"
                    | "require-accepted"
                    | "native-only"
                    | "profile-native"
                    | "compare-native"
                    | "compare-stages"
                    | "compare-bridge"
                    | "ao-only"
                    | "ao-timing-only"
                    | "ao-root-replay-only"
                    | "ao-focused-raw-photon-only"
                    | "ao-normal-z-scale-sweep"
                    | "deep"
                    | "direct-bin"
                    | "release-bin"
                    | "no-incremental"
                    | "fresh-bridge-cache"
                    | "allow-stale-direct-bin"
                    | "file-capture"
                    | "keep-going"
                    | "worst-cell-diagnostics"
                    | "aux-diagnostics"
                    | "profile"
                    | "skip-native-preflight"
                    | "skip-seed-packets"
                    | "seed-packets-only"
                    | "require-all-pass"
                    | "require-consistent"
                    | "require-finite"
                    | "require-performance"
                    | "require-goal-complete"
                    | "require-pass"
                    | "require-speedup"
                    | "require-gaea-speedup"
                    | "require-bridge-exact"
                    | "require-speedup-gate"
                    | "capture-live-stages"
                    | "dump-stages"
                    | "require-gpu-active"
                    | "require-cce"
                    | "require-session-reuse"
                    | "gpu-exact-barrier"
                    | "trace-probe"
                    | "trace-directions"
                    | "path-commit-scalar-focus"
                    | "path-commit-integrated-debug"
                    | "cpu-trace-barrier"
                    | "cpu-commit-barrier"
                    | "resident-break-on-inactive"
                    | "resident-wave-loop"
                    | "resident-layer-loop"
                    | "resident-layer-cpu-shape-loop"
                    | "force-gpu-wave"
                    | "prewarm"
                    | "open"
                    | "no-new-console"
                    | "verbose"
                    | "offline"
                    | "repair"
                    | "reanalyze"
                    | "strict"
                    | "include-traces"
                    | "include-optional"
                    | "include-pixels"
                    | "inverse"
                    | "darker"
                    | "verify-gpu"
                    | "gpu"
                    | "verify-handle-gpu"
                    | "handle-gpu"
                    | "keep-nodes"
                    | "render-still-rocks"
                    | "debris-point-cloud"
                    | "debris-export-point-cloud"
            );
            if takes_value {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| format!("--{key} requires a value."))?
                    .clone();
                flags.entry(key).or_default().push(value);
            } else {
                flags.entry(key).or_default().push("true".to_string());
            }
            index += 1;
        }
        Ok(Self {
            command,
            flags,
            passthrough,
        })
    }

    fn flag(&self, key: &str) -> Option<&str> {
        self.flags
            .get(key)
            .and_then(|values| values.last())
            .map(String::as_str)
    }

    fn has(&self, key: &str) -> bool {
        self.flags.contains_key(key)
    }

    fn prefers_release_probe_bins(&self) -> bool {
        self.has("release-bin") || matches!(self.command.as_str(), "perf-migrate" | "speed-migrate")
    }

    fn node(&self) -> String {
        self.flag("node").unwrap_or("Mountain").to_string()
    }

    fn case_name(&self) -> String {
        self.flag("case").unwrap_or("old_baseline").to_string()
    }

    fn json(&self) -> bool {
        self.has("json")
    }

    fn run(&self) -> bool {
        self.has("run") && !self.has("dry-run")
    }
}
