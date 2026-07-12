#![recursion_limit = "256"]

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

mod crumble;
mod debris;
mod toolchain;

include!("flywheel/build_blackbox_inventory.rs");
include!("flywheel/cmd_raw_gate.rs");
include!("flywheel/cmd_perf_migrate.rs");
include!("flywheel/perf_candidate_diagnosis.rs");
include!("flywheel/status_recommendations.rs");
include!("flywheel/praxis_panel.rs");
include!("flywheel/transform_focused_cases.rs");
include!("flywheel/execute_live_heightfield_audit.rs");
include!("flywheel/frontier_health_commands.rs");
include!("flywheel/cmd_directional_warp_compare.rs");
include!("flywheel/warp_production_cases.rs");
include!("flywheel/summary_view.rs");
include!("flywheel/gpu_wave_diagnosis_view.rs");
include!("flywheel/gaea_viewport_reverse_powershell.rs");
include!("flywheel/tests.rs");
