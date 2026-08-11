import argparse
import json

import hou


def json_value(value):
    if isinstance(value, (hou.Vector2, hou.Vector3, hou.Vector4, hou.Quaternion)):
        return list(value)
    if isinstance(value, tuple):
        return [json_value(item) for item in value]
    if isinstance(value, (str, int, float, bool)) or value is None:
        return value
    return repr(value)


def node_parameter_contract(category, subject):
    if subject not in category.nodeTypes():
        return None
    container = hou.node("/obj").createNode("geo", run_init_scripts=False)
    try:
        node = container.createNode(subject)
        return {
            parameter_tuple.name(): {
                "label": template.label(),
                "type": str(template.type()).split(".")[-1],
                "size": len(parameter_tuple),
                "value": json_value(parameter_tuple.eval()),
                "menu_items": list(template.menuItems()) if hasattr(template, "menuItems") else [],
                "menu_labels": list(template.menuLabels()) if hasattr(template, "menuLabels") else [],
            }
            for parameter_tuple in node.parmTuples()
            for template in (parameter_tuple.parmTemplate(),)
        }
    finally:
        container.destroy()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--subject", required=True)
    arguments = parser.parse_args()
    category = hou.sopNodeTypeCategory()
    candidates = (arguments.subject + "::2.0", arguments.subject)
    for candidate in candidates:
        verb = category.nodeVerb(candidate)
        if verb is not None:
            print(
                json.dumps(
                    {
                        "provider": {"id": "houdini", "version": hou.applicationVersionString()},
                        "subject": {"kind": "sop", "id": candidate},
                        "parameters": {
                            key: json_value(value)
                            for key, value in sorted(verb.parms().items())
                        },
                        "parameter_contract": node_parameter_contract(category, arguments.subject),
                    },
                    indent=2,
                    sort_keys=True,
                )
            )
            return
    if arguments.subject in category.nodeTypes():
        print(
            json.dumps(
                {
                    "provider": {"id": "houdini", "version": hou.applicationVersionString()},
                    "subject": {"kind": "sop", "id": arguments.subject},
                    "execution_mode": "headless_node_network",
                    "parameters": node_parameter_contract(category, arguments.subject),
                },
                indent=2,
                sort_keys=True,
            )
        )
        return
    tokens = tuple(token for token in arguments.subject.lower().replace("_", " ").split() if token)
    suggestions = sorted(
        name
        for name in category.nodeTypes().keys()
        if any(token in name.lower() for token in tokens)
    )
    raise RuntimeError(
        "Houdini SOP Verb is unavailable: {}. Related SOP types: {}".format(
            arguments.subject, suggestions
        )
    )


if __name__ == "__main__":
    main()
