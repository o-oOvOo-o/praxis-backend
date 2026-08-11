import { expect, test } from "bun:test";
import {
  compareBoolean30VertexRefmap,
  type Boolean30VertexRefmapInput,
} from "../boolean30_vertex_refmap";

const input: Boolean30VertexRefmapInput = {
  case_id: "focused/crossing-vertex",
  inputs: {
    a: { primitive_vertex_offsets: [0], vertex: [] },
    b: { primitive_vertex_offsets: [0, 3], vertex: [66, 67, 68] },
  },
  actual: {
    vertex_point_indices: [0, 1, 2],
    primitive_vertex_offsets: [0, 3],
    vertex: [66, 67, 68],
  },
  expected: [66, 66, 66],
  points: [
    {
      point: 0,
      nearby_crossings: [],
      incident_facets: [{ facet: 0, corner: 0, source_set: 1, source_primitive: 0, source_triangle: 0 }],
    },
    {
      point: 1,
      nearby_crossings: [{}],
      incident_facets: [{ facet: 0, corner: 1, source_set: 1, source_primitive: 0, source_triangle: 0 }],
    },
    {
      point: 2,
      nearby_crossings: [{}],
      incident_facets: [{ facet: 0, corner: 2, source_set: 1, source_primitive: 0, source_triangle: 0 }],
    },
  ],
};

test("Boolean30 crossing endpoints use the source triangle cyclic vertex head", () => {
  expect(compareBoolean30VertexRefmap(input)).toEqual({
    schema: "c3d.boolean30.vertex-refmap-compare.v1",
    case_id: input.case_id,
    vertices: 3,
    crossing_vertices: 2,
    changed_vertices: 2,
    exact: true,
    first_mismatch: null,
  });
});

test("Boolean30 non-crossing vertices retain their direct source vertex", () => {
  const changed = structuredClone(input);
  changed.points[2].nearby_crossings = [];
  changed.expected[2] = 68;
  expect(compareBoolean30VertexRefmap(changed).exact).toBe(true);
});
