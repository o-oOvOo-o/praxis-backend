import hashlib
import os
import struct

import hou


def typed_buffer(root, relative_path, scalar_type, values):
    formats = {"f32": "f", "i32": "i", "u32": "I", "u8": "B"}
    payload = struct.pack("<" + formats[scalar_type] * len(values), *values)
    path = os.path.join(root, *relative_path.split("/"))
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "wb") as handle:
        handle.write(payload)
    return {
        "path": relative_path,
        "scalar_type": scalar_type,
        "length": len(values),
        "sha256": hashlib.sha256(payload).hexdigest(),
    }


def numeric_attributes(root, prefix, attributes, elements=None, geometry=None):
    captured = {}
    for attribute in sorted(attributes, key=lambda item: item.name()):
        if attribute.dataType() == hou.attribData.Float:
            scalar_type = "f32"
        elif attribute.dataType() == hou.attribData.Int:
            scalar_type = "i32"
        else:
            continue
        values = []
        owners = elements if elements is not None else (geometry,)
        for owner in owners:
            value = owner.attribValue(attribute)
            values.extend(value if isinstance(value, tuple) else (value,))
        captured[attribute.name()] = {
            "storage": scalar_type,
            "tuple_size": attribute.size(),
            "buffer": typed_buffer(
                root,
                "{}.attr.{}.{}le".format(prefix, attribute.name(), scalar_type),
                scalar_type,
                values,
            ),
        }
    return captured


def groups(root, prefix, group_items, count):
    captured = {}
    for group in sorted(group_items, key=lambda item: item.name()):
        members = (
            {element.number() for element in group.points()}
            if hasattr(group, "points")
            else {element.number() for element in group.prims()}
        )
        captured[group.name()] = typed_buffer(
            root,
            "{}.group.{}.u8".format(prefix, group.name()),
            "u8",
            [int(index in members) for index in range(count)],
        )
    return captured


def geometry_domains(root, case_slug, side, geometry):
    points = list(geometry.points())
    primitives = list(geometry.prims())
    vertices = [vertex for primitive in primitives for vertex in primitive.vertices()]
    prefix = "houdini_buffers/{}/{}".format(case_slug, side)
    point_attributes = numeric_attributes(root, prefix + ".point", geometry.pointAttribs(), points)
    vertex_attributes = numeric_attributes(root, prefix + ".vertex", geometry.vertexAttribs(), vertices)
    primitive_attributes = numeric_attributes(root, prefix + ".primitive", geometry.primAttribs(), primitives)
    detail_attributes = numeric_attributes(root, prefix + ".detail", geometry.globalAttribs(), geometry=geometry)
    return {
        "point": {
            "count": len(points),
            "attributes": point_attributes,
            "groups": groups(root, prefix + ".point", geometry.pointGroups(), len(points)),
        },
        "vertex": {
            "count": len(vertices),
            "point_indices": typed_buffer(
                root,
                prefix + ".vertex.point_indices.u32le",
                "u32",
                [vertex.point().number() for vertex in vertices],
            ),
            "attributes": vertex_attributes,
            "groups": {},
        },
        "primitive": {
            "count": len(primitives),
            "vertex_counts": typed_buffer(
                root,
                prefix + ".primitive.vertex_counts.u32le",
                "u32",
                [len(primitive.vertices()) for primitive in primitives],
            ),
            "closed": typed_buffer(
                root,
                prefix + ".primitive.closed.u8",
                "u8",
                [
                    int(primitive.intrinsicValue("closed"))
                    if primitive.type() == hou.primType.Polygon
                    else 0
                    for primitive in primitives
                ],
            ),
            "attributes": primitive_attributes,
            "groups": groups(root, prefix + ".primitive", geometry.primGroups(), len(primitives)),
        },
        "detail": {"attributes": detail_attributes},
    }
