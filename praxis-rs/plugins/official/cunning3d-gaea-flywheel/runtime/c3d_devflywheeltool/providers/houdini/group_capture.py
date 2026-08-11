import argparse
import ctypes
import datetime
import json
import os
import threading
import time

import hou


SUBJECTS = {
    "range": "grouprange",
    "expand": "groupexpand",
    "find_path": "groupfindpath",
    "promote": "grouppromote",
    "blast": "blast",
}


if os.name == "nt":
    class ProcessMemoryCounters(ctypes.Structure):
        _fields_ = [
            ("cb", ctypes.c_ulong),
            ("page_fault_count", ctypes.c_ulong),
            ("peak_working_set_size", ctypes.c_size_t),
            ("working_set_size", ctypes.c_size_t),
            ("quota_peak_paged_pool_usage", ctypes.c_size_t),
            ("quota_paged_pool_usage", ctypes.c_size_t),
            ("quota_peak_non_paged_pool_usage", ctypes.c_size_t),
            ("quota_non_paged_pool_usage", ctypes.c_size_t),
            ("pagefile_usage", ctypes.c_size_t),
            ("peak_pagefile_usage", ctypes.c_size_t),
        ]
    GET_CURRENT_PROCESS = ctypes.windll.kernel32.GetCurrentProcess
    GET_CURRENT_PROCESS.argtypes = []
    GET_CURRENT_PROCESS.restype = ctypes.c_void_p
    GET_PROCESS_MEMORY_INFO = ctypes.windll.psapi.GetProcessMemoryInfo
    GET_PROCESS_MEMORY_INFO.argtypes = [
        ctypes.c_void_p,
        ctypes.POINTER(ProcessMemoryCounters),
        ctypes.c_ulong,
    ]
    GET_PROCESS_MEMORY_INFO.restype = ctypes.c_int


def process_memory_counters():
    if os.name == "nt":
        counters = ProcessMemoryCounters()
        counters.cb = ctypes.sizeof(counters)
        process = GET_CURRENT_PROCESS()
        if not GET_PROCESS_MEMORY_INFO(
            process, ctypes.byref(counters), counters.cb
        ):
            raise ctypes.WinError()
        return counters
    return None


def working_set_bytes():
    counters = process_memory_counters()
    if counters is not None:
        return counters.working_set_size
    with open("/proc/self/status", "r", encoding="utf-8") as stream:
        for line in stream:
            if line.startswith("VmRSS:"):
                return int(line.split()[1]) * 1024
    return 0


def peak_working_set_bytes():
    counters = process_memory_counters()
    if counters is not None:
        return counters.peak_working_set_size
    import resource
    peak = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
    return peak if os.sys.platform == "darwin" else peak * 1024


class WorkingSetSampler:
    def __init__(self):
        self.peak = 0
        self._error = None
        self._stop = threading.Event()
        self._ready = threading.Event()
        self._thread = threading.Thread(target=self._run, name="c3d-group-memory-sampler", daemon=True)

    def _run(self):
        try:
            self.peak = working_set_bytes()
            self._ready.set()
            while not self._stop.is_set():
                self.peak = max(self.peak, working_set_bytes())
                self._stop.wait(0.001)
            self.peak = max(self.peak, working_set_bytes())
        except BaseException as error:
            self._error = error
            self._ready.set()

    def start(self):
        self._thread.start()
        self._ready.wait()
        if self._error is not None:
            raise self._error
        return self

    def reset(self, baseline):
        self.peak = baseline

    def stop(self):
        self._stop.set()
        self._thread.join()
        if self._error is not None:
            raise self._error
        return self.peak


def json_value(value):
    if isinstance(value, (hou.Vector2, hou.Vector3, hou.Vector4, hou.Quaternion)):
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


def topology_edges(geometry, primitives, points):
    result = []
    seen = set()
    for primitive in primitives:
        vertices = list(primitive.vertices())
        limit = len(vertices) if primitive.intrinsicValue("closed") else len(vertices) - 1
        for index in range(max(0, limit)):
            a = vertices[index].point().number()
            b = vertices[(index + 1) % len(vertices)].point().number()
            key = (min(a, b), max(a, b))
            if key not in seen:
                seen.add(key)
                result.append((key, geometry.findEdge(points[a], points[b])))
    return result


def group_payload(groups, elements, identity, ordered_entries):
    payload = {}
    index = {identity(element): offset for offset, element in enumerate(elements)}
    for group in sorted(groups, key=lambda value: value.name()):
        members = [offset for offset, element in enumerate(elements) if group.contains(element)]
        ordered = bool(group.isOrdered()) if hasattr(group, "isOrdered") else False
        ordered_members = []
        if ordered:
            # HOM exposes membership with domain-specific accessors in H22
            # (PointGroup.points, VertexGroup.vertices, EdgeGroup.edges,
            # PrimGroup.prims).  Older builds may additionally expose the
            # generic entries() method, but relying on it makes the oracle
            # adapter fail before the first case on Houdini 22.
            entries = group.entries() if hasattr(group, "entries") else ordered_entries(group)
            ordered_members = [index[identity(entry)] for entry in entries if identity(entry) in index]
        payload[group.name()] = {
            "members": members,
            "ordered": ordered,
            "ordered_members": ordered_members,
        }
    return payload


def normalized_domains(geometry):
    points = list(geometry.points())
    primitives = list(geometry.prims())
    vertices = [vertex for primitive in primitives for vertex in primitive.vertices()]
    edge_records = topology_edges(geometry, primitives, points)
    edges = [edge for _, edge in edge_records if edge is not None]
    edge_keys = [key for key, edge in edge_records if edge is not None]
    return {
        "point": {
            "count": len(points),
            "positions": [list(point.position()) for point in points],
            "attributes": {
                attribute.name(): attribute_payload(points, attribute)
                for attribute in sorted(geometry.pointAttribs(), key=lambda value: value.name())
            },
            "groups": group_payload(
                geometry.pointGroups(), points, lambda point: point.number(), lambda group: group.points()
            ),
        },
        "vertex": {
            "count": len(vertices),
            "point_indices": [vertex.point().number() for vertex in vertices],
            "attributes": {
                attribute.name(): attribute_payload(vertices, attribute)
                for attribute in sorted(geometry.vertexAttribs(), key=lambda value: value.name())
            },
            "groups": group_payload(
                geometry.vertexGroups(),
                vertices,
                lambda vertex: vertex.linearNumber(),
                lambda group: group.vertices(),
            ),
        },
        "edge": {
            "count": len(edges),
            "point_pairs": edge_keys,
            "groups": group_payload(
                geometry.edgeGroups(),
                edges,
                lambda edge: tuple(sorted(point.number() for point in edge.points())),
                lambda group: group.edges(),
            ),
        },
        "primitive": {
            "count": len(primitives),
            "vertex_counts": [len(primitive.vertices()) for primitive in primitives],
            "point_loops": [[point.number() for point in primitive.points()] for primitive in primitives],
            "closed": [bool(primitive.intrinsicValue("closed")) for primitive in primitives],
            "attributes": {
                attribute.name(): attribute_payload(primitives, attribute)
                for attribute in sorted(geometry.primAttribs(), key=lambda value: value.name())
            },
            "groups": group_payload(
                geometry.primGroups(),
                primitives,
                lambda primitive: primitive.number(),
                lambda group: group.prims(),
            ),
        },
    }


def discrete_buffer_serialization(domains):
    def integer_attributes(domain):
        return {
            name: payload
            for name, payload in domain.get("attributes", {}).items()
            if payload.get("storage") == "Int"
        }

    contract = {
        "point": {
            "count": domains["point"]["count"],
            "integer_attributes": integer_attributes(domains["point"]),
            "groups": domains["point"]["groups"],
        },
        "vertex": {
            "count": domains["vertex"]["count"],
            "point_indices": domains["vertex"]["point_indices"],
            "integer_attributes": integer_attributes(domains["vertex"]),
            "groups": domains["vertex"]["groups"],
        },
        "edge": {
            "count": domains["edge"]["count"],
            "point_pairs": domains["edge"]["point_pairs"],
            "groups": domains["edge"]["groups"],
        },
        "primitive": {
            "count": domains["primitive"]["count"],
            "vertex_counts": domains["primitive"]["vertex_counts"],
            "point_loops": domains["primitive"]["point_loops"],
            "closed": domains["primitive"]["closed"],
            "integer_attributes": integer_attributes(domains["primitive"]),
            "groups": domains["primitive"]["groups"],
        },
    }
    return json.dumps(contract, ensure_ascii=False, separators=(",", ":"), sort_keys=True)


def add_polygon(geometry, points, indices, closed=True):
    primitive = geometry.createPolygon()
    primitive.setIsClosed(closed)
    for index in indices:
        primitive.addVertex(points[index])
    return primitive


def matrix_geometry():
    geometry = hou.Geometry()
    points = []
    for position in (
        (0, 0, 0), (1, 0, 0), (2, 0, 0),
        (0, 1, 0), (1, 1, 0), (2, 1, 0),
        (0, 2, 0), (1, 2, 0), (2, 2, 0),
        (4, 0, 0), (5, 0, 0), (4, 1, 0),
    ):
        point = geometry.createPoint()
        point.setPosition(position)
        points.append(point)
    for loop in ((0, 1, 4, 3), (1, 2, 5, 4), (3, 4, 7, 6), (4, 5, 8, 7), (9, 10, 11)):
        add_polygon(geometry, points, loop)
    piece = geometry.addAttrib(hou.attribType.Point, "piece", 0)
    for index, point in enumerate(points):
        point.setAttribValue(piece, 0 if index < 9 else 1)
    uv = geometry.addAttrib(hou.attribType.Vertex, "uv", (0.0, 0.0, 0.0))
    for primitive in geometry.prims():
        for vertex in primitive.vertices():
            point = vertex.point().position()
            seam = 10.0 if primitive.number() in (1, 3) else 0.0
            vertex.setAttribValue(uv, (point[0] + seam, point[1], 0.0))
    point_seed = geometry.createPointGroup("point_seed")
    point_seed.add(points[0])
    point_pair = geometry.createPointGroup("point_pair")
    point_pair.add(points[0])
    point_pair.add(points[1])
    point_scattered = geometry.createPointGroup("point_scattered")
    point_scattered.add(points[0])
    point_scattered.add(points[4])
    controls = geometry.createPointGroup("controls", is_ordered=True)
    for index in (0, 4, 8):
        controls.add(points[index])
    range_controls = geometry.createPointGroup("range_controls", is_ordered=True)
    for index in (8, 0, 4):
        range_controls.add(points[index])
    range_region_controls = geometry.createPointGroup("range_region_controls", is_ordered=True)
    for index in (9, 10, 11, 0, 1, 4):
        range_region_controls.add(points[index])
    primitive_seed = geometry.createPrimGroup("primitive_seed")
    primitive_seed.add(geometry.prims()[0])
    primitive_controls = geometry.createPrimGroup("primitive_controls", is_ordered=True)
    primitive_controls.add(geometry.prims()[0])
    primitive_controls.add(geometry.prims()[3])
    vertex_seed = geometry.createVertexGroup("vertex_seed")
    vertex_seed.add(geometry.prims()[0].vertices()[0])
    vertex_controls = geometry.createVertexGroup("vertex_controls", is_ordered=True)
    vertex_controls.add(geometry.prims()[0].vertices()[0])
    vertex_controls.add(geometry.prims()[3].vertices()[2])
    edge_wall = geometry.createEdgeGroup("edge_wall")
    edge_wall.add(geometry.findEdge(points[1], points[4]))
    primitive_wall = geometry.createPrimGroup("primitive_wall")
    primitive_wall.add(geometry.prims()[1])
    return geometry


def bridge_geometry():
    geometry = hou.Geometry()
    points = []
    for position in ((0, 0, 0), (1, 0, 0), (2, 0, 0), (1, 1, 0)):
        point = geometry.createPoint()
        point.setPosition(position)
        points.append(point)
    add_polygon(geometry, points, (0, 1, 2, 1, 3))
    selected = geometry.createPointGroup("selected")
    selected.add(points)
    return geometry


def nonmanifold_geometry():
    geometry = hou.Geometry()
    points = []
    for position in ((0, 0, 0), (1, 0, 0), (0.5, 1, 0), (0.5, -1, 0), (0.5, 0, 0)):
        point = geometry.createPoint()
        point.setPosition(position)
        points.append(point)
    for loop in ((0, 1, 2), (1, 0, 3), (0, 1, 4)):
        add_polygon(geometry, points, loop)
    selected = geometry.createPrimGroup("primitive_seed")
    selected.add(geometry.prims()[0])
    return geometry


def normal_constraint_geometry():
    geometry = hou.Geometry()
    points = []
    for position in ((0, 0, 0), (1, 0, 0), (0.5, 1, 0), (0.5, -1, 0)):
        point = geometry.createPoint()
        point.setPosition(position)
        points.append(point)
    add_polygon(geometry, points, (0, 1, 2))
    add_polygon(geometry, points, (1, 0, 3))
    normal = geometry.addAttrib(hou.attribType.Prim, "N", (0.0, 0.0, 1.0))
    weight = geometry.addAttrib(hou.attribType.Prim, "weight", 0.0)
    geometry.prims()[0].setAttribValue(normal, (0.0, 0.0, 1.0))
    geometry.prims()[1].setAttribValue(normal, (1.0, 0.0, 0.0))
    geometry.prims()[0].setAttribValue(weight, 0.0)
    geometry.prims()[1].setAttribValue(weight, 0.05)
    primitive_seed = geometry.createPrimGroup("primitive_seed")
    primitive_seed.add(geometry.prims()[0])
    return geometry


def attribute_boundary_geometry():
    geometry = hou.Geometry()
    positions = (
        (0, 0, 0), (1, 0, 0), (0.5, 1, 0),
        (0.5, -1, 0), (-1, 0, 0), (-0.5, 1, 0),
    )
    points = []
    for position in positions:
        point = geometry.createPoint()
        point.setPosition(position)
        points.append(point)
    loops = ((0, 1, 2), (1, 0, 3), (0, 4, 5))
    for loop in loops:
        add_polygon(geometry, points, loop)
    uv = geometry.addAttrib(hou.attribType.Vertex, "uv", (0.0, 0.0, 0.0))
    for primitive in geometry.prims():
        seam = 10.0 if primitive.number() == 1 else 0.0
        for vertex in primitive.vertices():
            position = vertex.point().position()
            vertex.setAttribValue(uv, (position[0] + seam, position[1], 0.0))
    selected = geometry.createPrimGroup("primitive_all")
    selected.add(geometry.prims())
    return geometry


def performance_geometry(size):
    geometry = hou.Geometry()
    points = []
    for y in range(size + 1):
        for x in range(size + 1):
            point = geometry.createPoint()
            point.setPosition((x, y, 0))
            points.append(point)
    stride = size + 1
    for y in range(size):
        for x in range(size):
            i = y * stride + x
            add_polygon(geometry, points, (i, i + 1, i + stride + 1, i + stride))
    seed = geometry.createPointGroup("point_seed")
    seed.add(points[0])
    controls = geometry.createPointGroup("controls", is_ordered=True)
    controls.add(points[0])
    controls.add(points[-1])
    primitive_seed = geometry.createPrimGroup("primitive_seed")
    primitive_seed.add(geometry.prims()[0])
    return geometry


def path_tie_geometry(axis, control_indices):
    geometry = performance_geometry(axis - 1)
    controls = geometry.findPointGroup("controls")
    controls.clear()
    for index in control_indices:
        controls.add(geometry.points()[index])
    return geometry


def cases(matrix):
    focused = [
        ("range", "range/primitive/relative", matrix_geometry, {"numrange": ({"groupname#": "out", "group#": "primitive_seed", "grouptype#": 1},)}),
        ("range", "range/cross_domain_collision", matrix_geometry, {"numrange": ({"groupname#": "out", "grouptype#": 0, "method#": 0, "start#": 0, "end#": 0, "usecolgroup#": 1, "colgroup#": "edge_wall", "colgrouptype#": 0},)}),
        ("expand", "expand/point/cross_domain_collision", matrix_geometry, {"outputgroup": "out", "group": "point_seed", "grouptype": 3, "numsteps": 8, "usecolgroup": 1, "colgroup": "edge_wall", "colgrouptype": 1}),
        ("expand", "expand/primitive/uv_boundary", matrix_geometry, {"outputgroup": "out", "group": "primitive_seed", "grouptype": 4, "numsteps": 8, "useconnectivityattrib": 1, "connectivityattrib": "uv"}),
        ("find_path", "path/ordered_controls", matrix_geometry, {"outgroup": "out", "group": "controls", "grouptype": 2}),
        ("find_path", "path/cross_domain_collision", matrix_geometry, {"outgroup": "out", "group": "controls", "grouptype": 2, "usecolgroup": 1, "colgroup": "primitive_wall", "colgrouptype": 3}),
        ("promote", "promote/point_to_edge", matrix_geometry, {"promotions": ({"fromtype#": 2, "totype#": 2, "group#": "point_seed", "newname#": "out"},)}),
        ("promote", "promote/remove_degenerate_bridges", bridge_geometry, {"promotions": ({"fromtype#": 2, "totype#": 2, "group#": "selected", "newname#": "out", "removedegen#": 1},)}),
    ]
    if matrix == "focused":
        return focused
    semantic = list(focused)
    for group_type, group in ((1, "vertex_seed"), (2, "edge_wall"), (3, "point_seed"), (4, "primitive_seed")):
        semantic.append(("expand", "expand/domain/%d" % group_type, matrix_geometry, {"outputgroup": "out", "group": group, "grouptype": group_type, "numsteps": 1}))
    for style in range(3):
        semantic.append(("find_path", "path/edge_style/%d" % style, matrix_geometry, {"outgroup": "out", "group": "edge_wall", "grouptype": 1, "pathcontroltype": 1, "edgestyle": style}))
    semantic.extend([
        ("range", "range/ordered_base", matrix_geometry, {"numrange": ({"groupname#": "out", "group#": "range_controls", "grouptype#": 0, "method#": 0, "start#": 0, "end#": 1},)}),
        ("range", "range/ordered_connected_region", matrix_geometry, {"numrange": ({"groupname#": "out", "group#": "range_region_controls", "grouptype#": 0, "method#": 0, "start#": 0, "end#": 0, "connectedgeo#": 1, "usepartnum#": 1, "partnum#": 0, "keeponlypartnum#": 1},)}),
        ("range", "range/ordered_merge_destination", matrix_geometry, {"numrange": (
            {"groupname#": "out", "grouptype#": 0, "method#": 0, "start#": 3, "end#": 3},
            {"groupname#": "out", "group#": "range_controls", "grouptype#": 0, "mergeop#": 1, "method#": 0, "start#": 0, "end#": 1},
        )}),
        ("range", "range/domain/vertex", matrix_geometry, {"numrange": ({"groupname#": "out", "grouptype#": 2, "method#": 0, "start#": 0, "end#": 3},)}),
        ("range", "range/mode/start_length", matrix_geometry, {"numrange": ({"groupname#": "out", "grouptype#": 0, "method#": 2, "start#": 2, "length#": 3},)}),
        ("range", "range/mode/equal_partitions", matrix_geometry, {"numrange": ({"groupname#": "out", "grouptype#": 0, "method#": 3, "partition#": 1, "numpartition#": 3},)}),
        ("range", "range/invert_n_of_m_offset", matrix_geometry, {"numrange": ({"groupname#": "out", "grouptype#": 0, "method#": 0, "start#": 1, "end#": 8, "invert#": 1, "selectamount#": 2, "selecttotal#": 3, "selectoffset#": -1},)}),
        ("expand", "expand/step_attribute", matrix_geometry, {"outputgroup": "out", "group": "point_seed", "grouptype": 3, "numsteps": 2, "usestepattrib": 1, "stepattrib": "hop"}),
        ("find_path", "path/domain/primitive", matrix_geometry, {"outgroup": "out", "group": "primitive_controls", "grouptype": 3}),
        ("find_path", "path/domain/vertex", matrix_geometry, {"outgroup": "out", "group": "vertex_controls", "grouptype": 4}),
        ("find_path", "path/uv_attribute", matrix_geometry, {"outgroup": "out", "group": "controls", "grouptype": 2, "useuvattrib": 1, "uvattrib": "uv"}),
        ("promote", "promote/boundary/unshared", matrix_geometry, {"promotions": ({"fromtype#": 1, "totype#": 2, "group#": "primitive_seed", "newname#": "out", "onlyboundary#": 1, "includeunshared#": 1},)}),
        ("promote", "promote/boundary/connectivity_uv", matrix_geometry, {"promotions": ({"fromtype#": 1, "totype#": 2, "group#": "primitive_seed", "newname#": "out", "onlyboundary#": 1, "useconnectivityattrib#": 1, "connectivityattrib#": "uv"},)}),
        ("promote", "promote/output_attribute", matrix_geometry, {"promotions": ({"fromtype#": 1, "totype#": 1, "group#": "primitive_seed", "newname#": "out", "preserve#": 1, "toattrib#": 1},)}),
        ("promote", "promote/boundary/nonmanifold", nonmanifold_geometry, {"promotions": ({"fromtype#": 1, "totype#": 2, "group#": "primitive_seed", "newname#": "out", "onlyboundary#": 1, "includeunshared#": 1},)}),
        ("promote", "promote/boundary/attribute_points", attribute_boundary_geometry, {"promotions": ({"fromtype#": 1, "totype#": 0, "group#": "primitive_all", "newname#": "out", "onlyboundary#": 1, "includeunshared#": 0, "includecurveunshared#": 0, "useconnectivityattrib#": 1, "connectivityattrib#": "uv", "primsbyattribbndpts#": 1},)}),
        ("promote", "promote/containment/all", matrix_geometry, {"promotions": ({"fromtype#": 2, "totype#": 0, "group#": "point_pair", "newname#": "out", "onlyfull#": 1},)}),
        ("promote", "promote/containment/sharing_edge", matrix_geometry, {"promotions": ({"fromtype#": 2, "totype#": 0, "group#": "point_scattered", "newname#": "out", "onlyprimsedge#": 1},)}),
    ])
    for source_type in range(1, 5):
        for target_type in range(4):
            source_group = ("primitive_seed", "point_seed", "edge_wall", "vertex_seed")[source_type - 1]
            semantic.append(("promote", "promote/%d_to_%d" % (source_type, target_type), matrix_geometry, {"promotions": ({"fromtype#": source_type, "totype#": target_type, "group#": source_group, "newname#": "out", "preserve#": 1},)}))
    semantic.extend([
        ("range", "range/connectivity_attribute", matrix_geometry, {"numrange": ({"groupname#": "out", "grouptype#": 0, "method#": 1, "selectamount#": 1, "selecttotal#": 2, "useattrib#": 1, "attrib#": "piece"},)}),
        ("range", "range/connected_region", matrix_geometry, {"numrange": ({"groupname#": "out", "grouptype#": 0, "connectedgeo#": 1, "usepartnum#": 1, "partnum#": 1, "keeponlypartnum#": 1},)}),
        ("expand", "expand/negative_steps", matrix_geometry, {"outputgroup": "out", "group": "point_seed", "grouptype": 3, "numsteps": -1}),
        ("expand", "expand/flood", matrix_geometry, {"outputgroup": "out", "group": "point_seed", "grouptype": 3, "floodfill": 1}),
        ("find_path", "path/mode/pairs", matrix_geometry, {"outgroup": "out", "group": "controls", "grouptype": 2, "pathcontroltype": 2}),
        ("find_path", "path/mode/pairs_close", matrix_geometry, {"outgroup": "out", "group": "controls", "grouptype": 2, "pathcontroltype": 2, "operation": 2}),
        ("find_path", "path/ending/extend", matrix_geometry, {"outgroup": "out", "group": "controls", "grouptype": 2, "operation": 1}),
        ("find_path", "path/ending/close", matrix_geometry, {"outgroup": "out", "group": "controls", "grouptype": 2, "operation": 2}),
        ("range", "range/collision/exclude_boundary", matrix_geometry, {"numrange": ({"groupname#": "out", "grouptype#": 0, "method#": 0, "start#": 0, "end#": 0, "usecolgroup#": 1, "colgroup#": "edge_wall", "colgrouptype#": 0, "colallowonbnd#": 0},)}),
        ("expand", "expand/collision/allow_boundary", matrix_geometry, {"outputgroup": "out", "group": "point_seed", "grouptype": 3, "numsteps": 8, "usecolgroup": 1, "colgroup": "edge_wall", "colgrouptype": 1, "colgroupallowonbound": 1}),
        ("expand", "expand/collision/contain", matrix_geometry, {"outputgroup": "out", "group": "point_seed", "grouptype": 3, "numsteps": 8, "usecolgroup": 1, "colgroup": "primitive_wall", "colgrouptype": 3, "colgroupinvert": 1}),
        ("find_path", "path/collision/allow_boundary", matrix_geometry, {"outgroup": "out", "group": "controls", "grouptype": 2, "usecolgroup": 1, "colgroup": "edge_wall", "colgrouptype": 1, "colgrouponbnd": 1}),
        ("find_path", "path/collision/contain", matrix_geometry, {"outgroup": "out", "group": "controls", "grouptype": 2, "usecolgroup": 1, "colgroup": "primitive_wall", "colgrouptype": 3, "colgroupinvert": 1}),
        ("range", "range/multiparm/two_rules", matrix_geometry, {"numrange": (
            {"groupname#": "out_a", "grouptype#": 0, "selectamount#": 1, "selecttotal#": 2},
            {"groupname#": "out_b", "grouptype#": 0, "selectamount#": 1, "selecttotal#": 2, "selectoffset#": 1},
        )}),
        ("find_path", "path/tie/grid3_diagonal", lambda: path_tie_geometry(3, (0, 8)), {"outgroup": "out", "group": "controls", "grouptype": 2}),
        ("find_path", "path/tie/grid3_segments", lambda: path_tie_geometry(3, (0, 4, 8)), {"outgroup": "out", "group": "controls", "grouptype": 2}),
        ("find_path", "path/tie/grid4_diagonal", lambda: path_tie_geometry(4, (0, 15)), {"outgroup": "out", "group": "controls", "grouptype": 2}),
        ("find_path", "path/tie/grid5_diagonal", lambda: path_tie_geometry(5, (0, 24)), {"outgroup": "out", "group": "controls", "grouptype": 2}),
        ("find_path", "path/tie/grid4_reverse", lambda: path_tie_geometry(4, (15, 0)), {"outgroup": "out", "group": "controls", "grouptype": 2}),
        ("range", "range/merge/intersect_subtract", matrix_geometry, {"numrange": (
            {"groupname#": "out_intersect", "grouptype#": 0},
            {"groupname#": "out_intersect", "group#": "range_controls", "grouptype#": 0, "mergeop#": 2, "method#": 0, "start#": 0, "end#": 1},
            {"groupname#": "out_subtract", "grouptype#": 0},
            {"groupname#": "out_subtract", "group#": "range_controls", "grouptype#": 0, "mergeop#": 3, "method#": 0, "start#": 0, "end#": 1},
        )}),
        ("range", "range/disabled_rule", matrix_geometry, {"numrange": ({"enable#": 0, "groupname#": "disabled_out", "grouptype#": 0},)}),
        ("expand", "expand/primitive/share_edge", matrix_geometry, {"outputgroup": "out", "group": "primitive_seed", "grouptype": 4, "numsteps": 1, "primshareedge": 1}),
        ("expand", "expand/primitive/normal_constraint", normal_constraint_geometry, {"outputgroup": "out", "group": "primitive_seed", "grouptype": 4, "numsteps": 1, "bynormal": 1, "normalangle": 30.0, "overridenormal": 1, "normalattrib": "N"}),
        ("expand", "expand/connectivity/tolerance_inside", normal_constraint_geometry, {"outputgroup": "out", "group": "primitive_seed", "grouptype": 4, "numsteps": 1, "useconnectivityattrib": 1, "connectivityattrib": "weight", "tol": 0.1}),
        ("expand", "expand/connectivity/tolerance_outside", normal_constraint_geometry, {"outputgroup": "out", "group": "primitive_seed", "grouptype": 4, "numsteps": 1, "useconnectivityattrib": 1, "connectivityattrib": "weight", "tol": 0.01}),
        ("promote", "promote/multiparm/two_rules", matrix_geometry, {"promotions": (
            {"fromtype#": 2, "totype#": 2, "group#": "point_seed", "newname#": "out_edge", "preserve#": 1},
            {"fromtype#": 1, "totype#": 1, "group#": "primitive_seed", "newname#": "out_points", "preserve#": 1},
        )}),
        ("path_promote", "downstream/path_to_promote_edge", matrix_geometry, {
            "path": {"outgroup": "path_tmp", "group": "controls", "grouptype": 2},
            "promote": {"promotions": ({"fromtype#": 2, "totype#": 2, "group#": "path_tmp", "newname#": "downstream", "preserve#": 1},)},
        }),
        ("range_blast", "downstream/range_to_blast", matrix_geometry, {
            "range": {"numrange": ({"groupname#": "blast_group", "group#": "primitive_seed", "grouptype#": 1},)},
            "blast": {"group": "blast_group", "grouptype": 4},
        }),
    ])
    if matrix == "semantic":
        return semantic
    performance_subjects = (
        ("range", {"numrange": ({"groupname#": "out", "grouptype#": 0, "method#": 1, "selectamount#": 1, "selecttotal#": 2, "connectedgeo#": 1},)}),
        ("range", {"numrange": tuple(
            {"groupname#": "out_%d" % index, "grouptype#": 0, "method#": 1, "selectamount#": 1, "selecttotal#": 2, "selectoffset#": index, "connectedgeo#": 1}
            for index in range(4)
        )}),
        ("expand", {"outputgroup": "out", "group": "point_seed", "grouptype": 3, "numsteps": 32}),
        ("find_path", {"outgroup": "out", "group": "controls", "grouptype": 2}),
        ("promote", {"promotions": ({"fromtype#": 1, "totype#": 2, "group#": "primitive_seed", "newname#": "out"},)}),
    )
    return [
        (subject, "performance/%s/grid_%d" % ("range_multi" if subject == "range" and len(parameters["numrange"]) == 4 else subject, axis), lambda size=axis - 1: performance_geometry(size), parameters)
        for axis in (32, 128, 256)
        for subject, parameters in performance_subjects
    ]


def capture_case(verbs, case):
    subject, case_id, builder, overrides = case
    source = builder()
    output = hou.Geometry()
    warmups = 3 if case_id.startswith("performance/") else 0
    iterations = 15 if warmups else 1
    if subject == "path_promote":
        path_verb, path_defaults = verbs["find_path"]
        promote_verb, promote_defaults = verbs["promote"]
        path_verb.setParms(path_defaults)
        path_verb.setParms(overrides["path"])
        promote_verb.setParms(promote_defaults)
        promote_verb.setParms(overrides["promote"])
    elif subject == "range_blast":
        range_verb, range_defaults = verbs["range"]
        blast_verb, blast_defaults = verbs["blast"]
        range_verb.setParms(range_defaults)
        range_verb.setParms(overrides["range"])
        blast_verb.setParms(blast_defaults)
        blast_verb.setParms(overrides["blast"])
    else:
        verb, defaults = verbs[subject]
        verb.setParms(defaults)
        verb.setParms(overrides)

    def execute(destination):
        if subject == "path_promote":
            intermediate = hou.Geometry()
            path_verb.execute(intermediate, [source])
            promote_verb.execute(destination, [intermediate])
        elif subject == "range_blast":
            intermediate = hou.Geometry()
            range_verb.execute(intermediate, [source])
            blast_verb.execute(destination, [intermediate])
        else:
            verb.execute(destination, [source])
    cold_ms = None
    memory_sampler = WorkingSetSampler().start() if warmups else None
    baseline_working_set = working_set_bytes() if warmups else None
    if warmups:
        memory_sampler.reset(baseline_working_set)
    if warmups:
        cold_output = hou.Geometry()
        cold_started = time.perf_counter_ns()
        execute(cold_output)
        cold_ms = (time.perf_counter_ns() - cold_started) / 1_000_000.0
    for _ in range(warmups):
        execute(hou.Geometry())
    timings = []
    for _ in range(iterations):
        output = hou.Geometry()
        started = time.perf_counter_ns()
        execute(output)
        timings.append((time.perf_counter_ns() - started) / 1_000_000.0)
    timings.sort()
    sampled_peak_working_set = memory_sampler.stop() if warmups else None
    process_lifetime_peak_working_set = peak_working_set_bytes() if warmups else None
    input_domains = normalized_domains(source)
    output_domains = normalized_domains(output)
    if warmups and normalized_domains(cold_output) != output_domains:
        raise RuntimeError("Houdini Group cold case %s drifted from the measured output" % case_id)
    repeat_output = hou.Geometry()
    execute(repeat_output)
    if normalized_domains(repeat_output) != output_domains:
        raise RuntimeError("Houdini Group case %s is not deterministic" % case_id)
    payload = {
        "case_id": case_id,
        "node": subject,
        "parameters": overrides,
        "input": {
            "domains": input_domains,
            "discrete_buffer_serialization": discrete_buffer_serialization(input_domains),
        },
        "output": {
            "domains": output_domains,
            "discrete_buffer_serialization": discrete_buffer_serialization(output_domains),
        },
        "deterministic_repeat_exact": True,
    }
    if warmups:
        payload["performance"] = {
            "warmups": warmups,
            "iterations": iterations,
            "thread_count": int(os.environ.get("C3D_GROUP_BENCH_THREADS", "1")),
            "cold_ms": cold_ms,
            "cold_output_contract_exact": True,
            "timings_ms": timings,
            "p50_ms": timings[iterations // 2],
            "p95_ms": timings[min(iterations - 1, int(iterations * 0.95))],
            "baseline_working_set_bytes": baseline_working_set,
            "sampled_peak_working_set_bytes": sampled_peak_working_set,
            "sampled_peak_delta_bytes": max(0, sampled_peak_working_set - baseline_working_set),
            "process_lifetime_peak_working_set_bytes": process_lifetime_peak_working_set,
            "memory_scope": "isolated_process_sampled_working_set_peak" if os.environ.get("C3D_GROUP_ISOLATED_CASE") else "process_sampled_working_set_peak",
            "memory_sampler_threads": 1,
            "memory_sample_interval_ms": 1,
        }
    return payload


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--matrix", choices=("focused", "semantic", "performance"), default="focused")
    parser.add_argument("--output", required=True)
    parser.add_argument("--case-index", type=int)
    arguments = parser.parse_args()
    verbs = {}
    for subject, verb_name in SUBJECTS.items():
        verb = hou.sopNodeTypeCategory().nodeVerb(verb_name)
        if verb is None:
            raise RuntimeError("Houdini SOP verb %s is unavailable" % verb_name)
        verbs[subject] = (verb, verb.parms())
    selected = cases(arguments.matrix)
    if arguments.case_index is not None:
        if arguments.case_index < 0 or arguments.case_index >= len(selected):
            raise IndexError("case index %d is out of range" % arguments.case_index)
        selected = [selected[arguments.case_index]]
    payload = {
        "schema": "c3d.group.parity.capture.v1",
        "provider": {"id": "houdini", "version": hou.applicationVersionString()},
        "subject": {"kind": "sop_family", "id": "group"},
        "matrix": arguments.matrix,
        "cases": [capture_case(verbs, case) for case in selected],
        "provenance": {
            "command": "hython group_capture.py",
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
