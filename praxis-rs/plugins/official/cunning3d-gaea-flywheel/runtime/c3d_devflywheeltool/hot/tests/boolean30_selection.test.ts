import { expect, test } from "bun:test";
import {
  applySelectionDecisions,
  decideBoolean30Selection,
  deriveBoolean30ArrangementOrder,
  deriveBoolean30BufferOrder,
  deriveBoolean30SelectedArrangement,
  groupBoolean30Superfacets,
  orderBoolean30CellBoundaryFacets,
  parseBoolean30ClassifierInput,
  parseBoolean30NativeClassifierRoots,
  parseBoolean30NativeClassifierIndices,
  parseBoolean30NativeRadialCellUnions,
  replayBoolean30NativeRadialCellUnions,
  reportBoolean30CellComponents,
  reportBoolean30RadialTJunctions,
  reportBoolean30RadialCompatibility,
  reportBoolean30RadialSchedules,
  reportBoolean30Superfacets,
  type Boolean30SelectionStage,
} from "../boolean30_selection";

const stage: Boolean30SelectionStage = {
  schema: "c3d.boolean30.selection-stage.v1",
  case_id: "focused/ordered-facets",
  operation: "union",
  facets: [
    { facet_index: 0, source_set: 0, source_primitive: 4, source_triangle: 8, inside_other: false, coplanar: "None" },
    { facet_index: 1, source_set: 1, source_primitive: 2, source_triangle: 4, inside_other: false, coplanar: "None" },
    { facet_index: 2, source_set: 0, source_primitive: 2, source_triangle: 5, inside_other: false, coplanar: "SameDirection" },
  ],
};

test("Boolean30 hot selection preserves explicit facet order and reversal", () => {
  const selected = applySelectionDecisions(stage, [
    { facet_index: 1, reversed: false, rotation: 2 },
    { facet_index: 2, reversed: true },
    { facet_index: 0, reversed: false },
  ]);
  expect(selected).toEqual({
    schema: "c3d.boolean30.selection-decisions.v1",
    case_id: stage.case_id,
    facets: [
      { facet_index: 1, reversed: false, rotation: 2 },
      { facet_index: 2, reversed: true },
      { facet_index: 0, reversed: false },
    ],
  });
});

test("Boolean30 hot selection can test native source-range orders without Rust rebuilds", () => {
  const selected = decideBoolean30Selection(stage, "b-then-a");
  expect(selected.facets).toEqual([
    { facet_index: 1, reversed: false },
    { facet_index: 0, reversed: false },
    { facet_index: 2, reversed: false },
  ]);
});

test("Boolean30 source buckets retain proven subset and coplanar intra-source order", () => {
  const grouped: Boolean30SelectionStage = {
    ...stage,
    source_a: { triangle_count: 12, ordering_triangle_count: 12, subset: false },
    source_b: { triangle_count: 6, ordering_triangle_count: 12, subset: true },
    facets: [
      { ...stage.facets[2], facet_index: 0, source_set: 0, source_primitive: 2 },
      { ...stage.facets[1], facet_index: 1, source_set: 1, source_primitive: 1 },
      { ...stage.facets[1], facet_index: 2, source_set: 1, source_primitive: 0 },
      { ...stage.facets[1], facet_index: 3, source_set: 1, source_primitive: 1 },
      { ...stage.facets[0], facet_index: 4, source_set: 0, source_primitive: 4 },
    ],
  };
  expect(decideBoolean30Selection(grouped, "b-then-a").facets.map((facet) => facet.facet_index))
    .toEqual([1, 3, 2, 4, 0]);
});

test("Boolean30 hot selection applies Boolean keep and reversal policy before ordering", () => {
  const difference: Boolean30SelectionStage = {
    ...stage,
    operation: "a_minus_b",
    facets: [
      { ...stage.facets[0], inside_other: false },
      { ...stage.facets[1], inside_other: true },
      { ...stage.facets[2], source_set: 1, coplanar: "SameDirection" },
    ],
  };
  expect(decideBoolean30Selection(difference, "classified").facets).toEqual([
    { facet_index: 0, reversed: false },
    { facet_index: 1, reversed: true },
  ]);
});

test("Boolean30 hot selection rejects duplicate and unknown facet indices", () => {
  expect(() => applySelectionDecisions(stage, [
    { facet_index: 1, reversed: false },
    { facet_index: 1, reversed: true },
  ])).toThrow("duplicate facet_index 1");
  expect(() => applySelectionDecisions(stage, [{ facet_index: 99, reversed: false }]))
    .toThrow("unknown facet_index 99");
  expect(() => applySelectionDecisions(stage, [
    { facet_index: 1, reversed: false, rotation: 3 as 0 },
  ])).toThrow("invalid rotation 3");
});

test("Boolean30 buffer feedback derives cyclic starts and reversal without rebuilding Rust", () => {
  const decisions = applySelectionDecisions(stage, [
    { facet_index: 1, reversed: false },
    { facet_index: 0, reversed: true, rotation: 1 },
  ]);
  const actual = {
    point_positions: [[0, 0, 0], [1, 0, 0], [0, 1, 0], [2, 0, 0], [3, 0, 0], [2, 1, 0]],
    vertex_point_indices: [0, 1, 2, 3, 4, 5],
    primitive_vertex_offsets: [0, 3, 6],
  };
  const expected = {
    ...actual,
    vertex_point_indices: [2, 0, 1, 3, 5, 4],
  };

  expect(deriveBoolean30BufferOrder(stage, decisions, actual, expected)).toEqual({
    schema: "c3d.boolean30.selection-decisions.v1",
    case_id: stage.case_id,
    facets: [
      { facet_index: 1, reversed: false, rotation: 2 },
      { facet_index: 0, reversed: false, rotation: 0 },
    ],
  });
});

test("Boolean30 buffer feedback fails closed when primitive membership differs", () => {
  const decisions = applySelectionDecisions(stage, [{ facet_index: 1, reversed: false }]);
  const actual = {
    point_positions: [[0, 0, 0], [1, 0, 0], [0, 1, 0]],
    vertex_point_indices: [0, 1, 2],
    primitive_vertex_offsets: [0, 3],
  };
  expect(() => deriveBoolean30BufferOrder(stage, decisions, actual, {
    ...actual,
    point_positions: [[0, 0, 0], [2, 0, 0], [0, 1, 0]],
  })).toThrow("primitive 0 cannot match by reversal and cyclic rotation");
});

test("Boolean30 buffer feedback reorders decisions by native triangle membership", () => {
  const decisions = applySelectionDecisions(stage, [
    { facet_index: 1, reversed: false },
    { facet_index: 0, reversed: false, rotation: 1 },
  ]);
  const actual = {
    point_positions: [[0, 0, 0], [1, 0, 0], [0, 1, 0], [2, 0, 0], [3, 0, 0], [2, 1, 0]],
    vertex_point_indices: [0, 1, 2, 4, 5, 3],
    primitive_vertex_offsets: [0, 3, 6],
  };
  const expected = {
    ...actual,
    vertex_point_indices: [3, 4, 5, 2, 0, 1],
  };

  expect(deriveBoolean30BufferOrder(stage, decisions, actual, expected).facets).toEqual([
    { facet_index: 0, reversed: false, rotation: 0 },
    { facet_index: 1, reversed: false, rotation: 2 },
  ]);
});

test("Boolean30 arrangement feedback maps every native triangle to a Rust facet", () => {
  const arrangementStage: Boolean30SelectionStage = {
    ...stage,
    point_positions: [
      [0, 0, 0], [1, 0, 0], [0, 1, 0],
      [2, 0, 0], [3, 0, 0], [2, 1, 0],
    ],
    facets: [
      { ...stage.facets[0], facet_index: 7, source_triangle: 3, point_indices: [0, 1, 2] },
      { ...stage.facets[1], facet_index: 9, source_triangle: 8, point_indices: [3, 4, 5] },
    ],
  };
  const log = [
    "trace before",
    "arrangement exit prim1=[{off:20,idx:0,src_set:1,src_prim:4,pts:[1/1@2/0/0,2/2@2/1/0,3/3@3/0/0]},{off:23,idx:1,src_set:0,src_prim:2,pts:[4/4@0/1/0,5/5@0/0/0,6/6@1/0/0]}]",
  ].join("\n");

  expect(deriveBoolean30ArrangementOrder(arrangementStage, log)).toEqual({
    schema: "c3d.boolean30.arrangement-order.v1",
    case_id: stage.case_id,
    facets: [
      { native_index: 0, native_offset: 20, facet_index: 9, source_triangle: 8, reversed: true, rotation: 1 },
      { native_index: 1, native_offset: 23, facet_index: 7, source_triangle: 3, reversed: false, rotation: 2 },
    ],
  });
});

test("Boolean30 arrangement feedback accepts compact old/new point indices", () => {
  const arrangementStage: Boolean30SelectionStage = {
    ...stage,
    point_positions: [
      [0, 0, 0], [1, 0, 0], [0, 1, 0],
      [2, 0, 0], [3, 0, 0], [2, 1, 0],
    ],
    facets: [
      { ...stage.facets[0], facet_index: 7, source_triangle: 3, point_indices: [0, 1, 2] },
      { ...stage.facets[1], facet_index: 9, source_triangle: 8, point_indices: [3, 4, 5] },
    ],
  };
  const log = [
    "trace before",
    "arrangement exit prim1=[{off:20,idx:0,src_set:1,src_prim:4,pts:[30/3,50/5,40/4]},{off:23,idx:1,src_set:0,src_prim:2,pts:[20/2,0/0,10/1]}]",
  ].join("\n");

  expect(deriveBoolean30ArrangementOrder(arrangementStage, log)).toEqual({
    schema: "c3d.boolean30.arrangement-order.v1",
    case_id: stage.case_id,
    facets: [
      { native_index: 0, native_offset: 20, facet_index: 9, source_triangle: 8, reversed: true, rotation: 1 },
      { native_index: 1, native_offset: 23, facet_index: 7, source_triangle: 3, reversed: false, rotation: 2 },
    ],
  });
});

test("Boolean30 selected arrangement composes native order with Boolean reversal", () => {
  const difference: Boolean30SelectionStage = {
    ...stage,
    operation: "a_minus_b",
    facets: [
      { ...stage.facets[0], facet_index: 4, inside_other: false },
      { ...stage.facets[1], facet_index: 7, inside_other: true },
      { ...stage.facets[2], facet_index: 9, source_set: 1, coplanar: "SameDirection" },
    ],
  };
  const arrangement = {
    schema: "c3d.boolean30.arrangement-order.v1" as const,
    case_id: difference.case_id,
    facets: [
      { native_index: 0, native_offset: 20, facet_index: 9, source_triangle: 5, reversed: false, rotation: 0 as const },
      { native_index: 1, native_offset: 23, facet_index: 7, source_triangle: 4, reversed: true, rotation: 1 as const },
      { native_index: 2, native_offset: 26, facet_index: 4, source_triangle: 8, reversed: false, rotation: 2 as const },
    ],
  };

  expect(deriveBoolean30SelectedArrangement(difference, arrangement)).toEqual({
    schema: "c3d.boolean30.selection-decisions.v1",
    case_id: difference.case_id,
    facets: [
      // Native selection swaps the first two vertices after arrangement. In
      // R_r S^a form that changes R_1 S to R_2 and removes the reflection.
      { facet_index: 7, reversed: false, rotation: 2 },
      { facet_index: 4, reversed: false, rotation: 2 },
    ],
  });
});

test("Boolean30 superfacets follow native late-side union roots", () => {
  const connected: Boolean30SelectionStage = {
    ...stage,
    facets: [
      { ...stage.facets[0], facet_index: 0, point_indices: [0, 1, 2] },
      { ...stage.facets[1], facet_index: 1, point_indices: [4, 5, 6] },
      { ...stage.facets[0], facet_index: 2, point_indices: [2, 1, 3] },
      { ...stage.facets[2], facet_index: 3, point_indices: [10, 11, 12] },
      { ...stage.facets[1], facet_index: 4, point_indices: [6, 5, 7] },
    ],
  };

  expect(groupBoolean30Superfacets(connected).map((group) => ({
    root: group.root,
    facets: group.facets.map((facet) => facet.facet_index),
  }))).toEqual([
    { root: 2, facets: [0, 2] },
    { root: 3, facets: [3] },
    { root: 4, facets: [1, 4] },
  ]);
});

test("Boolean30 cell boundaries keep each native superfacet contiguous", () => {
  const cellBoundary: Boolean30SelectionStage = {
    ...stage,
    operation: "intersect",
    facet_side_windings: [0, 1, 2, 3].map(() => [[0, 0], [1, 1]]),
    facets: [
      { ...stage.facets[1], facet_index: 0, point_indices: [0, 1, 2], cells: [0, 2] },
      { ...stage.facets[1], facet_index: 1, point_indices: [2, 1, 3], cells: [0, 2] },
      { ...stage.facets[0], facet_index: 2, point_indices: [10, 11, 12], cells: [0, 2] },
      { ...stage.facets[1], facet_index: 3, point_indices: [3, 1, 4], cells: [0, 2] },
    ],
  };

  expect(orderBoolean30CellBoundaryFacets(cellBoundary)).toEqual([0, 1, 3, 2]);
});

test("Boolean30 superfacets stop at a non-mutual radial edge", () => {
  const radial: Boolean30SelectionStage = {
    ...stage,
    source_a: {
      triangle_count: 3,
      ordering_triangle_count: 3,
      subset: false,
      has_non_manifold_edges: true,
    },
    facets: [
      { ...stage.facets[0], facet_index: 0, source_primitive: 0, point_indices: [0, 1, 2] },
      { ...stage.facets[0], facet_index: 1, source_primitive: 1, point_indices: [1, 0, 3] },
      { ...stage.facets[0], facet_index: 2, source_primitive: 2, point_indices: [0, 1, 4] },
    ],
  };

  expect(groupBoolean30Superfacets(radial).map((group) => group.facets.map((facet) => facet.facet_index)))
    .toEqual([[0], [1], [2]]);
});

test("Boolean30 superfacet reports preserve facet-side cell evidence and selection coverage", () => {
  const classified: Boolean30SelectionStage = {
    ...stage,
    facets: [
      { ...stage.facets[0], facet_index: 0, point_indices: [0, 1, 2], cells: [4, 2] },
      { ...stage.facets[0], facet_index: 1, point_indices: [2, 1, 3], cells: [4, 2] },
      { ...stage.facets[1], facet_index: 2, point_indices: [10, 11, 12], cells: [7, 7] },
    ],
  };

  expect(reportBoolean30Superfacets(classified, new Set([0, 2]))).toEqual([
    {
      root: 1,
      facet_indices: [0, 1],
      size: 2,
      source_set: 0,
      inside_other: false,
      coplanar: "None",
      side_cells: [[4], [2]],
      distinct_cells: [2, 4],
      cell_pairs: [{ cells: [4, 2], facet_indices: [0, 1] }],
      selected_count: 1,
      selection: "partial",
    },
    {
      root: 2,
      facet_indices: [2],
      size: 1,
      source_set: 1,
      inside_other: false,
      coplanar: "None",
      side_cells: [[7], [7]],
      distinct_cells: [7],
      cell_pairs: [{ cells: [7, 7], facet_indices: [2] }],
      selected_count: 1,
      selection: "all",
    },
  ]);
});

test("Boolean30 cell component reports expose unreachable radial regions", () => {
  const disconnected: Boolean30SelectionStage = {
    ...stage,
    facets: [
      { ...stage.facets[0], facet_index: 0, cells: [0, 1] },
      { ...stage.facets[1], facet_index: 1, cells: [1, 2] },
      { ...stage.facets[2], facet_index: 2, cells: [3, 3] },
    ],
  };

  expect(reportBoolean30CellComponents(disconnected)).toEqual([
    { root: 0, cells: [0, 1, 2], facet_indices: [0, 1] },
    { root: 3, cells: [3], facet_indices: [2] },
  ]);
});

test("Boolean30 zero-delta connectors link cells without becoming output facets", () => {
  const connected: Boolean30SelectionStage = {
    ...stage,
    connector_cells: [[2, 3]],
    facets: [
      { ...stage.facets[0], facet_index: 0, cells: [0, 1] },
      { ...stage.facets[1], facet_index: 1, cells: [1, 2] },
      { ...stage.facets[2], facet_index: 2, cells: [3, 3] },
    ],
  };

  expect(reportBoolean30CellComponents(connected)).toEqual([
    { root: 0, cells: [0, 1, 2, 3], facet_indices: [0, 1, 2] },
  ]);
});

test("Boolean30 radial compatibility reports every multi-facet edge join", () => {
  const radial: Boolean30SelectionStage = {
    ...stage,
    point_positions: [
      [0, 0, 0], [0, 0, 1],
      [1, 0, 0], [0, 1, 0], [-1, 0, 0], [0, -1, 0],
    ],
    winding_facets: [0, 1, 2, 3].map((index) => ({
      cells: [index, (index + 1) % 4],
      delta: index % 2 === 0 ? [1, 0] : [0, 1],
    })),
    facets: [0, 1, 2, 3].map((index) => ({
      ...stage.facets[index % stage.facets.length],
      facet_index: index,
      source_set: (index % 2) as 0 | 1,
      point_indices: [0, 1, index + 2],
      inside_other: index >= 2,
    })),
  };

  const report = reportBoolean30RadialCompatibility(radial);
  expect(report).toHaveLength(4);
  expect(report.every((variant) => variant.rings === 1 && variant.joins === 4)).toBe(true);
});

test("Boolean30 radial schedules enumerate native hedge union directions", () => {
  const radial: Boolean30SelectionStage = {
    ...stage,
    point_positions: [
      [0, 0, 0], [0, 0, 1],
      [1, 0, 0], [0, 1, 0], [-1, 0, 0], [0, -1, 0],
    ],
    facet_side_windings: [
      [[0, 0], [1, 0]], [[1, 0], [1, 1]],
      [[1, 1], [0, 1]], [[0, 1], [0, 0]],
    ],
    facets: [0, 1, 2, 3].map((index) => ({
      ...stage.facets[index % stage.facets.length],
      facet_index: index,
      point_indices: [0, 1, index + 2],
    })),
  };
  const classifier = {
    point_positions: radial.point_positions!,
    facets: radial.facets.map((facet) => facet.point_indices!),
    connector_range: [4, 4] as [number, number],
  };

  const report = reportBoolean30RadialSchedules(radial, classifier);

  expect(report).toHaveLength(192);
  expect(new Set(report.map((variant) => variant.schedule))).toEqual(new Set(["hedges", "rings"]));
  expect(report.every((variant) => variant.cells.length === 8)).toBe(true);
});

test("Boolean30 classifier trace parser extracts the last complete cook", () => {
  const log = [
    "unrelated",
    '\u001b[31mboolean30_classifier_input={"point_positions":[[0,0,0],[1,0,0],[0,1,0]],"facets":[[0,1,2]],"connector_range":[1,1]}\u001b[0m',
  ].join("\n");

  expect(parseBoolean30ClassifierInput(log)).toEqual({
    point_positions: [[0, 0, 0], [1, 0, 0], [0, 1, 0]],
    facets: [[0, 1, 2]],
    connector_range: [1, 1],
  });
});

test("Boolean30 native classifier parser selects the requested side count", () => {
  const log = [
    "classifier_build_index entries=4 classes=1 values=[{element:0,root:0,index:0}]",
    "classifier_build_index entries=6 classes=2 values=[{element:0,root:4,index:1},{element:1,root:1,index:0},{element:2,root:4,index:1},{element:3,root:1,index:0},{element:4,root:4,index:1},{element:5,root:1,index:0}]",
  ].join("\n");

  expect(parseBoolean30NativeClassifierRoots(log, 6)).toEqual([4, 1, 4, 1, 4, 1]);
  expect(parseBoolean30NativeClassifierIndices(log, 6)).toEqual([1, 0, 1, 0, 1, 0]);
});

test("Boolean30 native radial trace maps hedge owners to exact cell sides", () => {
  const log = [
    "arrangement entry prim1=[{off:7,idx:0,src_set:0,src_prim:0,verts:[10,11,12],pts:[7/0@0/0/0,8/1@1/0/0,9/2@0/1/0]},{off:9,idx:1,src_set:1,src_prim:0,verts:[20,21,22],pts:[8/3@0/0/0,7/4@1/0/0,10/5@0/-1/0]}]",
    "radial_union first=10 second=22 reference_point=7",
    "radial_union first=11 second=21 reference_point=7",
  ].join("\n");

  const unions = parseBoolean30NativeRadialCellUnions(log);
  expect(unions.map((entry) => entry.cells)).toEqual([[0, 2], [1, 3]]);
  expect(replayBoolean30NativeRadialCellUnions(2, unions)).toEqual({
    roots: [0, 1, 0, 1],
    cells: [0, 1, 0, 1],
  });
});

test("Boolean30 radial T-junction report finds short edges covering a mismatched long edge", () => {
  const radial: Boolean30SelectionStage = {
    ...stage,
    point_positions: [
      [0, 0, 0], [2, 0, 0], [1, 0, 0],
      [0, 1, 0], [1, 1, 0], [2, 1, 0],
    ],
    winding_facets: [0, 1, 2].map(() => ({ cells: [0, 1], delta: [1, 0] })),
    facets: [
      { ...stage.facets[0], facet_index: 0, point_indices: [0, 1, 3] },
      { ...stage.facets[1], facet_index: 1, point_indices: [0, 2, 4] },
      { ...stage.facets[2], facet_index: 2, point_indices: [2, 1, 5] },
    ],
  };

  expect(reportBoolean30RadialTJunctions(radial, [[0, 1]])).toEqual([{
    edge: [0, 1],
    split_points: [2],
    overlapping_edges: [
      { edge: [0, 2], facet: 1, source_set: 1 },
      { edge: [1, 2], facet: 2, source_set: 0 },
    ],
  }]);
});
