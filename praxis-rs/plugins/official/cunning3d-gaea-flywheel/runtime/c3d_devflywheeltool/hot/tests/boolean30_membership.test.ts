import { expect, test } from "bun:test";
import { compareBoolean30FacetMembership } from "../boolean30_membership";

test("Boolean30 membership maps missing and extra triangles back to stage facets", () => {
  const stage = {
    schema: "c3d.boolean30.selection-stage.v1",
    case_id: "focused/membership",
    operation: "union",
    point_positions: [[0, 0, 0], [1, 0, 0], [0, 1, 0], [-1, 0, 0]],
    facets: [
      { facet_index: 0, point_indices: [0, 1, 2], source_set: 0, source_triangle: 0 },
      { facet_index: 1, point_indices: [0, 2, 3], source_set: 1, source_triangle: 1 },
    ],
  };
  const actual = {
    point_positions: stage.point_positions,
    vertex_point_indices: [0, 1, 2],
    primitive_vertex_offsets: [0, 3],
  };
  const expected = {
    point_positions: stage.point_positions,
    vertex_point_indices: [0, 2, 3],
    primitive_vertex_offsets: [0, 3],
  };

  const result = compareBoolean30FacetMembership(stage, actual, expected);
  expect(result.exact).toBe(false);
  expect(result.missing_expected[0].candidate_facets.map((facet) => facet.facet_index)).toEqual([1]);
  expect(result.extra_actual[0].candidate_facets.map((facet) => facet.facet_index)).toEqual([0]);
});
