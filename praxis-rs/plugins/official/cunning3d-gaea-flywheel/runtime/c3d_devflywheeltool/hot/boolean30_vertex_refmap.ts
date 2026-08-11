import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname } from "node:path";

export interface Boolean30VertexRefmapSource {
  primitive_vertex_offsets: number[];
  vertex: number[];
}

export interface Boolean30VertexRefmapPoint {
  point: number;
  nearby_crossings: unknown[];
  incident_facets: {
    facet: number;
    corner: number;
    source_set: 0 | 1;
    source_primitive: number;
    source_triangle: number;
  }[];
}

export interface Boolean30VertexRefmapInput {
  case_id: string;
  inputs: {
    a: Boolean30VertexRefmapSource;
    b: Boolean30VertexRefmapSource;
  };
  actual: {
    vertex_point_indices: number[];
    primitive_vertex_offsets: number[];
    vertex: number[];
  };
  expected: number[];
  points: Boolean30VertexRefmapPoint[];
}

export interface Boolean30VertexRefmapCompare {
  schema: "c3d.boolean30.vertex-refmap-compare.v1";
  case_id: string;
  vertices: number;
  crossing_vertices: number;
  changed_vertices: number;
  exact: boolean;
  first_mismatch: null | {
    vertex: number;
    primitive: number;
    corner: number;
    point: number;
    source_set: 0 | 1;
    source_primitive: number;
    source_triangle: number;
    actual: number;
    predicted: number;
    expected: number;
    reason: "value";
  };
}

interface SourceTriangle {
  primitive: number;
  cyclicHead: number;
}

function validateOffsets(offsets: number[], vertices: number, label: string): void {
  if (offsets.length === 0 || offsets[0] !== 0 || offsets[offsets.length - 1] !== vertices) {
    throw new Error(`${label} primitive offsets do not span the vertex buffer`);
  }
  for (let index = 1; index < offsets.length; index += 1) {
    if (offsets[index] < offsets[index - 1]) {
      throw new Error(`${label} primitive offsets are not monotonic`);
    }
  }
}

function sourceTriangles(source: Boolean30VertexRefmapSource, label: string): SourceTriangle[] {
  validateOffsets(source.primitive_vertex_offsets, source.vertex.length, label);
  const triangles: SourceTriangle[] = [];
  for (let primitive = 0; primitive + 1 < source.primitive_vertex_offsets.length; primitive += 1) {
    const start = source.primitive_vertex_offsets[primitive];
    const end = source.primitive_vertex_offsets[primitive + 1];
    const corners = end - start;
    for (let localTriangle = 0; localTriangle + 2 < corners; localTriangle += 1) {
      // Houdini rotates fan triangles after the first so the advancing edge owns corner zero.
      const cyclicHeadOffset = localTriangle === 0 ? start : start + localTriangle + 1;
      triangles.push({ primitive, cyclicHead: source.vertex[cyclicHeadOffset] });
    }
  }
  return triangles;
}

export function compareBoolean30VertexRefmap(
  input: Boolean30VertexRefmapInput,
): Boolean30VertexRefmapCompare {
  const vertices = input.actual.vertex.length;
  if (input.actual.vertex_point_indices.length !== vertices || input.expected.length !== vertices) {
    throw new Error("actual point, actual vertex, and expected vertex buffers have different lengths");
  }
  validateOffsets(input.actual.primitive_vertex_offsets, vertices, "actual");
  const triangles = [
    sourceTriangles(input.inputs.a, "input A"),
    sourceTriangles(input.inputs.b, "input B"),
  ];
  const pointTraces = new Map(input.points.map((point) => [point.point, point]));
  let primitive = 0;
  let crossingVertices = 0;
  let changedVertices = 0;
  let firstMismatch: Boolean30VertexRefmapCompare["first_mismatch"] = null;

  for (let vertex = 0; vertex < vertices; vertex += 1) {
    while (primitive + 1 < input.actual.primitive_vertex_offsets.length
      && vertex >= input.actual.primitive_vertex_offsets[primitive + 1]) {
      primitive += 1;
    }
    const corner = vertex - input.actual.primitive_vertex_offsets[primitive];
    const point = input.actual.vertex_point_indices[vertex];
    const pointTrace = pointTraces.get(point);
    if (!pointTrace) throw new Error(`output point ${point} has no transfer trace`);
    const facet = pointTrace.incident_facets.find(
      (candidate) => candidate.facet === primitive && candidate.corner === corner,
    );
    if (!facet) throw new Error(`output vertex ${vertex} has no matching incident facet trace`);

    let predicted = input.actual.vertex[vertex];
    if (pointTrace.nearby_crossings.length > 0) {
      crossingVertices += 1;
      const sourceTriangle = triangles[facet.source_set][facet.source_triangle];
      if (!sourceTriangle) {
        throw new Error(`source set ${facet.source_set} triangle ${facet.source_triangle} is missing`);
      }
      if (sourceTriangle.primitive !== facet.source_primitive) {
        throw new Error(`source triangle ${facet.source_triangle} belongs to primitive ${sourceTriangle.primitive}, not ${facet.source_primitive}`);
      }
      predicted = sourceTriangle.cyclicHead;
    }
    if (predicted !== input.actual.vertex[vertex]) changedVertices += 1;
    if (firstMismatch === null && predicted !== input.expected[vertex]) {
      firstMismatch = {
        vertex,
        primitive,
        corner,
        point,
        source_set: facet.source_set,
        source_primitive: facet.source_primitive,
        source_triangle: facet.source_triangle,
        actual: input.actual.vertex[vertex],
        predicted,
        expected: input.expected[vertex],
        reason: "value",
      };
    }
  }

  return {
    schema: "c3d.boolean30.vertex-refmap-compare.v1",
    case_id: input.case_id,
    vertices,
    crossing_vertices: crossingVertices,
    changed_vertices: changedVertices,
    exact: firstMismatch === null,
    first_mismatch: firstMismatch,
  };
}

function flagValue(args: string[], name: string): string | undefined {
  const index = args.indexOf(name);
  return index >= 0 ? args[index + 1] : undefined;
}

function values(geometry: any, owner: "vertex", name: string): number[] {
  const result = geometry?.attributes?.[owner]?.[name]?.values;
  if (!Array.isArray(result) || !result.every(Number.isFinite)) {
    throw new Error(`${owner} attribute ${name} is missing or non-numeric`);
  }
  return result;
}

function topology(geometry: any, name: string): number[] {
  const result = geometry?.[name];
  if (!Array.isArray(result) || !result.every(Number.isInteger)) {
    throw new Error(`topology buffer ${name} is missing or non-integral`);
  }
  return result;
}

export async function runBoolean30VertexRefmap(args: string[]): Promise<number> {
  const tracePath = flagValue(args, "--trace");
  const actualPath = flagValue(args, "--actual");
  const oraclePath = flagValue(args, "--oracle");
  const caseId = flagValue(args, "--case");
  const outputPath = flagValue(args, "--output");
  const run = args.includes("--run");
  const json = args.includes("--json");
  if (!tracePath || !actualPath || !oraclePath || !caseId || !outputPath) {
    console.error("houdini-boolean30-vertex-refmap requires --trace, --actual, --oracle, --case, and --output.");
    return 2;
  }
  const preview = {
    command: "hot houdini-boolean30-vertex-refmap",
    language: "typescript",
    runtime: `bun ${Bun.version}`,
    cargo_invocations: 0,
    trace: tracePath,
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

  const [trace, actual, oracle] = await Promise.all([
    readFile(tracePath, "utf8").then(JSON.parse),
    readFile(actualPath, "utf8").then(JSON.parse),
    readFile(oraclePath, "utf8").then(JSON.parse),
  ]);
  if (trace.case_id !== caseId) throw new Error(`trace case ${trace.case_id} does not match ${caseId}`);
  const oracleCase = oracle.cases?.find((candidate: any) => candidate.case_id === caseId);
  if (!oracleCase) throw new Error(`oracle case ${caseId} is missing`);
  const profile = oracle.input_buffers?.[oracleCase.profile];
  if (!profile?.a || !profile?.b) throw new Error(`oracle input profile ${oracleCase.profile} is missing`);

  const input: Boolean30VertexRefmapInput = {
    case_id: caseId,
    inputs: {
      a: {
        primitive_vertex_offsets: topology(profile.a, "primitive_vertex_offsets"),
        vertex: values(profile.a, "vertex", "c3d_src_vertex"),
      },
      b: {
        primitive_vertex_offsets: topology(profile.b, "primitive_vertex_offsets"),
        vertex: values(profile.b, "vertex", "c3d_src_vertex"),
      },
    },
    actual: {
      vertex_point_indices: topology(actual, "vertex_point_indices"),
      primitive_vertex_offsets: topology(actual, "primitive_vertex_offsets"),
      vertex: values(actual, "vertex", "c3d_src_vertex"),
    },
    expected: values(oracleCase.output, "vertex", "c3d_src_vertex"),
    points: trace.points,
  };
  const receipt = compareBoolean30VertexRefmap(input);
  await mkdir(dirname(outputPath), { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(receipt, null, 2)}\n`);
  console.log(JSON.stringify({ ...preview, ...receipt }, null, json ? 2 : 0));
  return receipt.exact ? 0 : 1;
}
