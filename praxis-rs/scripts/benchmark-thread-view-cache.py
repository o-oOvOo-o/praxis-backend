"""Exercise the production cache policy without compiling the full TUI; this does not measure frames."""
from pathlib import Path
import hashlib
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
TARGET = Path("F:/cargo-target2/Cunning3D/debug/praxis-thread-cache-policy-probe.exe")
source = (ROOT / "tui/src/app/thread_view_cache.rs").read_text(encoding="utf-8")
start = source.index("impl ThreadViewCache {")
end = source.index("\nimpl App {", start)
implementation = source[start:end].replace("pub(super) ", "")
constants = "\n".join(line for line in source.splitlines() if line.startswith("const THREAD_VIEW_CACHE_"))
HEAD = 'use std::collections::{HashMap, VecDeque};\nuse std::time::Instant;\ntype ThreadId = usize;\nstruct CachedThreadView { transcript_cells: Vec<()> }\n#[derive(Default)]\nstruct ThreadViewCache { entries: HashMap<ThreadId, CachedThreadView>, lru: VecDeque<ThreadId>, transcript_cells: usize }\n'
MAIN = 'fn main() {\n    let mut cache = ThreadViewCache::default();\n    cache.insert(0, CachedThreadView { transcript_cells: vec![(); 1000] });\n    assert!(cache.insert(1, CachedThreadView { transcript_cells: vec![(); 1000] }).is_empty());\n    assert_eq!(cache.insert(2, CachedThreadView { transcript_cells: vec![(); 1000] }).iter().map(|(id, _)| *id).collect::<Vec<_>>(), vec![0]);\n    assert!(cache.take(1).is_some());\n    assert_eq!(cache.transcript_cells, 1000);\n    assert_eq!(cache.drain().len(), 1);\n    assert_eq!(cache.transcript_cells, 0);\n    for id in 0..20 { cache.insert(id, CachedThreadView { transcript_cells: vec![] }); }\n    assert_eq!(cache.entries.len(), THREAD_VIEW_CACHE_MAX_ENTRIES);\n    cache.drain();\n\n    println!("threads,cells_per_thread,switches,hits,misses,policy_us");\n    for count in [2usize, 3, 8, 16] {\n        for cells in [50usize, 250, 1000] {\n            let mut cache = ThreadViewCache::default();\n            let mut active = 0;\n            let mut hits = 0;\n            let start = Instant::now();\n            for step in 1..=10000 {\n                let next = step % count;\n                if cache.take(next).is_some() { hits += 1; }\n                let _ = cache.insert(active, CachedThreadView { transcript_cells: vec![(); cells] });\n                active = next;\n            }\n            println!("{count},{cells},10000,{hits},{},{:.1}",10000-hits,start.elapsed().as_secs_f64()*1e6);\n            cache.drain();\n        }\n    }\n}\n'

with tempfile.TemporaryDirectory(prefix="praxis-cache-policy-") as work:
    harness = Path(work) / "cache_policy.rs"
    harness.write_text(HEAD + constants + "\n" + implementation + "\n" + MAIN, encoding="utf-8")
    subprocess.run(["rustc", "--edition", "2024", "-O", str(harness), "-o", str(TARGET)], cwd=ROOT, check=True)
    print("production_sha256=" + hashlib.sha256(source.encode()).hexdigest(), flush=True)
    subprocess.run([str(TARGET)], check=True)
