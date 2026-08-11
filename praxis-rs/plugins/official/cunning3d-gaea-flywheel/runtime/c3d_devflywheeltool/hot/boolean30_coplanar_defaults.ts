import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname } from "node:path";

export interface Boolean30CoplanarPoint {
  direct_source_point: boolean;
  nearby_crossings: { kind: string }[];
}

export function usesBoolean30DefaultPointAttributes(point: Boolean30CoplanarPoint): boolean {
  return !point.direct_source_point
    && point.nearby_crossings.length > 0
    && point.nearby_crossings.every((crossing) => crossing.kind === "CoplanarBoundary");
}

interface PointTrace {
  point: number;
  position: number[];
  nearby_crossings: { kind: string }[];
}

interface CoplanarCompareReceipt {
  schema: "c3d.boolean30.coplanar-defaults-compare.v1";
  case_id: string;
  points: number;
  default_points: number;
  exact: boolean;
  first_mismatch: null | {
    point: number;
    attribute: string;
    actual: unknown;
    predicted: unknown;
    expected: unknown;
  };
}

function pointKey(point: number[]): string {
  if (point.length !== 3 || !point.every(Number.isFinite)) throw new Error("invalid point position");
  return point.map((value) => Math.round(value * 1e12)).join(":");
}

function defaultValue(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(defaultValue);
  if (typeof value === "string") return "";
  if (typeof value === "boolean") return false;
  if (typeof value === "number") return 0;
  throw new Error(`unsupported point attribute value ${JSON.stringify(value)}`);
}

function sameValue(left: unknown, right: unknown): boolean {
  if (typeof left === "number" && typeof right === "number") {
    return Math.abs(left - right) <= 1e-5;
  }
  if (Array.isArray(left) && Array.isArray(right)) {
    return left.length === right.length && left.every((value, index) => sameValue(value, right[index]));
  }
  return left === right;
}

export function compareBoolean30CoplanarDefaults(
  caseId: string,
  sourcePositions: number[][],
  tracePoints: PointTrace[],
  actual: any,
  expected: any,
): CoplanarCompareReceipt {
  const pointCount = actual?.counts?.points;
  if (!Number.isInteger(pointCount) || expected?.counts?.points !== pointCount) {
    throw new Error("actual and expected point counts differ");
  }
  if (tracePoints.length !== pointCount) throw new Error("transfer trace does not cover every output point");
  const direct = new Set(sourcePositions.map(pointKey));
  const traceByPoint = new Map(tracePoints.map((point) => [point.point, point]));
  const attributes = Object.keys(expected?.attributes?.point ?? {}).filter((name) => name !== "P");
  let defaultPoints = 0;
  let firstMismatch: CoplanarCompareReceipt["first_mismatch"] = null;

  for (let point = 0; point < pointCount; point += 1) {
    const trace = traceByPoint.get(point);
    if (!trace) throw new Error(`output point ${point} is missing from transfer trace`);
    const useDefaults = usesBoolean30DefaultPointAttributes({
      direct_source_point: direct.has(pointKey(trace.position)),
      nearby_crossings: trace.nearby_crossings,
    });
    if (useDefaults) defaultPoints += 1;
    for (const attribute of attributes) {
      const actualValue = actual?.attributes?.point?.[attribute]?.values?.[point];
      const expectedValue = expected?.attributes?.point?.[attribute]?.values?.[point];
      if (actualValue === undefined || expectedValue === undefined) {
        throw new Error(`point attribute ${attribute} is missing value ${point}`);
      }
      const predicted = useDefaults ? defaultValue(actualValue) : actualValue;
      if (firstMismatch === null && !sameValue(predicted, expectedValue)) {
        firstMismatch = { point, attribute, actual: actualValue, predicted, expected: expectedValue };
      }
    }
  }
  return {
    schema: "c3d.boolean30.coplanar-defaults-compare.v1",
    case_id: caseId,
    points: pointCount,
    default_points: defaultPoints,
    exact: firstMismatch === null,
    first_mismatch: firstMismatch,
  };
}

function flagValue(args: string[], name: string): string | undefined {
  const index = args.indexOf(name);
  return index >= 0 ? args[index + 1] : undefined;
}

export async function runBoolean30CoplanarDefaults(args: string[]): Promise<number> {
  const tracePath = flagValue(args, "--trace");
  const actualPath = flagValue(args, "--actual");
  const oraclePath = flagValue(args, "--oracle");
  const caseId = flagValue(args, "--case");
  const outputPath = flagValue(args, "--output");
  const run = args.includes("--run");
  const json = args.includes("--json");
  if (!tracePath || !actualPath || !oraclePath || !caseId || !outputPath) {
    console.error("houdini-boolean30-coplanar-defaults requires --trace, --actual, --oracle, --case, and --output.");
    return 2;
  }
  const preview = {
    command: "hot houdini-boolean30-coplanar-defaults",
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
  const inputs = oracle.input_buffers?.[oracleCase.profile];
  if (!inputs?.a || !inputs?.b) throw new Error(`oracle profile ${oracleCase.profile} is missing`);
  const sourcePositions = [...inputs.a.point_positions, ...inputs.b.point_positions];
  const receipt = compareBoolean30CoplanarDefaults(
    caseId,
    sourcePositions,
    trace.points,
    actual,
    oracleCase.output,
  );
  await mkdir(dirname(outputPath), { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(receipt, null, 2)}\n`);
  console.log(JSON.stringify({ ...preview, ...receipt }, null, json ? 2 : 0));
  return receipt.exact ? 0 : 1;
}
