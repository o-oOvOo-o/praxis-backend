import argparse
import datetime
import json
import os

import hou

from houdini_geometry_capture import geometry_domains


def cases():
    return (
        {
            "case_id": "focused/explicit_nonuniform",
            "source_position": (1.0, -2.0, 0.5),
            "source_size": (2.0, 4.0, 6.0),
            "target_mode": "explicit",
            "target_position": (10.0, 3.0, -4.0),
            "target_size": (8.0, 2.0, 3.0),
            "uniform_scale": False,
            "scale_axis": "x",
        },
        {
            "case_id": "focused/second_input_uniform_x",
            "source_position": (1.0, -2.0, 0.5),
            "source_size": (2.0, 4.0, 6.0),
            "target_mode": "second_input",
            "target_position": (-5.0, 8.0, 2.0),
            "target_size": (6.0, 10.0, 12.0),
            "uniform_scale": True,
            "scale_axis": "x",
        },
    )


def canonical_parameters(case):
    return {
        "target_mode": case["target_mode"],
        "target_position": list(case["target_position"]),
        "target_size": list(case["target_size"]),
        "scale_to_fit": True,
        "uniform_scale": case["uniform_scale"],
        "scale_axis": case["scale_axis"],
        "translate": True,
        "source_justify": ["center", "center", "center"],
        "target_justify": ["same", "same", "same"],
    }


def configure_box(node, position, size):
    node.parmTuple("t").set(position)
    node.parmTuple("size").set(size)


def configure_match_size(node, case):
    node.parm("justifytarget").set("explicit" if case["target_mode"] == "explicit" else "input")
    node.parm("doscale").set(1)
    node.parm("uniformscale").set(int(case["uniform_scale"]))
    node.parm("scale_axis").set(case["scale_axis"])
    node.parm("scale_x").set(1)
    node.parm("scale_y").set(1)
    node.parm("scale_z").set(1)
    node.parm("dotranslate").set(1)
    node.parmTuple("size").set(case["target_size"])
    node.parmTuple("t").set(case["target_position"])
    for axis in "xyz":
        node.parm("justify_" + axis).set("center")
        node.parm("goal_" + axis).set("same")


def capture_case(root, container, case):
    source = container.createNode("box")
    configure_box(source, case["source_position"], case["source_size"])
    target = None
    if case["target_mode"] == "second_input":
        target = container.createNode("box")
        configure_box(target, case["target_position"], case["target_size"])
    match_size = container.createNode("matchsize")
    match_size.setInput(0, source)
    if target is not None:
        match_size.setInput(1, target)
    configure_match_size(match_size, case)
    match_size.cook(force=True)
    slug = case["case_id"].replace("/", "_")
    return {
        "case_id": case["case_id"],
        "parameters": canonical_parameters(case),
        "provider_overrides": {
            "justifytarget": "explicit" if case["target_mode"] == "explicit" else "input",
            "doscale": 1,
            "uniformscale": int(case["uniform_scale"]),
            "scale_axis": case["scale_axis"],
            "dotranslate": 1,
            "justify_x": "center",
            "justify_y": "center",
            "justify_z": "center",
            "goal_x": "same",
            "goal_y": "same",
            "goal_z": "same",
            "size": list(case["target_size"]),
            "t": list(case["target_position"]),
        },
        "input": {"domains": geometry_domains(root, slug, "input", source.geometry())},
        "target": {"domains": geometry_domains(root, slug, "target", target.geometry())}
        if target is not None
        else None,
        "output": {"domains": geometry_domains(root, slug, "output", match_size.geometry())},
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--matrix", choices=("focused",), default="focused")
    parser.add_argument("--output", required=True)
    arguments = parser.parse_args()
    output = os.path.abspath(arguments.output)
    root = os.path.dirname(output)
    container = hou.node("/obj").createNode("geo", run_init_scripts=False)
    try:
        captured_cases = [capture_case(root, container, case) for case in cases()]
    finally:
        container.destroy()
    payload = {
        "schema": "c3d.parity.capture.v1",
        "provider": {
            "id": "houdini",
            "version": hou.applicationVersionString(),
            "execution_mode": "headless_node_network",
        },
        "subject": {"kind": "sop", "id": "matchsize"},
        "profile": arguments.matrix,
        "cases": captured_cases,
        "provenance": {
            "command": "hython houdini_match_size_capture.py",
            "captured_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
            "executable": os.path.abspath(os.sys.executable),
        },
    }
    os.makedirs(root, exist_ok=True)
    with open(output, "w", encoding="utf-8") as handle:
        json.dump(payload, handle, indent=2, sort_keys=True)
        handle.write("\n")
    print(json.dumps({"output": output, "subject": "matchsize", "cases": len(captured_cases)}, sort_keys=True))


if __name__ == "__main__":
    main()
