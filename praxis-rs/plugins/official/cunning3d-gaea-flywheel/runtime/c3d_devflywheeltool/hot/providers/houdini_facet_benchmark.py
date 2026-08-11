import argparse
import json
import os
import statistics
import time

import hou

from houdini_facet_capture import configure_facet, create_source, focused_cases, parameters, semantic_cases


def percentile(values, fraction):
    ordered = sorted(values)
    return ordered[min(len(ordered) - 1, int((len(ordered) - 1) * fraction))]


def create_grid_source(container, size):
    node = container.createNode("python")
    node.parm("python").set(
        "geo = hou.pwd().geometry()\n"
        "size = {}\n".format(size)
        + "points = []\n"
        + "for row in range(size):\n"
        + "    for column in range(size):\n"
        + "        point = geo.createPoint()\n"
        + "        point.setPosition((float(column), float(row), 0.0))\n"
        + "        points.append(point)\n"
        + "for row in range(size - 1):\n"
        + "    for column in range(size - 1):\n"
        + "        first = row * size + column\n"
        + "        polygon = geo.createPolygon()\n"
        + "        for index in (first, first + 1, first + size + 1, first + size):\n"
        + "            polygon.addVertex(points[index])\n"
        + "        polygon.setIsClosed(True)\n"
    )
    return node


def selected_cases(profile):
    if profile == "focused":
        return focused_cases()
    if profile == "semantic":
        return semantic_cases()
    return (
        {"case_id": "stress/grid_post_normals", "source": "grid", "overrides": {"postnml": 1}},
        {"case_id": "stress/grid_unique", "source": "grid", "overrides": {"unique": 1}},
        {"case_id": "stress/grid_cusp", "source": "grid", "overrides": {"cusp": 1, "angle": 30.0}},
        {"case_id": "stress/grid_orient", "source": "grid", "overrides": {"orientPolys": 1}},
        {"case_id": "stress/grid_consolidate_zero", "source": "grid", "overrides": {"cons": 2, "dist": 0.0}},
    )


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--matrix", choices=("focused", "semantic", "stress"), default="semantic")
    parser.add_argument("--stress-size", type=int, default=64)
    parser.add_argument("--warmup", type=int, default=20)
    parser.add_argument("--iterations", type=int, default=100)
    parser.add_argument("--output", required=True)
    arguments = parser.parse_args()
    cases = selected_cases(arguments.matrix)
    container = hou.node("/obj").createNode("geo", run_init_scripts=False)
    nodes = []
    try:
        for case in cases:
            source = create_grid_source(container, arguments.stress_size) if case["source"] == "grid" else create_source(container, case["source"])
            facet = container.createNode("facet")
            facet.setInput(0, source)
            configure_facet(facet, parameters(case["overrides"]))
            facet.cook(force=True)
            nodes.append((case["case_id"], facet))
        for _ in range(arguments.warmup):
            for _, node in nodes:
                node.cook(force=True)
        samples = {case_id: [] for case_id, _ in nodes}
        batch_samples = []
        for _ in range(arguments.iterations):
            batch_started = time.perf_counter_ns()
            for case_id, node in nodes:
                started = time.perf_counter_ns()
                node.cook(force=True)
                samples[case_id].append((time.perf_counter_ns() - started) / 1_000_000.0)
            batch_samples.append((time.perf_counter_ns() - batch_started) / 1_000_000.0)
    finally:
        container.destroy()
    case_stats = {
        case_id: {
            "median_ms": statistics.median(values),
            "p95_ms": percentile(values, 0.95),
            "min_ms": min(values),
        }
        for case_id, values in samples.items()
    }
    payload = {
        "schema": "c3d.facet.benchmark.v1",
        "provider": {"id": "houdini", "version": hou.applicationVersionString()},
        "profile": arguments.matrix,
        "warmup": arguments.warmup,
        "iterations": arguments.iterations,
        "cases": len(cases),
        "stress_size": arguments.stress_size if arguments.matrix == "stress" else None,
        "batch": {
            "median_ms": statistics.median(batch_samples),
            "p95_ms": percentile(batch_samples, 0.95),
            "min_ms": min(batch_samples),
        },
        "per_case_median_sum_ms": sum(item["median_ms"] for item in case_stats.values()),
        "case_stats": case_stats,
        "scope": "prebuilt SOP nodes; force cook only; process startup, node creation, capture, and file I/O excluded",
    }
    output = os.path.abspath(arguments.output)
    os.makedirs(os.path.dirname(output), exist_ok=True)
    with open(output, "w", encoding="utf-8") as handle:
        json.dump(payload, handle, indent=2, sort_keys=True)
        handle.write("\n")
    print(json.dumps({"output": output, "cases": len(cases), "iterations": arguments.iterations}, sort_keys=True))


if __name__ == "__main__":
    main()
