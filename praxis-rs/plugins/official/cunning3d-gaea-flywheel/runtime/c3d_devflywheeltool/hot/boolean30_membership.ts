import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname } from "node:path";

interface TriangleBuffer {
  point_positions: number[][];
  vertex_point_indices: number[];
  primitive_vertex_offsets: number[];
}

interface StageFacet {
  facet_index: number;
  point_indices: number[];
  source_set: number;
  source_triangle: number;
  source_primitive?: number;
  cells?: number[];
  inside_other?: boolean;
  coplanar?: string;
}

interface SelectionStage {
  schema: string;
  case_id: string;
  operation: string;
  point_positions: number[][];
  facets: StageFacet[];
}

interface MembershipDifference {
  primitive: number;
  triangle_key: string;
  candidate_facets: StageFacet[];
}

export interface Boolean30MembershipCompare {
  schema: "c3d.boolean30.facet-membership-compare.v1";
  case_id: string;
  actual_primitives: number;
  expected_primitives: number;
  common_primitives: number;
  missing_expected: MembershipDifference[];
  extra_actual: MembershipDifference[];
  exact: boolean;
}

function pointKey(point: number[]): string {
  if (point.length !== 3 || !point.every(Number.isFinite)) throw new Error("invalid point position");
  return point.map((value) => Math.round(value * 1e6)).join(":");
}

function triangleRecords(buffer: TriangleBuffer): { primitive: number; key: string }[] {
  const offsets = buffer.primitive_vertex_offsets;
  if (!Array.isArray(offsets) || offsets[0] !== 0
      || offsets[offsets.length - 1] !== buffer.vertex_point_indices.length) {
    throw new Error("primitive offsets do not span the vertex buffer");
  }
  const records = [];
  for (let primitive = 0; primitive + 1 < offsets.length; primitive += 1) {
    const points = buffer.vertex_point_indices
      .slice(offsets[primitive], offsets[primitive + 1])
      .map((point) => buffer.point_positions[point]);
    if (points.length !== 3 || points.some((point) => !point)) {
      throw new Error(`primitive ${primitive} is not a valid triangle`);
    }
    records.push({ primitive, key: points.map(pointKey).sort().join("|") });
  }
  return records;
}

function bucket(records: { primitive: number; key: string }[]): Map<string, number[]> {
  const result = new Map<string, number[]>();
  for (const record of records) {
    const primitives = result.get(record.key) ?? [];
    primitives.push(record.primitive);
    result.set(record.key, primitives);
  }
  return result;
}

export function compareBoolean30FacetMembership(
  stage: SelectionStage,
  actual: TriangleBuffer,
  expected: TriangleBuffer,
): Boolean30MembershipCompare {
  if (stage.schema !== "c3d.boolean30.selection-stage.v1") throw new Error("invalid selection stage");
  const candidates = new Map<string, StageFacet[]>();
  for (const facet of stage.facets) {
    const points = facet.point_indices.map((point) => stage.point_positions[point]);
    if (points.length !== 3 || points.some((point) => !point)) {
      throw new Error(`stage facet ${facet.facet_index} is invalid`);
    }
    const key = points.map(pointKey).sort().join("|");
    candidates.set(key, [...(candidates.get(key) ?? []), facet]);
  }
  const actualRecords = triangleRecords(actual);
  const expectedRecords = triangleRecords(expected);
  const actualBuckets = bucket(actualRecords);
  const expectedBuckets = bucket(expectedRecords);
  const keys = new Set([...actualBuckets.keys(), ...expectedBuckets.keys()]);
  const missingExpected: MembershipDifference[] = [];
  const extraActual: MembershipDifference[] = [];
  let commonPrimitives = 0;
  for (const key of keys) {
    const actualPrimitives = actualBuckets.get(key) ?? [];
    const expectedPrimitives = expectedBuckets.get(key) ?? [];
    const common = Math.min(actualPrimitives.length, expectedPrimitives.length);
    commonPrimitives += common;
    for (const primitive of expectedPrimitives.slice(common)) {
      missingExpected.push({ primitive, triangle_key: key, candidate_facets: candidates.get(key) ?? [] });
    }
    for (const primitive of actualPrimitives.slice(common)) {
      extraActual.push({ primitive, triangle_key: key, candidate_facets: candidates.get(key) ?? [] });
    }
  }
  return {
    schema: "c3d.boolean30.facet-membership-compare.v1",
    case_id: stage.case_id,
    actual_primitives: actualRecords.length,
    expected_primitives: expectedRecords.length,
    common_primitives: commonPrimitives,
    missing_expected: missingExpected,
    extra_actual: extraActual,
    exact: missingExpected.length === 0 && extraActual.length === 0,
  };
}

function flagValue(args: string[], name: string): string | undefined {
  const index = args.indexOf(name);
  return index >= 0 ? args[index + 1] : undefined;
}

export async function runBoolean30Membership(args: string[]): Promise<number> {
  const stagePath = flagValue(args, "--stage");
  const actualPath = flagValue(args, "--actual");
  const oraclePath = flagValue(args, "--oracle");
  const caseId = flagValue(args, "--case");
  const outputPath = flagValue(args, "--output");
  const run = args.includes("--run");
  const json = args.includes("--json");
  if (!stagePath || !actualPath || !oraclePath || !caseId || !outputPath) {
    console.error("houdini-boolean30-membership requires --stage, --actual, --oracle, --case, and --output.");
    return 2;
  }
  const preview = {
    command: "hot houdini-boolean30-membership",
    language: "typescript",
    runtime: `bun ${Bun.version}`,
    cargo_invocations: 0,
    stage: stagePath,
    actual: actualPath,
    oracle: oraclePath,
    case_id: caseId,
    output: outputPath,
    run,
  };
  if (!run) {
    console.log(JSON.stringify(preview, null, json ? 2 : 0));
    return 0;
  }
  const [stage, actual, oracle] = await Promise.all([
    readFile(stagePath, "utf8").then(JSON.parse),
    readFile(actualPath, "utf8").then(JSON.parse),
    readFile(oraclePath, "utf8").then(JSON.parse),
  ]);
  if (stage.case_id !== caseId) throw new Error(`stage case ${stage.case_id} does not match ${caseId}`);
  const expected = oracle.cases?.find((candidate: any) => candidate.case_id === caseId)?.output;
  if (!expected) throw new Error(`oracle case ${caseId} is missing`);
  const receipt = compareBoolean30FacetMembership(stage, actual, expected);
  await mkdir(dirname(outputPath), { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(receipt, null, 2)}\n`);
  console.log(JSON.stringify({
    ...preview,
    actual_primitives: receipt.actual_primitives,
    expected_primitives: receipt.expected_primitives,
    common_primitives: receipt.common_primitives,
    missing_expected: receipt.missing_expected.length,
    extra_actual: receipt.extra_actual.length,
    exact: receipt.exact,
  }, null, json ? 2 : 0));
  return 0;
}
