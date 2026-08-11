export type Boolean30CoplanarRelation = "None" | "SameDirection" | "OppositeDirection";
export type Boolean30Operation = "union" | "intersect" | "a_minus_b" | "b_minus_a";

export interface Boolean30ClassifiedFacet {
  facet_index: number;
  source_set: 0 | 1;
  source_primitive: number;
  source_triangle: number;
  inside_other: boolean;
  coplanar: Boolean30CoplanarRelation;
  point_indices?: [number, number, number];
  cells?: [number, number];
}

export interface Boolean30SelectionStage {
  schema: "c3d.boolean30.selection-stage.v1";
  case_id: string;
  operation: Boolean30Operation;
  correct_reversed_normals?: boolean | number;
  source_a?: Boolean30SourceOrder;
  source_b?: Boolean30SourceOrder;
  point_positions?: [number, number, number][];
  connector_cells?: [number, number][];
  winding_facets?: { cells: [number, number]; delta: [number, number] }[];
  exterior_candidates?: {
    exterior: number;
    cell_windings?: [number, number][];
    boundary_cells?: boolean[];
    error?: string;
  }[];
  approximate_cell_windings?: [number, number][];
  cell_windings?: [number, number][];
  facet_side_windings?: [[number, number], [number, number]][];
  boundary_cells?: boolean[];
  cell_winding_error?: string;
  facets: Boolean30ClassifiedFacet[];
}

export interface Boolean30SourceOrder {
  triangle_count: number;
  ordering_triangle_count: number;
  subset: boolean;
  has_non_manifold_edges?: boolean;
}

export type Boolean30SelectionOrder =
  | "classified"
  | "facet-index"
  | "a-then-b"
  | "b-then-a"
  | "native-superfacets";

export interface Boolean30Superfacet {
  root: number;
  facets: Boolean30ClassifiedFacet[];
}

export type Boolean30SelectionCoverage = "none" | "partial" | "all";

export interface Boolean30SuperfacetReport {
  root: number;
  facet_indices: number[];
  size: number;
  source_set: 0 | 1;
  inside_other: boolean;
  coplanar: Boolean30CoplanarRelation;
  side_cells: [number[], number[]];
  distinct_cells: number[];
  cell_pairs: { cells: [number, number]; facet_indices: number[] }[];
  selected_count: number;
  selection: Boolean30SelectionCoverage;
}

export interface Boolean30CellComponentReport {
  root: number;
  cells: number[];
  facet_indices: number[];
}

export interface Boolean30RadialCompatibilityReport {
  descending: boolean;
  swapped_sides: boolean;
  rings: number;
  joins: number;
  mismatches: number;
  mismatch_details: {
    edge: [number, number];
    current_facet: number;
    next_facet: number;
    current_depth: string;
    next_depth: string;
    current_angle: number;
    next_angle: number;
    ring: { facet: number; canonical: boolean; angle: number }[];
  }[];
}

export interface Boolean30RadialClassifierInput {
  point_positions: [number, number, number][];
  facets: [number, number, number][];
  connector_range: [number, number];
}

export interface Boolean30RadialScheduleReport {
  schedule: "hedges" | "rings";
  facet_descending?: boolean;
  edge_order?: string;
  ring_order?: "first" | "sorted" | "reverse-sorted";
  first_use_head?: boolean;
  double_unions?: boolean;
  radial_descending: boolean;
  reversed_arguments: boolean;
  swapped_sides: boolean;
  roots: number[];
  cells: number[];
  cell_windings: string[];
  consistent: boolean;
}

export interface Boolean30RadialTJunctionReport {
  edge: [number, number];
  split_points: number[];
  overlapping_edges: { edge: [number, number]; facet: number; source_set: 0 | 1 }[];
}

export interface Boolean30NativeRadialCellUnion {
  first_hedge: number;
  second_hedge: number;
  reference_point: number;
  first_primitive: number;
  second_primitive: number;
  cells: [number, number];
}

export interface Boolean30FacetDecision {
  facet_index: number;
  reversed: boolean;
  rotation?: 0 | 1 | 2;
}

export interface Boolean30SelectionDecisions {
  schema: "c3d.boolean30.selection-decisions.v1";
  case_id: string;
  facets: Boolean30FacetDecision[];
}

export interface Boolean30TriangleBuffer {
  point_positions: readonly (readonly number[])[];
  vertex_point_indices: readonly number[];
  primitive_vertex_offsets: readonly number[];
}

export interface Boolean30ArrangementFacet {
  native_index: number;
  native_offset: number;
  facet_index: number;
  source_triangle: number;
  reversed: boolean;
  rotation: 0 | 1 | 2;
}

export interface Boolean30ArrangementOrder {
  schema: "c3d.boolean30.arrangement-order.v1";
  case_id: string;
  facets: Boolean30ArrangementFacet[];
}

export function applySelectionDecisions(
  stage: Boolean30SelectionStage,
  decisions: readonly Boolean30FacetDecision[],
): Boolean30SelectionDecisions {
  const available = new Set(stage.facets.map((facet) => facet.facet_index));
  const selected = new Set<number>();
  for (const decision of decisions) {
    if (!available.has(decision.facet_index)) {
      throw new Error(`${stage.case_id}: unknown facet_index ${decision.facet_index}`);
    }
    if (selected.has(decision.facet_index)) {
      throw new Error(`${stage.case_id}: duplicate facet_index ${decision.facet_index}`);
    }
    if (decision.rotation !== undefined && ![0, 1, 2].includes(decision.rotation)) {
      throw new Error(`${stage.case_id}: invalid rotation ${decision.rotation}`);
    }
    selected.add(decision.facet_index);
  }
  return {
    schema: "c3d.boolean30.selection-decisions.v1",
    case_id: stage.case_id,
    facets: decisions.map(({ facet_index, reversed, rotation }) => ({
      facet_index,
      reversed,
      ...(rotation === undefined ? {} : { rotation }),
    })),
  };
}

function bufferTriangle(
  stage: Boolean30SelectionStage,
  buffer: Boolean30TriangleBuffer,
  primitive: number,
): readonly (readonly number[])[] {
  const start = buffer.primitive_vertex_offsets[primitive];
  const end = buffer.primitive_vertex_offsets[primitive + 1];
  if (!Number.isInteger(start) || end - start !== 3) {
    throw new Error(`${stage.case_id}: primitive ${primitive} is not a triangle`);
  }
  return buffer.vertex_point_indices.slice(start, end).map((point) => {
    const position = buffer.point_positions[point];
    if (!position || position.length !== 3 || position.some((value) => !Number.isFinite(value))) {
      throw new Error(`${stage.case_id}: primitive ${primitive} has an invalid point`);
    }
    return position;
  });
}

function rotateTriangle<T>(triangle: readonly T[], rotation: number): T[] {
  return [...triangle.slice(rotation), ...triangle.slice(0, rotation)];
}

function trianglesNear(
  left: readonly (readonly number[])[],
  right: readonly (readonly number[])[],
  tolerance: number,
): boolean {
  return left.every((point, index) => point.every((value, axis) => (
    Math.abs(value - right[index][axis]) <= tolerance
  )));
}

function triangleOrientation(
  base: readonly (readonly number[])[],
  target: readonly (readonly number[])[],
  tolerance: number,
): { reversed: boolean; rotation: 0 | 1 | 2 } | undefined {
  for (const reversed of [false, true]) {
    const oriented = base.map((point) => [...point]);
    if (reversed) [oriented[0], oriented[1]] = [oriented[1], oriented[0]];
    for (const rotation of [0, 1, 2] as const) {
      if (trianglesNear(rotateTriangle(oriented, rotation), target, tolerance)) {
        return { reversed, rotation };
      }
    }
  }
  return undefined;
}

function indexTriangleOrientation(
  base: readonly number[],
  target: readonly number[],
): { reversed: boolean; rotation: 0 | 1 | 2 } | undefined {
  if (base.length !== 3 || target.length !== 3) return undefined;
  for (const reversed of [false, true]) {
    const oriented = [...base];
    if (reversed) [oriented[0], oriented[1]] = [oriented[1], oriented[0]];
    for (const rotation of [0, 1, 2] as const) {
      const rotated = rotateTriangle(oriented, rotation);
      if (rotated.every((point, index) => point === target[index])) {
        return { reversed, rotation };
      }
    }
  }
  return undefined;
}

export function deriveBoolean30ArrangementOrder(
  stage: Boolean30SelectionStage,
  log: string,
  tolerance = 1e-6,
): Boolean30ArrangementOrder {
  if (!stage.point_positions || stage.facets.some((facet) => !facet.point_indices)) {
    throw new Error(`${stage.case_id}: arrangement feedback requires stage point geometry`);
  }
  const marker = "arrangement exit prim1=[";
  const markerStart = log.lastIndexOf(marker);
  if (markerStart < 0) throw new Error(`${stage.case_id}: native arrangement exit is missing`);
  const trace = log.slice(markerStart + marker.length);
  const entryPattern = /\{off:(\d+),idx:(\d+),src_set:\d+,src_prim:\d+,pts:\[([^\]]+)\]\}/g;
  const native: {
    native_index: number;
    native_offset: number;
    points?: number[][];
    point_indices?: number[];
  }[] = [];
  for (const match of trace.matchAll(entryPattern)) {
    const encodedPoints = match[3].split(",");
    if (encodedPoints.length !== 3) {
      throw new Error(`${stage.case_id}: native arrangement facet is not a triangle`);
    }
    const coordinateForm = encodedPoints.every((encoded) => encoded.includes("@"));
    const indexForm = encodedPoints.every((encoded) => !encoded.includes("@"));
    if (!coordinateForm && !indexForm) {
      throw new Error(`${stage.case_id}: mixed native arrangement point formats`);
    }
    const points = coordinateForm
      ? encodedPoints.map((encoded) => {
        const point = encoded.slice(encoded.indexOf("@") + 1).split("/").map(Number);
        if (point.length !== 3 || point.some((value) => !Number.isFinite(value))) {
          throw new Error(`${stage.case_id}: invalid native arrangement point ${encoded}`);
        }
        return point;
      })
      : undefined;
    const pointIndices = indexForm
      ? encodedPoints.map((encoded) => {
        const pair = encoded.split("/").map(Number);
        if (pair.length !== 2 || pair.some((value) => !Number.isInteger(value))) {
          throw new Error(`${stage.case_id}: invalid native arrangement point index ${encoded}`);
        }
        return pair[1];
      })
      : undefined;
    native.push({
      native_offset: Number(match[1]),
      native_index: Number(match[2]),
      points,
      point_indices: pointIndices,
    });
  }
  if (native.length !== stage.facets.length) {
    throw new Error(`${stage.case_id}: native arrangement has ${native.length} facets, stage has ${stage.facets.length}`);
  }

  const unmatched = new Set(stage.facets.map((facet) => facet.facet_index));
  const byIndex = new Map(stage.facets.map((facet) => [facet.facet_index, facet]));
  const facets = native.map((entry) => {
    const matches = [...unmatched].flatMap((facetIndex) => {
      const facet = byIndex.get(facetIndex)!;
      const orientation = entry.points
        ? triangleOrientation(
          facet.point_indices!.map((point) => stage.point_positions![point]),
          entry.points,
          tolerance,
        )
        : indexTriangleOrientation(facet.point_indices!, entry.point_indices!);
      return orientation ? [{ facet, orientation }] : [];
    });
    if (matches.length !== 1) {
      throw new Error(`${stage.case_id}: native facet ${entry.native_index} maps to ${matches.length} Rust facets`);
    }
    const [{ facet, orientation }] = matches;
    unmatched.delete(facet.facet_index);
    return {
      native_index: entry.native_index,
      native_offset: entry.native_offset,
      facet_index: facet.facet_index,
      source_triangle: facet.source_triangle,
      ...orientation,
    };
  });
  return {
    schema: "c3d.boolean30.arrangement-order.v1",
    case_id: stage.case_id,
    facets,
  };
}

export function deriveBoolean30SelectedArrangement(
  stage: Boolean30SelectionStage,
  arrangement: Boolean30ArrangementOrder,
): Boolean30SelectionDecisions {
  if (arrangement.case_id !== stage.case_id) {
    throw new Error(`${stage.case_id}: arrangement case identity differs`);
  }
  const selected = new Map(
    decideBoolean30Selection(stage, "classified").facets
      .map((decision) => [decision.facet_index, decision]),
  );
  const facets = arrangement.facets.flatMap((facet) => {
    const selection = selected.get(facet.facet_index);
    if (!selection) return [];
    // Arrangement orientation is R_r S^a. Native selection reversal is
    // applied afterward, and S R_r = R_-r S.
    return [{
      facet_index: facet.facet_index,
      reversed: facet.reversed !== selection.reversed,
      rotation: (selection.reversed ? (3 - facet.rotation) % 3 : facet.rotation) as 0 | 1 | 2,
    }];
  });
  if (facets.length !== selected.size) {
    const arranged = new Set(arrangement.facets.map((facet) => facet.facet_index));
    const missing = [...selected.keys()].filter((facet) => !arranged.has(facet));
    throw new Error(`${stage.case_id}: selected facets missing from arrangement: ${missing.join(",")}`);
  }
  return applySelectionDecisions(stage, facets);
}

export function deriveBoolean30BufferOrder(
  stage: Boolean30SelectionStage,
  decisions: Boolean30SelectionDecisions,
  actual: Boolean30TriangleBuffer,
  expected: Boolean30TriangleBuffer,
  tolerance = 1e-6,
): Boolean30SelectionDecisions {
  if (decisions.case_id !== stage.case_id || decisions.facets.length + 1 !== actual.primitive_vertex_offsets.length
      || decisions.facets.length + 1 !== expected.primitive_vertex_offsets.length) {
    throw new Error(`${stage.case_id}: decision and primitive counts differ`);
  }
  const unused = new Set(decisions.facets.map((_, primitive) => primitive));
  const facets = decisions.facets.map((_, expectedPrimitive) => {
    const expectedTriangle = bufferTriangle(stage, expected, expectedPrimitive);
    const candidates = [...unused].flatMap((actualPrimitive) => {
      const decision = decisions.facets[actualPrimitive];
      const actualTriangle = bufferTriangle(stage, actual, actualPrimitive);
      const baseRotation = decision.rotation ?? 0;
      const unrotated = rotateTriangle(actualTriangle, (3 - baseRotation) % 3);
      for (const toggleReversal of [false, true]) {
        const oriented = [...unrotated];
        if (toggleReversal) [oriented[0], oriented[1]] = [oriented[1], oriented[0]];
        for (const rotation of [0, 1, 2] as const) {
          if (trianglesNear(rotateTriangle(oriented, rotation), expectedTriangle, tolerance)) {
            return [{
              actualPrimitive,
              decision: {
                facet_index: decision.facet_index,
                reversed: toggleReversal ? !decision.reversed : decision.reversed,
                rotation,
              },
            }];
          }
        }
      }
      return [];
    });
    if (candidates.length === 0) {
      throw new Error(
        `${stage.case_id}: primitive ${expectedPrimitive} cannot match by reversal and cyclic rotation`,
      );
    }
    if (candidates.length !== 1) {
      throw new Error(
        `${stage.case_id}: primitive ${expectedPrimitive} ambiguously matches ${candidates.length} facets`,
      );
    }
    unused.delete(candidates[0].actualPrimitive);
    return candidates[0].decision;
  });
  return applySelectionDecisions(stage, facets);
}

function selectionRule(
  operation: Boolean30Operation,
  facet: Boolean30ClassifiedFacet,
): { keep: boolean; reversed: boolean } {
  if (facet.coplanar === "SameDirection") {
    const keep = facet.source_set === 0 && (operation === "union" || operation === "intersect");
    return { keep, reversed: false };
  }
  switch (operation) {
    case "union":
      return { keep: !facet.inside_other, reversed: false };
    case "intersect":
      return { keep: facet.inside_other, reversed: false };
    case "a_minus_b":
      return facet.source_set === 0
        ? { keep: !facet.inside_other, reversed: false }
        : { keep: facet.inside_other, reversed: true };
    case "b_minus_a":
      return facet.source_set === 1
        ? { keep: !facet.inside_other, reversed: false }
        : { keep: facet.inside_other, reversed: true };
  }
}

function classificationKey(facet: Boolean30ClassifiedFacet): string {
  return `${facet.source_set}:${Number(facet.inside_other)}:${facet.coplanar}`;
}

export function groupBoolean30Superfacets(
  stage: Boolean30SelectionStage,
): Boolean30Superfacet[] {
  const parents = stage.facets.map((_, index) => index);
  const ranks = stage.facets.map(() => 0);

  const find = (index: number): number => {
    let root = index;
    while (parents[root] !== root) root = parents[root];
    while (parents[index] !== index) {
      const next = parents[index];
      parents[index] = root;
      index = next;
    }
    return root;
  };
  const union = (first: number, second: number): void => {
    const firstRoot = find(first);
    const secondRoot = find(second);
    if (firstRoot === secondRoot) return;
    if (ranks[firstRoot] < ranks[secondRoot]) parents[firstRoot] = secondRoot;
    else if (ranks[firstRoot] > ranks[secondRoot]) parents[secondRoot] = firstRoot;
    else {
      // UT_Classifier keeps the first argument as the root when ranks tie.
      parents[secondRoot] = firstRoot;
      ranks[firstRoot] += 1;
    }
  };

  const edgeUses = new Map<string, number[]>();
  for (const [current, facet] of stage.facets.entries()) {
    if (!facet.point_indices) continue;
    for (let edge = 0; edge < 3; edge += 1) {
      const first = facet.point_indices[edge];
      const second = facet.point_indices[(edge + 1) % 3];
      const low = Math.min(first, second);
      const high = Math.max(first, second);
      const key = `${classificationKey(facet)}:${low}:${high}`;
      const uses = edgeUses.get(key);
      if (uses) uses.push(current);
      else edgeUses.set(key, [current]);
    }
  }
  const visitedEdgeUses = new Map<string, number[]>();
  for (const [current, facet] of stage.facets.entries()) {
    if (!facet.point_indices) continue;
    for (let edge = 0; edge < 3; edge += 1) {
      const first = facet.point_indices[edge];
      const second = facet.point_indices[(edge + 1) % 3];
      const low = Math.min(first, second);
      const high = Math.max(first, second);
      const key = `${classificationKey(facet)}:${low}:${high}`;
      const source = facet.source_set === 0 ? stage.source_a : stage.source_b;
      if (edgeUses.get(key)!.length > 2 && source?.has_non_manifold_edges === true) continue;
      const earlier = visitedEdgeUses.get(key);
      if (earlier) {
        for (const neighbor of earlier) union(current, neighbor);
        earlier.push(current);
      } else visitedEdgeUses.set(key, [current]);
    }
  }

  const groups = new Map<number, Boolean30ClassifiedFacet[]>();
  for (const [index, facet] of stage.facets.entries()) {
    const root = find(index);
    const members = groups.get(root);
    if (members) members.push(facet);
    else groups.set(root, [facet]);
  }
  return [...groups.entries()]
    .sort(([left], [right]) => left - right)
    .map(([root, facets]) => ({ root: stage.facets[root].facet_index, facets }));
}

function ordinaryCellSelected(operation: Boolean30Operation, depth: readonly number[]): boolean {
  const insideA = depth[0] > 0;
  const insideB = depth[1] > 0;
  switch (operation) {
    case "union": return insideA || insideB;
    case "intersect": return insideA && insideB;
    case "a_minus_b": return insideA && !insideB;
    case "b_minus_a": return insideB && !insideA;
  }
}

export function orderBoolean30CellBoundaryFacets(stage: Boolean30SelectionStage): number[] {
  if (!stage.facet_side_windings) {
    throw new Error(`${stage.case_id}: facet-side windings are missing`);
  }
  const selected: { facet: Boolean30ClassifiedFacet; cell: number }[] = [];
  stage.facets.forEach((facet, index) => {
    const sides = stage.facet_side_windings![index];
    const sideSelected = sides.map((depth) => ordinaryCellSelected(stage.operation, depth));
    if (sideSelected[0] === sideSelected[1]) return;
    const side = sideSelected[0] ? 0 : 1;
    const cell = facet.cells?.[side];
    if (cell === undefined) {
      throw new Error(`${stage.case_id}: facet ${facet.facet_index} has no cells`);
    }
    selected.push({ facet, cell });
  });
  selected.sort((left, right) => left.cell - right.cell
    || left.facet.facet_index - right.facet.facet_index);
  const groupByFacet = new Map<number, number>();
  for (const [group, superfacet] of groupBoolean30Superfacets(stage).entries()) {
    for (const facet of superfacet.facets) groupByFacet.set(facet.facet_index, group);
  }
  const groups = new Map<number, number[]>();
  for (const { facet } of selected) {
    const group = groupByFacet.get(facet.facet_index);
    if (group === undefined) throw new Error(`${stage.case_id}: facet ${facet.facet_index} has no superfacet`);
    const members = groups.get(group);
    if (members) members.push(facet.facet_index);
    else groups.set(group, [facet.facet_index]);
  }
  return [...groups.values()].flat();
}

export function reportBoolean30Superfacets(
  stage: Boolean30SelectionStage,
  selected: ReadonlySet<number> = new Set(),
): Boolean30SuperfacetReport[] {
  return groupBoolean30Superfacets(stage).map(({ root, facets }) => {
    const first = facets[0];
    const sideCells: [Set<number>, Set<number>] = [new Set(), new Set()];
    const pairFacets = new Map<string, { cells: [number, number]; facet_indices: number[] }>();
    for (const facet of facets) {
      if (!facet.cells) continue;
      sideCells[0].add(facet.cells[0]);
      sideCells[1].add(facet.cells[1]);
      const key = `${facet.cells[0]}:${facet.cells[1]}`;
      const pair = pairFacets.get(key);
      if (pair) pair.facet_indices.push(facet.facet_index);
      else pairFacets.set(key, { cells: [...facet.cells], facet_indices: [facet.facet_index] });
    }
    const selectedCount = facets.filter((facet) => selected.has(facet.facet_index)).length;
    const selection: Boolean30SelectionCoverage = selectedCount === 0
      ? "none"
      : selectedCount === facets.length ? "all" : "partial";
    const cellPairs = [...pairFacets.values()].sort((left, right) => (
      left.cells[0] - right.cells[0] || left.cells[1] - right.cells[1]
    ));
    const distinctCells = new Set([...sideCells[0], ...sideCells[1]]);
    return {
      root,
      facet_indices: facets.map((facet) => facet.facet_index),
      size: facets.length,
      source_set: first.source_set,
      inside_other: first.inside_other,
      coplanar: first.coplanar,
      side_cells: [
        [...sideCells[0]].sort((left, right) => left - right),
        [...sideCells[1]].sort((left, right) => left - right),
      ],
      distinct_cells: [...distinctCells].sort((left, right) => left - right),
      cell_pairs: cellPairs,
      selected_count: selectedCount,
      selection,
    };
  });
}

export function reportBoolean30CellComponents(
  stage: Boolean30SelectionStage,
): Boolean30CellComponentReport[] {
  const adjacency = new Map<number, Set<number>>();
  for (const facet of stage.facets) {
    if (!facet.cells) continue;
    const [first, second] = facet.cells;
    if (!adjacency.has(first)) adjacency.set(first, new Set());
    if (!adjacency.has(second)) adjacency.set(second, new Set());
    adjacency.get(first)!.add(second);
    adjacency.get(second)!.add(first);
  }
  for (const [first, second] of stage.connector_cells ?? []) {
    if (!adjacency.has(first)) adjacency.set(first, new Set());
    if (!adjacency.has(second)) adjacency.set(second, new Set());
    adjacency.get(first)!.add(second);
    adjacency.get(second)!.add(first);
  }
  const unseen = new Set([...adjacency.keys()].sort((left, right) => left - right));
  const reports: Boolean30CellComponentReport[] = [];
  while (unseen.size > 0) {
    const root = unseen.values().next().value as number;
    const pending = [root];
    const cells: number[] = [];
    unseen.delete(root);
    while (pending.length > 0) {
      const cell = pending.shift()!;
      cells.push(cell);
      for (const neighbor of [...adjacency.get(cell)!].sort((left, right) => left - right)) {
        if (!unseen.delete(neighbor)) continue;
        pending.push(neighbor);
      }
    }
    cells.sort((left, right) => left - right);
    const members = new Set(cells);
    reports.push({
      root,
      cells,
      facet_indices: stage.facets
        .filter((facet) => facet.cells?.some((cell) => members.has(cell)))
        .map((facet) => facet.facet_index)
        .sort((left, right) => left - right),
    });
  }
  return reports;
}

export function reportBoolean30RadialCompatibility(
  stage: Boolean30SelectionStage,
): Boolean30RadialCompatibilityReport[] {
  if (!stage.point_positions || !stage.winding_facets) return [];
  type EdgeUse = { facet: number; canonical: boolean; third: number; angle?: number };
  const edges = new Map<string, EdgeUse[]>();
  for (const [facet, record] of stage.facets.entries()) {
    if (!record.point_indices) continue;
    for (let edge = 0; edge < 3; edge += 1) {
      const start = record.point_indices[edge];
      const end = record.point_indices[(edge + 1) % 3];
      const key = start < end ? `${start}:${end}` : `${end}:${start}`;
      const use = { facet, canonical: start < end, third: record.point_indices[(edge + 2) % 3] };
      const uses = edges.get(key);
      if (uses) uses.push(use);
      else edges.set(key, [use]);
    }
  }
  const subtract = (a: number[], b: number[]): number[] => a.map((value, axis) => value - b[axis]);
  const dot = (a: number[], b: number[]): number => a.reduce((sum, value, axis) => sum + value * b[axis], 0);
  const cross = (a: number[], b: number[]): number[] => [
    a[1] * b[2] - a[2] * b[1],
    a[2] * b[0] - a[0] * b[2],
    a[0] * b[1] - a[1] * b[0],
  ];
  const normalize = (value: number[]): number[] => {
    const length = Math.sqrt(dot(value, value));
    return value.map((component) => component / length);
  };
  const depth = (facet: number, side: number): string => {
    const record = stage.facets[facet];
    const delta = stage.winding_facets![facet].delta;
    const own = delta[record.source_set] > 0 ? side : 1 - side;
    const other = Number(record.inside_other);
    return record.source_set === 0 ? `${own}:${other}` : `${other}:${own}`;
  };
  const rings = [...edges.entries()].filter(([, uses]) => uses.length >= 3).map(([key, uses]) => {
    const [start, end] = key.split(":").map(Number);
    const origin = stage.point_positions![start];
    const axis = normalize(subtract(stage.point_positions![end], origin));
    const absolute = axis.map(Math.abs);
    const reference = absolute[0] <= absolute[1] && absolute[0] <= absolute[2]
      ? [1, 0, 0]
      : absolute[1] <= absolute[2] ? [0, 1, 0] : [0, 0, 1];
    const basisU = normalize(cross(axis, reference));
    const basisV = cross(axis, basisU);
    return { edge: [start, end] as [number, number], uses: uses.map((use) => {
      const ray = subtract(stage.point_positions![use.third], origin);
      return { ...use, angle: Math.atan2(dot(ray, basisV), dot(ray, basisU)) };
    }).sort((left, right) => left.angle - right.angle || left.facet - right.facet) };
  });
  return [false, true].flatMap((descending) => [false, true].map((swapped_sides) => {
    let mismatches = 0;
    let joins = 0;
    const mismatch_details: Boolean30RadialCompatibilityReport["mismatch_details"] = [];
    for (const sourceRing of rings) {
      const ring = descending ? [...sourceRing.uses].reverse() : sourceRing.uses;
      for (let index = 0; index < ring.length; index += 1) {
        const current = ring[index];
        const next = ring[(index + 1) % ring.length];
        let currentSide = current.canonical ? 1 : 0;
        let nextSide = next.canonical ? 0 : 1;
        if (swapped_sides) {
          currentSide = 1 - currentSide;
          nextSide = 1 - nextSide;
        }
        joins += 1;
        const currentDepth = depth(current.facet, currentSide);
        const nextDepth = depth(next.facet, nextSide);
        if (currentDepth !== nextDepth) {
          mismatches += 1;
          if (mismatch_details.length < 20) mismatch_details.push({
            edge: sourceRing.edge,
            current_facet: current.facet,
            next_facet: next.facet,
            current_depth: currentDepth,
            next_depth: nextDepth,
            current_angle: current.angle,
            next_angle: next.angle,
            ring: ring.map((use) => ({
              facet: use.facet,
              canonical: use.canonical,
              angle: use.angle,
            })),
          });
        }
      }
    }
    return { descending, swapped_sides, rings: rings.length, joins, mismatches, mismatch_details };
  }));
}

/** Enumerates the native primitive-to-hedge classifier union schedule. */
export function reportBoolean30RadialSchedules(
  stage: Boolean30SelectionStage,
  classifier: Boolean30RadialClassifierInput,
): Boolean30RadialScheduleReport[] {
  if (!stage.facet_side_windings) return [];
  type Use = { facet: number; edge: number; canonical: boolean; angle: number };
  type Ring = { uses: Use[]; by_hedge: Map<string, number> };
  const pointKeys = new Map<string, number>();
  const representatives = classifier.point_positions.map((point, index) => {
    const key = point.map((value) => Object.is(value, -0) ? 0 : value).join(":");
    const representative = pointKeys.get(key);
    if (representative !== undefined) return representative;
    pointKeys.set(key, index);
    return index;
  });
  const edgeUses = new Map<string, Omit<Use, "angle">[]>();
  classifier.facets.forEach((points, facet) => {
    for (let edge = 0; edge < 3; edge += 1) {
      const start = representatives[points[edge]];
      const end = representatives[points[(edge + 1) % 3]];
      if (start === end) continue;
      const key = start < end ? `${start}:${end}` : `${end}:${start}`;
      const use = { facet, edge, canonical: start < end };
      const uses = edgeUses.get(key);
      if (uses) uses.push(use);
      else edgeUses.set(key, [use]);
    }
  });
  const subtract = (a: number[], b: number[]): number[] => a.map((value, axis) => value - b[axis]);
  const dot = (a: number[], b: number[]): number => a.reduce((sum, value, axis) => sum + value * b[axis], 0);
  const cross = (a: number[], b: number[]): number[] => [
    a[1] * b[2] - a[2] * b[1],
    a[2] * b[0] - a[0] * b[2],
    a[0] * b[1] - a[1] * b[0],
  ];
  const normalize = (value: number[]): number[] => {
    const length = Math.sqrt(dot(value, value));
    return value.map((component) => component / length);
  };
  const rings = new Map<string, Ring>();
  for (const [key, sourceUses] of edgeUses) {
    const [start, end] = key.split(":").map(Number);
    const origin = classifier.point_positions[start];
    const axis = normalize(subtract(classifier.point_positions[end], origin));
    const absolute = axis.map(Math.abs);
    const reference = absolute[0] <= absolute[1] && absolute[0] <= absolute[2]
      ? [1, 0, 0]
      : absolute[1] <= absolute[2] ? [0, 1, 0] : [0, 0, 1];
    const basisU = normalize(cross(axis, reference));
    const basisV = cross(axis, basisU);
    const uses = sourceUses.map((use) => {
      const third = classifier.facets[use.facet][(use.edge + 2) % 3];
      const offset = subtract(classifier.point_positions[third], origin);
      const ray = offset.map((component, index) => component - axis[index] * dot(offset, axis));
      return { ...use, angle: Math.atan2(dot(ray, basisV), dot(ray, basisU)) };
    }).sort((left, right) => left.angle - right.angle || left.facet - right.facet || left.edge - right.edge);
    rings.set(key, { uses, by_hedge: new Map(uses.map((use, index) => [`${use.facet}:${use.edge}`, index])) });
  }
  const ringByHedge = new Map<string, Ring>();
  for (const ring of rings.values()) {
    for (const use of ring.uses) ringByHedge.set(`${use.facet}:${use.edge}`, ring);
  }
  const edgeOrders = [
    [0, 1, 2], [0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0],
  ];
  type Schedule = Pick<Boolean30RadialScheduleReport,
    "schedule" | "facet_descending" | "edge_order" | "ring_order" | "first_use_head"
    | "double_unions" | "radial_descending" | "reversed_arguments" | "swapped_sides">;
  const booleans = [false, true];
  const hedgeSchedules = booleans.flatMap((facet_descending) => edgeOrders.flatMap((edgeOrder) =>
    booleans.flatMap((radial_descending) => booleans.flatMap((reversed_arguments) =>
      booleans.map((swapped_sides): Schedule => ({
        schedule: "hedges",
        facet_descending,
        edge_order: edgeOrder.join(""),
        radial_descending,
        reversed_arguments,
        swapped_sides,
      }))))));
  const ringOrders = ["first", "sorted", "reverse-sorted"] as const;
  const ringSchedules = ringOrders.flatMap((ring_order) => booleans.flatMap((first_use_head) =>
    booleans.flatMap((double_unions) => booleans.flatMap((radial_descending) =>
      booleans.flatMap((reversed_arguments) => booleans.map((swapped_sides): Schedule => ({
        schedule: "rings",
        ring_order,
        first_use_head,
        double_unions,
        radial_descending,
        reversed_arguments,
        swapped_sides,
      })))))));
  return [...hedgeSchedules, ...ringSchedules].map((schedule): Boolean30RadialScheduleReport => {
        const parents = classifier.facets.flatMap((_, facet) => [facet * 2, facet * 2 + 1]);
        const ranks = parents.map(() => 0);
        const find = (value: number): number => {
          let root = value;
          while (parents[root] !== root) root = parents[root];
          while (parents[value] !== value) {
            const next = parents[value];
            parents[value] = root;
            value = next;
          }
          return root;
        };
        const union = (first: number, second: number): void => {
          const firstRoot = find(first);
          const secondRoot = find(second);
          if (firstRoot === secondRoot) return;
          if (ranks[firstRoot] < ranks[secondRoot]) parents[firstRoot] = secondRoot;
          else if (ranks[firstRoot] > ranks[secondRoot]) parents[secondRoot] = firstRoot;
          else {
            parents[secondRoot] = firstRoot;
            ranks[firstRoot] += 1;
          }
        };
        const join = (current: Use, next: Use, complement = false): void => {
          let first = current.facet * 2 + Number(current.canonical);
          let second = next.facet * 2 + Number(!next.canonical);
          if (complement) {
            first ^= 1;
            second ^= 1;
          }
          if (schedule.swapped_sides) {
            first ^= 1;
            second ^= 1;
          }
          if (schedule.reversed_arguments) [first, second] = [second, first];
          union(first, second);
        };
        if (schedule.schedule === "hedges") {
          const facets = Array.from({ length: classifier.facets.length }, (_, index) => index);
          if (schedule.facet_descending) facets.reverse();
          const edges = schedule.edge_order!.split("").map(Number);
          for (const facet of facets) for (const edge of edges) {
            const ring = ringByHedge.get(`${facet}:${edge}`);
            const index = ring?.by_hedge.get(`${facet}:${edge}`);
            if (!ring || index === undefined) continue;
            const direction = schedule.radial_descending ? -1 : 1;
            join(ring.uses[index], ring.uses[(index + direction + ring.uses.length) % ring.uses.length]);
          }
        } else {
          const orderedRings = [...rings.entries()];
          if (schedule.ring_order !== "first") orderedRings.sort((left, right) => {
            const a = left[0].split(":").map(Number);
            const b = right[0].split(":").map(Number);
            const order = a[0] - b[0] || a[1] - b[1];
            return schedule.ring_order === "sorted" ? order : -order;
          });
          for (const [, ring] of orderedRings) {
            let uses = schedule.radial_descending ? [...ring.uses].reverse() : [...ring.uses];
            if (schedule.first_use_head) {
              const head = uses.reduce((best, use, index) =>
                use.facet * 3 + use.edge < uses[best].facet * 3 + uses[best].edge ? index : best, 0);
              uses = [...uses.slice(head), ...uses.slice(0, head)];
            }
            for (let index = 0; index < uses.length; index += 1) {
              const current = uses[index];
              const next = uses[(index + 1) % uses.length];
              join(current, next);
              if (schedule.double_unions) join(current, next, true);
            }
          }
        }
        const roots = parents.map((_, side) => find(side));
        const classByRoot = new Map([...new Set(roots)].sort((left, right) => left - right)
          .map((root, cell) => [root, cell]));
        const cells = roots.map((root) => classByRoot.get(root)!);
        const depths = Array.from({ length: classByRoot.size }, () => new Set<string>());
        let visible = 0;
        for (let facet = 0; facet < classifier.facets.length; facet += 1) {
          if (facet >= classifier.connector_range[0] && facet < classifier.connector_range[1]) continue;
          for (let side = 0; side < 2; side += 1) {
            const cell = classByRoot.get(roots[facet * 2 + side])!;
            depths[cell].add(stage.facet_side_windings![visible][side].join(","));
          }
          visible += 1;
        }
        return {
          ...schedule,
          roots,
          cells,
          cell_windings: depths.map((values) => values.size === 1 ? [...values][0] : [...values].sort().join("|")),
          consistent: depths.every((values) => values.size <= 1),
        };
      });
}

export function parseBoolean30ClassifierInput(log: string): Boolean30RadialClassifierInput {
  const prefix = "boolean30_classifier_input=";
  const payload = log.split(/\r?\n/).reverse().find((line) => line.includes(prefix));
  if (!payload) throw new Error("Boolean30 classifier trace is missing.");
  const start = payload.indexOf(prefix) + prefix.length;
  const end = payload.lastIndexOf("}") + 1;
  const input = JSON.parse(payload.slice(start, end)) as Partial<Boolean30RadialClassifierInput>;
  if (!Array.isArray(input.point_positions) || !Array.isArray(input.facets)
    || !Array.isArray(input.connector_range) || input.connector_range.length !== 2) {
    throw new Error("Boolean30 classifier trace payload is invalid.");
  }
  return input as Boolean30RadialClassifierInput;
}

function parseBoolean30NativeClassifierValues(
  log: string,
  entries: number,
): { element: number; root: number; index: number }[] {
  const marker = `classifier_build_index entries=${entries} `;
  const line = log.split(/\r?\n/).find((candidate) => candidate.includes(marker));
  if (!line) throw new Error(`Native Boolean30 classifier with ${entries} entries is missing.`);
  const values = [...line.matchAll(/element:(\d+),root:(\d+),index:(\d+)/g)]
    .map((match) => ({ element: Number(match[1]), root: Number(match[2]), index: Number(match[3]) }));
  if (values.length !== entries || values.some((entry, index) => entry.element !== index)) {
    throw new Error(`Native Boolean30 classifier with ${entries} entries is incomplete.`);
  }
  return values;
}

export function parseBoolean30NativeClassifierRoots(log: string, entries: number): number[] {
  return parseBoolean30NativeClassifierValues(log, entries).map((entry) => entry.root);
}

export function parseBoolean30NativeClassifierIndices(log: string, entries: number): number[] {
  return parseBoolean30NativeClassifierValues(log, entries).map((entry) => entry.index);
}

/** Reconstructs the cell-side joins made by FUN_1859ef650/FUN_1859eed60. */
export function parseBoolean30NativeRadialCellUnions(
  log: string,
): Boolean30NativeRadialCellUnion[] {
  const marker = "arrangement entry prim1=[";
  const entryStart = log.indexOf(marker);
  if (entryStart < 0) throw new Error("Native Boolean30 arrangement entry is missing.");
  const entryEnd = log.indexOf("\n", entryStart);
  const entry = log.slice(entryStart, entryEnd < 0 ? log.length : entryEnd);
  const hedges = new Map<number, { primitive: number; point: number }>();
  for (const match of entry.matchAll(
    /\{off:-?\d+,idx:(\d+),src_set:-?\d+,src_prim:-?\d+,verts:\[([^\]]*)\],pts:\[([^\]]*)\]\}/g,
  )) {
    const primitive = Number(match[1]);
    const primitiveHedges = match[2].length === 0 ? [] : match[2].split(",").map(Number);
    const points = match[3].length === 0 ? [] : match[3].split(",")
      .map((encoded) => Number(encoded.slice(0, encoded.indexOf("/"))));
    if (primitiveHedges.length !== points.length) {
      throw new Error(`Native Boolean30 primitive ${primitive} has mismatched topology.`);
    }
    primitiveHedges.forEach((hedge, corner) => {
      if (!Number.isInteger(hedge) || !Number.isInteger(points[corner]) || hedges.has(hedge)) {
        throw new Error(`Native Boolean30 hedge ${hedge} has an invalid owner.`);
      }
      hedges.set(hedge, { primitive, point: points[corner] });
    });
  }
  if (hedges.size === 0) {
    throw new Error("Native Boolean30 arrangement entry has no hedge owners.");
  }

  const unions = [...log.matchAll(
    /radial_union first=(-?\d+) second=(-?\d+) reference_(?:point|primitive)=(-?\d+)/g,
  )].map((match): Boolean30NativeRadialCellUnion => {
    const firstHedge = Number(match[1]);
    const secondHedge = Number(match[2]);
    const reference = Number(match[3]);
    const first = hedges.get(firstHedge);
    const second = hedges.get(secondHedge);
    if (!first || !second) {
      throw new Error(
        `Native Boolean30 radial union has unmapped hedges ${firstHedge},${secondHedge}.`,
      );
    }
    // f650 compares each hedge's point against the edge reference point. eed60
    // then selects even for first==reference and second!=reference; else odd.
    const firstSide = Number(first.point !== reference);
    const secondSide = Number(second.point === reference);
    return {
      first_hedge: firstHedge,
      second_hedge: secondHedge,
      reference_point: reference,
      first_primitive: first.primitive,
      second_primitive: second.primitive,
      cells: [first.primitive * 2 + firstSide, second.primitive * 2 + secondSide],
    };
  });
  if (unions.length === 0) throw new Error("Native Boolean30 radial unions are missing.");
  return unions;
}

/** Replays UT_Classifier's rank-based union semantics for native cell joins. */
export function replayBoolean30NativeRadialCellUnions(
  facetCount: number,
  unions: readonly Boolean30NativeRadialCellUnion[],
): { roots: number[]; cells: number[] } {
  const parents = Array.from({ length: facetCount * 2 }, (_, index) => index);
  const ranks = parents.map(() => 0);
  const find = (value: number): number => {
    if (!Number.isInteger(value) || value < 0 || value >= parents.length) {
      throw new Error(`Native Boolean30 cell ${value} is outside ${parents.length} sides.`);
    }
    let root = value;
    while (parents[root] !== root) root = parents[root];
    while (parents[value] !== value) {
      const next = parents[value];
      parents[value] = root;
      value = next;
    }
    return root;
  };
  for (const { cells: [first, second] } of unions) {
    const firstRoot = find(first);
    const secondRoot = find(second);
    if (firstRoot === secondRoot) continue;
    if (ranks[firstRoot] < ranks[secondRoot]) parents[firstRoot] = secondRoot;
    else if (ranks[firstRoot] > ranks[secondRoot]) parents[secondRoot] = firstRoot;
    else {
      parents[secondRoot] = firstRoot;
      ranks[firstRoot] += 1;
    }
  }
  const roots = parents.map((_, cell) => find(cell));
  const classByRoot = new Map([...new Set(roots)].sort((left, right) => left - right)
    .map((root, cell) => [root, cell]));
  return { roots, cells: roots.map((root) => classByRoot.get(root)!) };
}

/** Finds facet edges that geometrically overlap mismatched radial-ring edges. */
export function reportBoolean30RadialTJunctions(
  stage: Boolean30SelectionStage,
  targetEdges?: [number, number][],
): Boolean30RadialTJunctionReport[] {
  if (!stage.point_positions) return [];
  const positions = stage.point_positions;
  const subtract = (a: number[], b: number[]): number[] => a.map((value, axis) => value - b[axis]);
  const dot = (a: number[], b: number[]): number => a.reduce((sum, value, axis) => sum + value * b[axis], 0);
  const cross = (a: number[], b: number[]): number[] => [
    a[1] * b[2] - a[2] * b[1],
    a[2] * b[0] - a[0] * b[2],
    a[0] * b[1] - a[1] * b[0],
  ];
  const normalized = (left: number, right: number): [number, number] => left < right
    ? [left, right]
    : [right, left];
  const candidates = stage.facets.flatMap((record, facet) => record.point_indices
    ? [0, 1, 2].map((edge) => ({
      edge: normalized(record.point_indices![edge], record.point_indices![(edge + 1) % 3]),
      facet,
      source_set: record.source_set,
    }))
    : []);
  const targets = targetEdges ?? [...new Map(
    reportBoolean30RadialCompatibility(stage)[0]?.mismatch_details
      .map((detail) => [detail.edge.join(":"), detail.edge]) ?? [],
  ).values()];
  return targets.flatMap((edge): Boolean30RadialTJunctionReport[] => {
    const origin = positions[edge[0]];
    const direction = subtract(positions[edge[1]], origin);
    const lengthSquared = dot(direction, direction);
    if (lengthSquared === 0) return [];
    const overlapping_edges = candidates.filter((candidate) => {
      if (candidate.edge[0] === edge[0] && candidate.edge[1] === edge[1]) return false;
      const offsets = candidate.edge.map((point) => subtract(positions[point], origin));
      const collinear = offsets.every((offset) => {
        const error = cross(direction, offset);
        return dot(error, error) <= Number.EPSILON ** 2 * 4096 * lengthSquared * Math.max(1, dot(offset, offset));
      });
      if (!collinear) return false;
      const interval = offsets.map((offset) => dot(offset, direction) / lengthSquared).sort((a, b) => a - b);
      return Math.max(0, interval[0]) < Math.min(1, interval[1]);
    }).sort((left, right) => left.edge[0] - right.edge[0]
      || left.edge[1] - right.edge[1]
      || left.facet - right.facet);
    if (overlapping_edges.length === 0) return [];
    const split_points = [...new Set(overlapping_edges.flatMap((candidate) => candidate.edge))]
      .filter((point) => point !== edge[0] && point !== edge[1])
      .sort((left, right) => left - right);
    return [{ edge, split_points, overlapping_edges }];
  });
}

function orderBySourceBuckets<T extends { facet: Boolean30ClassifiedFacet }>(
  stage: Boolean30SelectionStage,
  facets: T[],
  bFirst: boolean,
): T[] {
  const groupByPrimitive = (input: T[]): T[] => {
    const groups = new Map<number, T[]>();
    for (const entry of input) {
      const group = groups.get(entry.facet.source_primitive);
      if (group) group.push(entry);
      else groups.set(entry.facet.source_primitive, [entry]);
    }
    return [...groups.values()].flat();
  };
  const a = facets.filter((entry) => entry.facet.source_set === 0);
  let b = facets.filter((entry) => entry.facet.source_set === 1);
  const coplanarPrimitives = new Set(
    a.filter((entry) => entry.facet.coplanar === "SameDirection")
      .map((entry) => entry.facet.source_primitive),
  );
  let orderedA: T[];
  if (stage.source_a?.subset) {
    orderedA = [
      ...groupByPrimitive(a.filter((entry) => !coplanarPrimitives.has(entry.facet.source_primitive))),
      ...groupByPrimitive(a.filter((entry) => coplanarPrimitives.has(entry.facet.source_primitive))),
    ];
  } else {
    orderedA = [
      ...a.filter((entry) => entry.facet.coplanar !== "SameDirection"),
      ...a.filter((entry) => entry.facet.coplanar === "SameDirection"),
    ];
  }
  if (stage.source_b?.subset) b = groupByPrimitive(b);
  return bFirst ? [...b, ...orderedA] : [...orderedA, ...b];
}

export function decideBoolean30Selection(
  stage: Boolean30SelectionStage,
  order: Boolean30SelectionOrder,
): Boolean30SelectionDecisions {
  let facets = stage.facets
    .map((facet, inputOrder) => ({ facet, inputOrder, ...selectionRule(stage.operation, facet) }))
    .filter((entry) => entry.keep);
  if (order === "classified") facets.sort((left, right) => left.inputOrder - right.inputOrder);
  else if (order === "facet-index") facets.sort((left, right) => left.facet.facet_index - right.facet.facet_index);
  else if (order === "a-then-b" || order === "b-then-a") {
    facets = orderBySourceBuckets(stage, facets, order === "b-then-a");
  } else {
    const byFacet = new Map(facets.map((entry) => [entry.facet.facet_index, entry]));
    const sourceOrder = orderBySourceBuckets(stage, facets, false);
    const sourceRank = new Map(sourceOrder.map((entry, index) => [entry.facet.facet_index, index]));
    facets = groupBoolean30Superfacets(stage).flatMap((group) => group.facets
      .map((facet) => byFacet.get(facet.facet_index))
      .filter((entry): entry is typeof facets[number] => entry !== undefined)
      .sort((left, right) => (
        sourceRank.get(left.facet.facet_index)! - sourceRank.get(right.facet.facet_index)!
      )));
  }
  return applySelectionDecisions(
    stage,
    facets.map(({ facet, reversed }) => ({ facet_index: facet.facet_index, reversed })),
  );
}

function flagValue(args: string[], name: string): string | undefined {
  const index = args.indexOf(name);
  return index >= 0 ? args[index + 1] : undefined;
}

function parseStage(value: unknown): Boolean30SelectionStage {
  const stage = value as Partial<Boolean30SelectionStage>;
  if (stage.schema !== "c3d.boolean30.selection-stage.v1" || typeof stage.case_id !== "string") {
    throw new Error("Invalid Boolean30 selection-stage identity.");
  }
  if (!Array.isArray(stage.facets) || !["union", "intersect", "a_minus_b", "b_minus_a"].includes(stage.operation ?? "")) {
    throw new Error(`${stage.case_id}: invalid Boolean30 selection-stage payload.`);
  }
  return stage as Boolean30SelectionStage;
}

function parseClassifierStage(value: unknown): Boolean30SelectionStage {
  const stage = value as Partial<Boolean30SelectionStage>;
  if (stage.schema !== "c3d.boolean30.selection-stage.v1" || typeof stage.case_id !== "string"
    || !Array.isArray(stage.facets)) {
    throw new Error("Invalid Boolean30 classifier selection-stage payload.");
  }
  return stage as Boolean30SelectionStage;
}

export async function runBoolean30Selection(args: string[]): Promise<number> {
  const stagePath = flagValue(args, "--stage");
  const outputPath = flagValue(args, "--output");
  const nativeArrangementLogPath = flagValue(args, "--native-arrangement-log");
  const classifierLogPath = flagValue(args, "--classifier-log");
  const nativeClassifierLogPath = flagValue(args, "--native-classifier-log");
  const selectedArrangement = args.includes("--selected-arrangement");
  const rotationActualPath = flagValue(args, "--rotation-actual");
  const decisionsPath = flagValue(args, "--decisions");
  const oraclePath = flagValue(args, "--oracle");
  const order = (flagValue(args, "--order") ?? "classified") as Boolean30SelectionOrder;
  const run = args.includes("--run");
  const json = args.includes("--json");
  if (!stagePath || !outputPath) {
    console.error("houdini-boolean30-selection requires --stage PATH and --output PATH.");
    return 2;
  }
  if (rotationActualPath && (!decisionsPath || !oraclePath)) {
    console.error("Boolean30 rotation feedback requires --decisions PATH and --oracle PATH.");
    return 2;
  }
  if (selectedArrangement && !nativeArrangementLogPath) {
    console.error("Boolean30 selected arrangement requires --native-arrangement-log PATH.");
    return 2;
  }
  if (nativeArrangementLogPath && rotationActualPath) {
    console.error("Boolean30 arrangement and buffer feedback modes are mutually exclusive.");
    return 2;
  }
  if (!["classified", "facet-index", "a-then-b", "b-then-a", "native-superfacets"].includes(order)) {
    console.error(`Unsupported Boolean30 selection order: ${order}`);
    return 2;
  }
  const preview = {
    command: "hot houdini-boolean30-selection",
    language: "typescript",
    runtime: `bun ${Bun.version}`,
    cargo_invocations: 0,
    stage: stagePath,
    output: outputPath,
    order,
    native_arrangement_log: nativeArrangementLogPath,
    classifier_log: classifierLogPath,
    native_classifier_log: nativeClassifierLogPath,
    selected_arrangement: selectedArrangement,
    rotation_actual: rotationActualPath,
    decisions: decisionsPath,
    oracle: oraclePath,
    run,
  };
  if (!run) {
    console.log(JSON.stringify(preview, null, json ? 2 : 0));
    return 0;
  }
  const stageValue = JSON.parse(await readFile(stagePath, "utf8"));
  if (classifierLogPath) {
    const stage = parseClassifierStage(stageValue);
    const classifier = parseBoolean30ClassifierInput(await readFile(classifierLogPath, "utf8"));
    const schedules = reportBoolean30RadialSchedules(stage, classifier);
    const nativeClassifierLog = nativeClassifierLogPath
      ? await readFile(nativeClassifierLogPath, "utf8")
      : undefined;
    const nativeRoots = nativeClassifierLog
      ? parseBoolean30NativeClassifierRoots(nativeClassifierLog, classifier.facets.length * 2)
      : undefined;
    const nativeCells = nativeClassifierLog
      ? parseBoolean30NativeClassifierIndices(
        nativeClassifierLog,
        classifier.facets.length * 2,
      )
      : undefined;
    const nativeRadialUnions = nativeClassifierLog
      ? parseBoolean30NativeRadialCellUnions(nativeClassifierLog)
      : undefined;
    const nativeRadialReplay = nativeRadialUnions
      ? replayBoolean30NativeRadialCellUnions(classifier.facets.length, nativeRadialUnions)
      : undefined;
    const compared = schedules.map((schedule) => {
      if (!nativeRoots) return schedule;
      const mismatches = schedule.roots.reduce((count, root, index) => count + Number(root !== nativeRoots[index]), 0);
      return {
        ...schedule,
        native_root_mismatches: mismatches,
        first_native_root_mismatch: schedule.roots.findIndex((root, index) => root !== nativeRoots[index]),
        native_cell_mismatches: schedule.cells.reduce(
          (count, cell, index) => count + Number(cell !== nativeCells![index]),
          0,
        ),
        first_native_cell_mismatch: schedule.cells.findIndex((cell, index) => cell !== nativeCells![index]),
      };
    }).sort((left, right) => ("native_cell_mismatches" in left ? left.native_cell_mismatches : 0)
      - ("native_cell_mismatches" in right ? right.native_cell_mismatches : 0)
      || ("native_root_mismatches" in left ? left.native_root_mismatches : 0)
      - ("native_root_mismatches" in right ? right.native_root_mismatches : 0));
    await mkdir(dirname(outputPath), { recursive: true });
    await writeFile(outputPath, `${JSON.stringify(compared, null, 2)}\n`);
    console.log(JSON.stringify({
      ...preview,
      case_id: stage.case_id,
      classifier_facets: classifier.facets.length,
      schedules: schedules.length,
      best_native_root_mismatches: nativeRoots && "native_root_mismatches" in compared[0]
        ? compared[0].native_root_mismatches
        : undefined,
      best_native_cell_mismatches: nativeCells && "native_cell_mismatches" in compared[0]
        ? compared[0].native_cell_mismatches
        : undefined,
      exact_native_cell_schedules: nativeCells
        ? compared.filter((schedule) => "native_cell_mismatches" in schedule && schedule.native_cell_mismatches === 0).length
        : undefined,
      exact_native_root_schedules: nativeRoots
        ? compared.filter((schedule) => "native_root_mismatches" in schedule && schedule.native_root_mismatches === 0).length
        : undefined,
      native_radial_unions: nativeRadialUnions?.length,
      native_radial_root_mismatches: nativeRadialReplay && nativeRoots
        ? nativeRadialReplay.roots.reduce(
          (count, root, index) => count + Number(root !== nativeRoots[index]),
          0,
        )
        : undefined,
      native_radial_cell_mismatches: nativeRadialReplay && nativeCells
        ? nativeRadialReplay.cells.reduce(
          (count, cell, index) => count + Number(cell !== nativeCells[index]),
          0,
        )
        : undefined,
      consistent_schedules: schedules.filter((schedule) => schedule.consistent).length,
      cell_orders: [...new Set(schedules.filter((schedule) => schedule.consistent)
        .map((schedule) => schedule.cell_windings.join(";")))],
    }, null, json ? 2 : 0));
    return 0;
  }
  const stage = parseStage(stageValue);
  if (nativeArrangementLogPath) {
    const arrangement = deriveBoolean30ArrangementOrder(
      stage,
      await readFile(nativeArrangementLogPath, "utf8"),
    );
    const result = selectedArrangement
      ? deriveBoolean30SelectedArrangement(stage, arrangement)
      : arrangement;
    await mkdir(dirname(outputPath), { recursive: true });
    await writeFile(outputPath, `${JSON.stringify(result, null, 2)}\n`);
    console.log(JSON.stringify({
      ...preview,
      case_id: stage.case_id,
      facets: result.facets.length,
      arrangement_facets: arrangement.facets.length,
      source_triangles: new Set(arrangement.facets.map((facet) => facet.source_triangle)).size,
    }, null, json ? 2 : 0));
    return 0;
  }
  let decisions: Boolean30SelectionDecisions;
  if (rotationActualPath) {
    const base = JSON.parse(await readFile(decisionsPath!, "utf8")) as Boolean30SelectionDecisions;
    const validated = applySelectionDecisions(stage, base.facets);
    const oracle = JSON.parse(await readFile(oraclePath!, "utf8")) as {
      cases?: { case_id?: string; output?: Boolean30TriangleBuffer }[];
    };
    const expected = oracle.cases?.find((entry) => entry.case_id === stage.case_id)?.output;
    if (!expected) throw new Error(`${stage.case_id}: oracle case is missing`);
    const actual = JSON.parse(await readFile(rotationActualPath, "utf8")) as Boolean30TriangleBuffer;
    decisions = deriveBoolean30BufferOrder(stage, validated, actual, expected);
  } else decisions = decideBoolean30Selection(stage, order);
  const superfacets = order === "native-superfacets"
    ? groupBoolean30Superfacets(stage)
    : [];
  const selected = new Set(decisions.facets.map((facet) => facet.facet_index));
  const superfacetReport = order === "native-superfacets"
    ? reportBoolean30Superfacets(stage, selected)
    : [];
  await mkdir(dirname(outputPath), { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(decisions, null, 2)}\n`);
  const radialCompatibility = order === "native-superfacets"
    ? reportBoolean30RadialCompatibility(stage)
    : [];
  console.log(JSON.stringify({
    ...preview,
    case_id: stage.case_id,
    facets: decisions.facets.length,
    superfacet_roots: superfacets.map((group) => group.root),
    selected_superfacet_sizes: superfacets
      .map((group) => group.facets.filter((facet) => selected.has(facet.facet_index)).length)
      .filter((size) => size > 0),
    superfacets: superfacetReport,
    cell_components: order === "native-superfacets" ? reportBoolean30CellComponents(stage) : [],
    radial_compatibility: radialCompatibility,
    radial_t_junctions: order === "native-superfacets"
      ? reportBoolean30RadialTJunctions(
        stage,
        [...new Map(radialCompatibility[0]?.mismatch_details
          .map((detail) => [detail.edge.join(":"), detail.edge]) ?? []).values()],
      )
      : [],
  }, null, json ? 2 : 0));
  return 0;
}
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname } from "node:path";
