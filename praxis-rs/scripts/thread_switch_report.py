"""Summarize real post-draw thread-switch events; terminal byte arrivals are not frame timings."""
import argparse
import json
import math
from pathlib import Path
import re

MARKER = "thread switch frame committed"

def parse_events(text):
    events = []
    for line in text.splitlines():
        if MARKER not in line:
            continue
        values = dict(re.findall(r"(thread_id|cached|first_frame|ready|elapsed_us|draw_us)=([^ ]+)", line))
        try:
            if values["first_frame"] not in ("true", "false") or values["ready"] not in ("true", "false"):
                continue
            events.append({"thread_id": values["thread_id"], "cached": values.get("cached"),
                "first_frame":values["first_frame"] == "true", "ready":values["ready"] == "true",
                "elapsed_us":int(values["elapsed_us"]), "draw_us":int(values["draw_us"])})
        except (KeyError, ValueError):
            continue
    return events

def validate_plan(events, expected, log_text):
    actual = [event["thread_id"] for event in events if event["ready"]]
    return {"passed":actual == expected and "thread switch probe complete" in log_text,
        "expected_ready_frames":len(expected), "actual_ready_frames":len(actual),
        "order_matches":actual == expected,
        "completion_marker":"thread switch probe complete" in log_text}

def distribution(values):
    values = sorted(values)
    if not values:
        return {"samples":0}
    return {"samples":len(values), "p50_us":values[math.ceil(len(values)*0.5)-1],
        "p95_us":values[math.ceil(len(values)*0.95)-1], "max_us":values[-1]}

def report(events):
    ready_by_cache = {"hit":[], "miss":[], "unknown":[]}
    for event in events:
        if event["ready"]:
            outcome = {"Some(true)":"hit", "Some(false)":"miss"}.get(event.get("cached"), "unknown")
            ready_by_cache[outcome].append(event["elapsed_us"])
    return {"unique_threads":len({event["thread_id"] for event in events}),
        "first_frame":distribution([event["elapsed_us"] for event in events if event["first_frame"]]),
        "ready_frame":distribution([event["elapsed_us"] for event in events if event["ready"]]),
        "ready_by_cache":{key:distribution(values) for key,values in ready_by_cache.items()},
        "draw":distribution([event["draw_us"] for event in events]), "events":events,
        "note":"No events means no measurement, not zero latency. Small sample percentiles are descriptive only."}

if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("log", type=Path)
    args = parser.parse_args()
    print(json.dumps(report(parse_events(args.log.read_text(encoding="utf-8", errors="replace"))), indent=2))
