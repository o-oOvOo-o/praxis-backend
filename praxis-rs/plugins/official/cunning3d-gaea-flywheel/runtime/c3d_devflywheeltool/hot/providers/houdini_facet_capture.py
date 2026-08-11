import argparse
import datetime
import json
import os

import hou

from houdini_geometry_capture import geometry_domains


DEFAULT_PARAMETERS = {
    "group": "",
    "grouptype": 0,
    "prenml": 0,
    "unit": 0,
    "unique": 0,
    "cons": 0,
    "dist": 0.001,
    "accurate": 1,
    "inline": 0,
    "inlinedist": 0.001,
    "orientPolys": 0,
    "cusp": 0,
    "angle": 20.0,
    "remove": 0,
    "mkplanar": 0,
    "postnml": 0,
    "reversenml": 0,
}


def mesh_specs():
    hinge = {
        "points": ((0, 0, 0), (1, 0, 0), (1, 1, 0), (0, 1, 0), (1, 1, 1), (1, 0, 1)),
        "faces": ((0, 1, 2, 3), (1, 5, 4, 2)),
        "point_attributes": {
            "id": (0, 1, 2, 3, 4, 5),
            "weight": (0.0, 0.2, 0.4, 0.6, 0.8, 1.0),
            "N": ((0, 0, 2), (0, 0, 3), (0, 0, 4), (0, 0, 5), (2, 0, 0), (3, 0, 0)),
        },
        "point_groups": {"selected": (1, 2)},
        "primitive_attributes": {"material_id": (10, 20)},
        "primitive_groups": {"second": (1,)},
    }
    return {
        "hinge": hinge,
        "near": {
            "points": ((0, 0, 0), (1, 0, 0), (0, 1, 0), (0.0005, 0, 0), (1.0004, 0, 0), (0, 1.0003, 0)),
            "faces": ((0, 1, 2), (3, 4, 5)),
            "point_attributes": {
                "id": (0, 1, 2, 3, 4, 5),
                "weight": (0.0, 1.0, 2.0, 10.0, 11.0, 12.0),
                "N": ((2, 0, 0), (2, 0, 0), (2, 0, 0), (0, 4, 0), (0, 4, 0), (0, 4, 0)),
            },
            "point_groups": {"selected": (0, 1, 2)},
            "primitive_attributes": {"material_id": (10, 20)},
        },
        "inline": {
            "points": ((0, 0, 0), (1, 0, 0), (2, 0, 0), (2, 1, 0), (0, 1, 0)),
            "faces": ((0, 1, 2, 3, 4),),
            "point_attributes": {"id": (0, 1, 2, 3, 4)},
            "point_groups": {"selected": (1,)},
        },
        "degenerate": {
            "points": ((0, 0, 0), (1, 0, 0), (0, 1, 0), (2, 0, 0), (3, 0, 0), (4, 0, 0)),
            "faces": ((0, 1, 2), (0, 0, 1), (3, 4, 5)),
            "point_attributes": {"id": (0, 1, 2, 3, 4, 5)},
            "primitive_attributes": {"material_id": (10, 20, 30)},
            "primitive_groups": {"bad": (1, 2)},
        },
        "nonplanar": {
            "points": ((0, 0, 0), (2, 0, 0.2), (2, 2, -0.1), (0, 2, 0.3)),
            "faces": ((0, 1, 2, 3),),
            "point_attributes": {"id": (0, 1, 2, 3)},
        },
        "orient": {
            "points": ((0, 0, 0), (1, 0, 0), (1, 1, 0), (0, 1, 0), (2, 0, 0), (2, 1, 0)),
            "faces": ((0, 1, 2, 3), (1, 2, 5, 4)),
            "point_attributes": {"id": (0, 1, 2, 3, 4, 5)},
            "primitive_attributes": {"material_id": (10, 20)},
        },
        "threshold": {
            "points": (
                (0, 0, 0), (0.0009999, 0, 0),
                (10, 0, 0), (10.0010001, 0, 0),
                (20, 0, 0), (20.0007, 0.0007, 0),
                (30, 0, 0), (30.00071, 0.00071, 0),
                (40, 0, 0), (40.00075, 0, 0), (40.0015, 0, 0),
            ),
            "faces": ((0, 2, 4), (1, 3, 5), (6, 8, 10), (7, 9, 10)),
            "point_attributes": {
                "id": tuple(range(11)),
                "weight": tuple(float(index) for index in range(11)),
            },
        },
        "open": {
            "points": ((0, 0, 0), (1, 0, 0), (2, 0.5, 0), (3, 0, 0)),
            "faces": ((0, 1, 2, 3),),
            "closed": (False,),
            "point_attributes": {"id": (0, 1, 2, 3)},
        },
    }


def focused_cases():
    return (
        {"case_id": "focused/box_cusp_60", "source": "box", "overrides": {"cusp": 1, "angle": 60.0}},
        {"case_id": "focused/box_cusp_120", "source": "box", "overrides": {"cusp": 1, "angle": 120.0}},
        {"case_id": "focused/hinge_cusp_60", "source": "hinge", "overrides": {"cusp": 1, "angle": 60.0}},
        {"case_id": "focused/hinge_cusp_120", "source": "hinge", "overrides": {"cusp": 1, "angle": 120.0}},
    )


def semantic_cases():
    return focused_cases() + (
        {"case_id": "semantic/default_passthrough", "source": "hinge", "overrides": {}},
        {"case_id": "semantic/open_polygon_passthrough", "source": "open", "overrides": {}},
        {"case_id": "semantic/open_polygon_cusp_noop", "source": "open", "overrides": {"cusp": 1, "angle": 60.0}},
        {"case_id": "semantic/cusp_angle_zero", "source": "hinge", "overrides": {"cusp": 1, "angle": 0.0}},
        {"case_id": "semantic/cusp_angle_180", "source": "hinge", "overrides": {"cusp": 1, "angle": 180.0}},
        {"case_id": "semantic/unique_all", "source": "hinge", "overrides": {"unique": 1}},
        {"case_id": "semantic/consolidate_points_slow", "source": "near", "overrides": {"cons": 1, "dist": 0.001}},
        {"case_id": "semantic/consolidate_points_fast_accurate", "source": "near", "overrides": {"cons": 2, "dist": 0.001, "accurate": 1}},
        {"case_id": "semantic/consolidate_points_fast_legacy_distance", "source": "near", "overrides": {"cons": 2, "dist": 0.001, "accurate": 0}},
        {"case_id": "semantic/consolidate_points_zero_distance", "source": "near", "overrides": {"cons": 2, "dist": 0.0}},
        {"case_id": "semantic/consolidate_threshold_slow", "source": "threshold", "overrides": {"cons": 1, "dist": 0.001}},
        {"case_id": "semantic/consolidate_threshold_fast_accurate", "source": "threshold", "overrides": {"cons": 2, "dist": 0.001, "accurate": 1}},
        {"case_id": "semantic/consolidate_threshold_fast_legacy", "source": "threshold", "overrides": {"cons": 2, "dist": 0.001, "accurate": 0}},
        {"case_id": "semantic/consolidate_normals_slow", "source": "near", "overrides": {"cons": 3, "dist": 0.001}},
        {"case_id": "semantic/consolidate_normals_fast", "source": "near", "overrides": {"cons": 4, "dist": 0.001}},
        {"case_id": "semantic/remove_inline", "source": "inline", "overrides": {"inline": 1, "inlinedist": 0.001}},
        {"case_id": "semantic/remove_inline_zero_distance", "source": "inline", "overrides": {"inline": 1, "inlinedist": 0.0}},
        {"case_id": "semantic/orient_polygons", "source": "orient", "overrides": {"orientPolys": 1}},
        {"case_id": "semantic/remove_degenerate", "source": "degenerate", "overrides": {"remove": 1}},
        {"case_id": "semantic/make_planar", "source": "nonplanar", "overrides": {"mkplanar": 1}},
        {"case_id": "semantic/pre_compute_normals", "source": "hinge", "overrides": {"prenml": 1}},
        {"case_id": "semantic/unit_normals", "source": "hinge", "overrides": {"unit": 1}},
        {"case_id": "semantic/unit_missing_normals", "source": "nonplanar", "overrides": {"unit": 1}},
        {"case_id": "semantic/post_compute_normals", "source": "hinge", "overrides": {"postnml": 1}},
        {"case_id": "semantic/reverse_precomputed_normals", "source": "hinge", "overrides": {"prenml": 1, "reversenml": 1}},
        {"case_id": "semantic/reverse_missing_normals", "source": "nonplanar", "overrides": {"reversenml": 1}},
        {"case_id": "semantic/pre_normals_then_unique", "source": "hinge", "overrides": {"prenml": 1, "unique": 1}},
        {"case_id": "semantic/unique_then_post_normals", "source": "hinge", "overrides": {"unique": 1, "postnml": 1}},
        {"case_id": "semantic/group_points_unique", "source": "hinge", "overrides": {"group": "selected", "grouptype": 1, "unique": 1}},
        {"case_id": "semantic/group_point_range_unique", "source": "hinge", "overrides": {"group": "1-2", "grouptype": 1, "unique": 1}},
        {"case_id": "semantic/group_primitives_cusp", "source": "hinge", "overrides": {"group": "second", "grouptype": 2, "cusp": 1, "angle": 60.0}},
        {"case_id": "semantic/group_primitive_range_cusp", "source": "hinge", "overrides": {"group": "1", "grouptype": 2, "cusp": 1, "angle": 60.0}},
        {"case_id": "semantic/missing_group_noop", "source": "hinge", "overrides": {"group": "missing", "grouptype": 1, "unique": 1}},
        {"case_id": "semantic/combined_cleanup", "source": "near", "overrides": {"cons": 2, "dist": 0.001, "remove": 1, "postnml": 1}},
    )


def create_mesh_source(container, spec):
    node = container.createNode("python")
    node.parm("python").set(
        "geo = hou.pwd().geometry()\n"
        "spec = {}\n".format(repr(spec))
        + "points = []\n"
        + "for position in spec['points']:\n"
        + "    point = geo.createPoint()\n"
        + "    point.setPosition(position)\n"
        + "    points.append(point)\n"
        + "primitives = []\n"
        + "for face_index, indices in enumerate(spec['faces']):\n"
        + "    polygon = geo.createPolygon()\n"
        + "    for index in indices:\n"
        + "        polygon.addVertex(points[index])\n"
        + "    closed_values = spec.get('closed', ())\n"
        + "    polygon.setIsClosed(closed_values[face_index] if face_index < len(closed_values) else True)\n"
        + "    primitives.append(polygon)\n"
        + "vertices = [vertex for primitive in primitives for vertex in primitive.vertices()]\n"
        + "for name, values in spec.get('point_attributes', {}).items():\n"
        + "    default_value = tuple(float(component) for component in values[0]) if name == 'N' else values[0]\n"
        + "    attribute = geo.addAttrib(hou.attribType.Point, name, default_value, transform_as_normal=(name == 'N'))\n"
        + "    for point, value in zip(points, values):\n"
        + "        point.setAttribValue(attribute, tuple(float(component) for component in value) if name == 'N' else value)\n"
        + "for name, values in spec.get('primitive_attributes', {}).items():\n"
        + "    attribute = geo.addAttrib(hou.attribType.Prim, name, values[0])\n"
        + "    for primitive, value in zip(primitives, values):\n"
        + "        primitive.setAttribValue(attribute, value)\n"
        + "vertex_id = geo.addAttrib(hou.attribType.Vertex, 'vertex_id', 0)\n"
        + "vertex_uv = geo.addAttrib(hou.attribType.Vertex, 'uv_probe', (0.0, 0.0))\n"
        + "for index, vertex in enumerate(vertices):\n"
        + "    vertex.setAttribValue(vertex_id, index)\n"
        + "    vertex.setAttribValue(vertex_uv, (float(index), float(index) + 0.25))\n"
        + "geo.addAttrib(hou.attribType.Global, 'source_tag', 77)\n"
        + "geo.setGlobalAttribValue('source_tag', 77)\n"
        + "for name, members in spec.get('point_groups', {}).items():\n"
        + "    group = geo.createPointGroup(name)\n"
        + "    group.add([points[index] for index in members])\n"
        + "for name, members in spec.get('primitive_groups', {}).items():\n"
        + "    group = geo.createPrimGroup(name)\n"
        + "    group.add([primitives[index] for index in members])\n"
    )
    return node


def create_source(container, kind):
    if kind == "box":
        node = container.createNode("box")
        node.parmTuple("size").set((2.0, 3.0, 4.0))
        node.parmTuple("t").set((0.5, -1.0, 2.0))
        return node
    return create_mesh_source(container, mesh_specs()[kind])


def parameters(overrides):
    values = DEFAULT_PARAMETERS.copy()
    values.update(overrides)
    return values


def configure_facet(node, values):
    for name, value in values.items():
        node.parm(name).set(value)


def capture_case(root, container, case, profile):
    source = create_source(container, case["source"])
    values = parameters(case["overrides"])
    facet = container.createNode("facet")
    facet.setInput(0, source)
    configure_facet(facet, values)
    try:
        facet.cook(force=True)
    except hou.OperationFailed as error:
        details = "; ".join(facet.errors())
        raise RuntimeError("Facet case {} failed: {}".format(case["case_id"], details)) from error
    slug = case["case_id"].replace("/", "_")
    source_geometry = source.geometry()
    captured = {
        "case_id": case["case_id"],
        "parameters": values,
        "provider_overrides": values,
        "input": {"domains": geometry_domains(root, slug, "input", source_geometry)},
        "output": {"domains": geometry_domains(root, slug, "output", facet.geometry())},
    }
    if profile == "focused":
        captured["discovery"] = {
            "point_vertex_primitive_order": [
                [vertex.prim().number() for vertex in point.vertices()]
                for point in source_geometry.points()
            ]
        }
    return captured


def parameter_contract(container):
    node = container.createNode("facet")
    return {
        parameter_tuple.name(): {
            "label": template.label(),
            "type": str(template.type()).split(".")[-1],
            "size": len(parameter_tuple),
            "default": list(parameter_tuple.eval()),
            "menu_items": list(template.menuItems()) if hasattr(template, "menuItems") else [],
            "menu_labels": list(template.menuLabels()) if hasattr(template, "menuLabels") else [],
        }
        for parameter_tuple in node.parmTuples()
        for template in (parameter_tuple.parmTemplate(),)
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--matrix", choices=("focused", "semantic"), default="focused")
    parser.add_argument("--output", required=True)
    arguments = parser.parse_args()
    output = os.path.abspath(arguments.output)
    root = os.path.dirname(output)
    container = hou.node("/obj").createNode("geo", run_init_scripts=False)
    try:
        selected_cases = focused_cases() if arguments.matrix == "focused" else semantic_cases()
        captured_cases = [capture_case(root, container, case, arguments.matrix) for case in selected_cases]
        contract = parameter_contract(container)
    finally:
        container.destroy()
    payload = {
        "schema": "c3d.parity.capture.v1",
        "provider": {"id": "houdini", "version": hou.applicationVersionString(), "execution_mode": "headless_node_network"},
        "subject": {"kind": "sop", "id": "facet"},
        "profile": arguments.matrix,
        "parameter_contract": contract,
        "cases": captured_cases,
        "provenance": {
            "command": "hython houdini_facet_capture.py",
            "captured_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
            "executable": os.path.abspath(os.sys.executable),
        },
    }
    os.makedirs(root, exist_ok=True)
    with open(output, "w", encoding="utf-8") as handle:
        json.dump(payload, handle, indent=2, sort_keys=True)
        handle.write("\n")
    print(json.dumps({"output": output, "subject": "facet", "profile": arguments.matrix, "cases": len(captured_cases)}, sort_keys=True))


if __name__ == "__main__":
    main()
