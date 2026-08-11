import argparse
import datetime
import json
import os
import time

import hou


SUBJECT = "fuse::2.0"
IGNORED_PROVIDER_DETAIL_ATTRIBUTES = {"varmap"}


def json_value(value):
    if isinstance(value, hou.Vector2):
        return list(value)
    if isinstance(value, (hou.Vector3, hou.Vector4, hou.Quaternion)):
        return list(value)
    if isinstance(value, tuple):
        return [json_value(item) for item in value]
    return value


def attribute_payload(elements, attribute):
    return {
        "storage": str(attribute.dataType()).split(".")[-1],
        "tuple_size": attribute.size(),
        "values": [json_value(element.attribValue(attribute)) for element in elements],
    }


def group_payload(groups, elements):
    return {
        group.name(): [group.contains(element) for element in elements]
        for group in sorted(groups, key=lambda value: value.name())
    }


def normalized_domains(geometry):
    points = list(geometry.points())
    primitives = list(geometry.prims())
    vertices = [vertex for primitive in primitives for vertex in primitive.vertices()]
    return {
        "point": {
            "count": len(points),
            "attributes": {
                attribute.name(): attribute_payload(points, attribute)
                for attribute in sorted(geometry.pointAttribs(), key=lambda value: value.name())
            },
            "groups": group_payload(geometry.pointGroups(), points),
        },
        "vertex": {
            "count": len(vertices),
            "point_indices": [vertex.point().number() for vertex in vertices],
            "attributes": {
                attribute.name(): attribute_payload(vertices, attribute)
                for attribute in sorted(geometry.vertexAttribs(), key=lambda value: value.name())
            },
            "groups": group_payload(geometry.vertexGroups(), vertices),
        },
        "primitive": {
            "count": len(primitives),
            "vertex_counts": [len(primitive.vertices()) for primitive in primitives],
            "point_loops": [
                [point.number() for point in primitive.points()] for primitive in primitives
            ],
            "closed": [
                bool(primitive.intrinsicValue("closed"))
                if primitive.type() == hou.primType.Polygon
                else False
                for primitive in primitives
            ],
            "attributes": {
                attribute.name(): attribute_payload(primitives, attribute)
                for attribute in sorted(geometry.primAttribs(), key=lambda value: value.name())
            },
            "groups": group_payload(geometry.primGroups(), primitives),
        },
        "detail": {
            "attributes": {
                attribute.name(): {
                    "storage": str(attribute.dataType()).split(".")[-1],
                    "tuple_size": attribute.size(),
                    "values": [json_value(geometry.attribValue(attribute))],
                }
                for attribute in sorted(geometry.globalAttribs(), key=lambda value: value.name())
                if attribute.name() not in IGNORED_PROVIDER_DETAIL_ATTRIBUTES
            }
        },
    }


def chain_geometry():
    geometry = hou.Geometry()
    points = []
    for x in (0.0, 0.0009, 0.0018):
        point = geometry.createPoint()
        point.setPosition((x, 0.0, 0.0))
        points.append(point)
    point_id = geometry.addAttrib(hou.attribType.Point, "pid", -1)
    point_value = geometry.addAttrib(hou.attribType.Point, "foo", 0.0)
    for index, point in enumerate(points):
        point.setAttribValue(point_id, 10 + index)
        point.setAttribValue(point_value, float(index + 1))
    selected = geometry.createPointGroup("pick")
    selected.add(points[1])
    selected.add(points[2])
    return geometry


def topology_geometry():
    geometry = hou.Geometry()
    positions = (
        (0.0, 0.0, 0.0),
        (0.0005, 0.0, 0.0),
        (1.0, 0.0, 0.0),
        (0.0, 1.0, 0.0),
        (2.0, 0.0, 0.0),
        (2.0005, 0.0, 0.0),
        (3.0, 0.0, 0.0),
    )
    points = []
    for position in positions:
        point = geometry.createPoint()
        point.setPosition(position)
        points.append(point)
    point_id = geometry.addAttrib(hou.attribType.Point, "pid", -1)
    point_value = geometry.addAttrib(hou.attribType.Point, "foo", 0.0)
    for index, point in enumerate(points):
        point.setAttribValue(point_id, 100 + index)
        point.setAttribValue(point_value, index + 0.25)
    vertex_id = geometry.addAttrib(hou.attribType.Vertex, "vtx", -1)
    primitive_id = geometry.addAttrib(hou.attribType.Prim, "primid", -1)
    for primitive_index, point_indices in enumerate(((0, 1, 2, 3), (4, 5, 6))):
        primitive = geometry.createPolygon()
        primitive.setIsClosed(True)
        for point_index in point_indices:
            primitive.addVertex(points[point_index])
        primitive.setAttribValue(primitive_id, 200 + primitive_index)
    vertices = [vertex for primitive in geometry.prims() for vertex in primitive.vertices()]
    for index, vertex in enumerate(vertices):
        vertex.setAttribValue(vertex_id, 300 + index)
    selected = geometry.createPointGroup("pick")
    selected.add(points[1])
    selected.add(points[4])
    second = geometry.createPrimGroup("second")
    second.add(geometry.prims()[1])
    return geometry


def normals_geometry():
    geometry = topology_geometry()
    geometry.addAttrib(hou.attribType.Vertex, "N", (0.0, 1.0, 0.0))
    geometry.addAttrib(hou.attribType.Prim, "N", (0.0, -1.0, 0.0))
    return geometry


def reducer_geometry():
    geometry = hou.Geometry()
    points = []
    for index, x in enumerate((0.0, 0.0002, 0.0004)):
        point = geometry.createPoint()
        point.setPosition((x, 0.0, 0.0))
        points.append(point)
    scalar = geometry.addAttrib(hou.attribType.Point, "foo", 0.0)
    integer = geometry.addAttrib(hou.attribType.Point, "ival", 0)
    label = geometry.addAttrib(hou.attribType.Point, "label", "")
    weight = geometry.addAttrib(hou.attribType.Point, "weight", 0.0)
    target = geometry.addAttrib(hou.attribType.Point, "snap_to", -1)
    for index, point in enumerate(points):
        point.setAttribValue(scalar, (1.0, 4.0, 2.0)[index])
        point.setAttribValue(integer, (1, 4, 2)[index])
        point.setAttribValue(label, ("a", "c", "b")[index])
        point.setAttribValue(weight, (0.25, 2.0, 1.0)[index])
        point.setAttribValue(target, (2, 2, -1)[index])
    odd = geometry.createPointGroup("odd")
    odd.add(points[1])
    ends = geometry.createPointGroup("ends")
    ends.add(points[0])
    ends.add(points[2])
    query = geometry.createPointGroup("query")
    query.add(points[0])
    query.add(points[1])
    target_group = geometry.createPointGroup("target")
    target_group.add(points[2])
    return geometry


def grid_geometry():
    geometry = hou.Geometry()
    for position in (
        (-0.26, 0.04, 0.0),
        (-0.24, 0.04, 0.0),
        (0.24, 0.04, 0.0),
        (0.26, 0.04, 0.0),
        (0.74, 0.04, 0.14),
    ):
        point = geometry.createPoint()
        point.setPosition(position)
    return geometry


def two_input_geometry():
    query = hou.Geometry()
    for x in (0.0, 0.0009, 0.0018):
        point = query.createPoint()
        point.setPosition((x, 0.0, 0.0))
    target = hou.Geometry()
    for x in (0.0005, 0.0017):
        point = target.createPoint()
        point.setPosition((x, 0.0, 0.0))
    return query, target


def performance_geometry():
    geometry = hou.Geometry()
    for index in range(200_000):
        pair = index // 2
        within_pair = index & 1
        point = geometry.createPoint()
        point.setPosition((pair * 0.01 + within_pair * 0.0005, 0.0, 0.0))
    return geometry


def case_registry(matrix):
    if matrix == "performance":
        return [("performance/paired_200k", performance_geometry, {})]
    focused = [
        ("near/least_chain/default", chain_geometry, {}),
        ("topology/repeated_and_degenerate/default", topology_geometry, {}),
    ]
    if matrix == "focused":
        return focused
    semantic = focused + [
        ("near/least_chain/keep_fused", chain_geometry, {"keepconsolidatedpoints": 1}),
        ("near/least_chain/snap_only", chain_geometry, {"consolidatesnappedpoints": 0}),
        ("near/least_chain/least_position", chain_geometry, {"positionsnapmethod": 1}),
        ("near/least_chain/greatest_position", chain_geometry, {"positionsnapmethod": 2}),
        (
            "near/least_chain/snap_outputs",
            chain_geometry,
            {"createsnappedgroup": 1, "createsnappedattrib": 1},
        ),
        ("near/closest_chain", chain_geometry, {"algorithm": 1}),
        ("topology/repeated_and_degenerate/no_cleanup", topology_geometry, {"deldegen": 0}),
        (
            "topology/repeated_and_degenerate/keep_degenerate_points",
            topology_geometry,
            {"deldegenpoints": 0},
        ),
        (
            "topology/repeated_and_degenerate/remove_all_unused",
            topology_geometry,
            {"delunusedpoints": 1},
        ),
        (
            "topology/repeated_and_degenerate/keep_fused",
            topology_geometry,
            {"keepconsolidatedpoints": 1},
        ),
    ]
    if matrix == "semantic":
        return semantic

    promotion = [
        ("near/no_position_write", chain_geometry, {"usepositionsnapmethod": 0}),
        (
            "near/explicit_groups/target_read_only",
            reducer_geometry,
            {"querygroup": "query", "usetargetgroup": 1, "targetgroup": "target"},
        ),
        (
            "near/explicit_groups/modify_both",
            reducer_geometry,
            {
                "querygroup": "query",
                "usetargetgroup": 1,
                "targetgroup": "target",
                "modifyboth": 1,
            },
        ),
        ("near/radius", reducer_geometry, {"useradiusattrib": 1, "radiusattrib": "weight"}),
        (
            "near/match/equal",
            reducer_geometry,
            {"usematchattrib": 1, "matchattrib": "ival", "matchtype": 0},
        ),
        (
            "near/match/unequal",
            reducer_geometry,
            {"usematchattrib": 1, "matchattrib": "ival", "matchtype": 1},
        ),
        (
            "specified/point",
            reducer_geometry,
            {"snaptype": 2, "targetptattrib": "snap_to", "targetclass": 0},
        ),
        ("two_input/default", two_input_geometry, {}),
        ("topology/normals/recompute", normals_geometry, {}),
    ]
    for method in range(14):
        promotion.append(
            (
                f"position_method/{method:02d}",
                reducer_geometry,
                {"positionsnapmethod": method, "positionsnapweightname": "weight"},
            )
        )
    for method in range(16):
        name = "label" if method in (10, 15) else "foo ival"
        promotion.append(
            (
                f"attribute_method/{method:02d}",
                reducer_geometry,
                {
                    "numpointattribs": (
                        {
                            "attribsnapmethod#": method,
                            "pointattribnames#": name,
                            "pointattribweightname#": "weight",
                        },
                    ),
                },
            )
        )
    for method in range(5):
        promotion.append(
            (
                f"group_method/{method:02d}",
                reducer_geometry,
                {
                    "numgroups": (
                        {
                            "grouppropagation#": method,
                            "pointgroupnames#": "odd ends",
                        },
                    ),
                },
            )
        )
    grid_cases = (
        ("grid/spacing/nearest", {"gridtype": 0, "gridspacing": (0.5, 0.5, 0.5)}),
        ("grid/spacing/down", {"gridtype": 0, "gridspacing": (0.5, 0.5, 0.5), "gridround": 1}),
        ("grid/spacing/up", {"gridtype": 0, "gridspacing": (0.5, 0.5, 0.5), "gridround": 2}),
        ("grid/lines", {"gridtype": 1, "gridlines": (4.0, 5.0, 8.0)}),
        ("grid/power_of_two", {"gridtype": 2, "gridpow2": (2, 3, 4)}),
        ("grid/offset", {"gridtype": 0, "gridspacing": (0.5, 0.5, 0.5), "gridoffset": (0.5, 0.5, 0.5)}),
        ("grid/tolerance", {"gridtype": 0, "gridspacing": (0.5, 0.5, 0.5), "gridtol": 0.05}),
    )
    for case_id, parameters in grid_cases:
        promotion.append(
            (
                case_id,
                grid_geometry,
                {"snaptype": 1, "consolidatesnappedpoints": 0, **parameters},
            )
        )
    return semantic + promotion


def capture_case(verb, defaults, case_id, builder, overrides):
    built = builder()
    sources = list(built) if isinstance(built, tuple) else [built]
    verb.setParms(defaults)
    if overrides:
        verb.setParms(overrides)
    if case_id.startswith("performance/"):
        warmup = hou.Geometry()
        verb.execute(warmup, sources)
        timings = []
        output = None
        for _ in range(7):
            output = hou.Geometry()
            started = time.perf_counter()
            verb.execute(output, sources)
            timings.append((time.perf_counter() - started) * 1000.0)
        timings.sort()
        return {
            "case_id": case_id,
            "parameters": overrides,
            "input": {"domains": {"point": {"count": len(sources[0].points())}}},
            "output": {"domains": {"point": {"count": len(output.points())}}},
            "performance": {
                "iterations": len(timings),
                "timings_ms": timings,
                "median_ms": timings[len(timings) // 2],
            },
        }
    output = hou.Geometry()
    verb.execute(output, sources)
    captured = {
        "case_id": case_id,
        "parameters": overrides,
        "input": {"domains": normalized_domains(sources[0])},
        "output": {"domains": normalized_domains(output)},
    }
    if len(sources) > 1:
        captured["inputs"] = [
            {"domains": normalized_domains(source)} for source in sources
        ]
    return captured


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--matrix",
        choices=("focused", "semantic", "promotion", "performance"),
        default="focused",
    )
    parser.add_argument("--output", required=True)
    arguments = parser.parse_args()
    verb = hou.sopNodeTypeCategory().nodeVerb(SUBJECT)
    if verb is None:
        raise RuntimeError("Houdini SOP verb fuse::2.0 is unavailable")
    defaults = verb.parms()
    payload = {
        "schema": "c3d.parity.capture.v1",
        "provider": {"id": "houdini", "version": hou.applicationVersionString()},
        "subject": {"kind": "sop", "id": SUBJECT},
        "matrix": arguments.matrix,
        "cases": [
            capture_case(verb, defaults, case_id, builder, overrides)
            for case_id, builder, overrides in case_registry(arguments.matrix)
        ],
        "provenance": {
            "command": "hython fuse_capture.py",
            "captured_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
            "executable": os.path.abspath(os.sys.executable),
        },
    }
    output = os.path.abspath(arguments.output)
    os.makedirs(os.path.dirname(output), exist_ok=True)
    with open(output, "w", encoding="utf-8") as handle:
        json.dump(payload, handle, indent=2, sort_keys=True)
        handle.write("\n")
    print(json.dumps({"output": output, "cases": len(payload["cases"])}, sort_keys=True))


if __name__ == "__main__":
    main()
