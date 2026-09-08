"""Launch a real Praxis terminal without submitting a prompt; capture local terminal evidence."""
import argparse
import hashlib
import json
import os
import re
import psutil
from pathlib import Path
import threading
import time
import pyte
from winpty import PtyProcess
from thread_switch_report import parse_events, report, validate_plan

parser = argparse.ArgumentParser()
parser.add_argument("--binary", required=True)
parser.add_argument("--output", required=True)
parser.add_argument("--cwd", default=str(Path(__file__).resolve().parents[1]))
parser.add_argument("--seconds", type=float, default=15)
parser.add_argument("--trust-workspace", action="store_true")
parser.add_argument("--switches", type=int, default=0)
parser.add_argument("--rows", default="7,9,11")
parser.add_argument("--thread-plan", type=Path)
args = parser.parse_args()
rows = [int(row) for row in args.rows.split(",")]
if not rows or any(row < 1 or row > 45 for row in rows):
    parser.error("--rows must name terminal rows in 1..45")
output = Path(args.output)
output.mkdir(parents=True, exist_ok=True)
binary = Path(args.binary).resolve(strict=True)
started = time.monotonic()
env = os.environ.copy()
env.pop("PRAXIS_TUI_THREAD_SWITCH_PROBE", None)
if args.thread_plan:
    env["PRAXIS_TUI_THREAD_SWITCH_PROBE"] = str(args.thread_plan.resolve(strict=True))
env["RUST_LOG"] = "praxis_tui::thread_switch=info,praxis_app_gateway::thread_resume=info,praxis_tui=warn,praxis_core=warn"
app_logs = output.resolve() / "app-logs"
app_logs.mkdir(exist_ok=False)
process = PtyProcess.spawn([str(binary), "-c", "log_dir=" + json.dumps(str(app_logs))], cwd=args.cwd, env=env, dimensions=(45, 140))
owned_process = psutil.Process(process.pid)
screen = pyte.Screen(140, 45)
terminal_stream = pyte.Stream(screen)
screen_lock = threading.Lock()
chunks = []
actions = []
errors = []
memory_samples = []
def drain():
    pending = ""
    startup_text = ""
    trusted = False
    try:
        with (output / "terminal.ansi").open("w", encoding="utf-8") as stream:
            while process.isalive():
                data = process.read(65536)
                if not data:
                    break
                chunks.append({"elapsed_ms": (time.monotonic()-started)*1000, "chars":len(data)})
                with screen_lock:
                    terminal_stream.feed(data)
                stream.write(data)
                stream.flush()
                startup_text = (startup_text + data)[-200000:]
                clean = re.sub(r"\x1b\][^\x1b\x07]*(?:\x07|\x1b\\)", "", startup_text)
                clean = re.sub(r"\x1b\[[0-?]*[ -/]*[@-~]", "", clean)
                if args.trust_workspace and not trusted and "Doyoutrustthecontentsofthisdirectory?" in "".join(clean.split()):
                    trusted = True
                    actions.append({"action":"accept_workspace_trust", "elapsed_ms":(time.monotonic()-started)*1000})
                    process.write("\r")
                pending += data
                if "\x1b[6n" in pending:
                    process.write("\x1b[1;1R")
                    pending = pending.replace("\x1b[6n", "")
                pending = pending[-32:]
    except (EOFError, OSError) as error:
        errors.append(type(error).__name__)
reader = threading.Thread(target=drain, daemon=True)
reader.start()
try:
    deadline = time.monotonic() + args.seconds
    next_action = started + 12
    sent = 0
    while process.isalive() and time.monotonic() < deadline:
        try:
            memory_samples.append({"elapsed_ms":(time.monotonic()-started)*1000,
                "rss_bytes":owned_process.memory_info().rss})
        except psutil.NoSuchProcess:
            break
        if sent < args.switches and time.monotonic() >= next_action:
            row = rows[sent % len(rows)]
            with screen_lock:
                before = list(screen.display)
            (output / f"screen-{sent:03}-before.txt").write_text("\n".join(before), encoding="utf-8")
            process.write(f"\x1b[<0;8;{row}M\x1b[<0;8;{row}m")
            actions.append({"action":"sidebar_click", "column":8, "row":row, "sidebar_label":before[row-1][:28],
                "elapsed_ms":(time.monotonic()-started)*1000,
                "rss_bytes":owned_process.memory_info().rss if owned_process.is_running() else None})
            sent += 1
            next_action = time.monotonic() + 2
        time.sleep(0.1)
finally:
    if process.isalive():
        process.write("\x03")
        time.sleep(1)
    if process.isalive():
        process.write("\x03")
        time.sleep(1)
    if owned_process.is_running():
        try:
            owned_process.kill()
            owned_process.wait(timeout=5)
        except psutil.NoSuchProcess:
            pass
        except psutil.TimeoutExpired:
            errors.append("child_exit_wait_timeout")
    reader.join(timeout=3)
app_log = app_logs / "praxis-tui.log"
log_text = app_log.read_text(encoding="utf-8", errors="replace") if app_log.exists() else ""
events = parse_events(log_text)
measurements = report(events)
if args.thread_plan:
    measurements["plan_validation"] = validate_plan(events, json.loads(args.thread_plan.read_text(encoding="utf-8")), log_text)
(output / "switch-timings.json").write_text(json.dumps(measurements, indent=2), encoding="utf-8")
with screen_lock:
    (output / "screen-final.txt").write_text("\n".join(screen.display), encoding="utf-8")
receipt = {"binary":str(binary), "binary_sha256":hashlib.sha256(binary.read_bytes()).hexdigest(),
    "duration_ms":(time.monotonic()-started)*1000, "chunks":chunks, "reader_errors":errors, "actions":actions, "child_exited":not owned_process.is_running(),
    "thread_plan":str(args.thread_plan) if args.thread_plan else None,
    "memory_samples":memory_samples,
    "purpose":"terminal navigation evidence; switch completion requires matching application timing logs", "text_prompts_written":0}
(output / "receipt.json").write_text(json.dumps(receipt, indent=2), encoding="utf-8")
print(json.dumps({"chunks":len(chunks), "chars":sum(c["chars"] for c in chunks), "artifacts":str(output)}))

if args.thread_plan and not measurements["plan_validation"]["passed"]:
    raise SystemExit(2)
