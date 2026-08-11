import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname } from "node:path";

type Triangle = [number, number, number];

export interface Boolean30RustDelaunayTrace {
  schema: "c3d.boolean30.delaunay-trace.v1";
  case_id: string;
  source_set: number;
  source_triangle: number;
  steps: {
    point: number | null;
    constraint: [number, number] | null;
    triangles: (Triangle | null)[];
  }[];
}

interface NativeStep {
  point: number;
  triangles: (Triangle | null)[];
}

export interface Boolean30DelaunayTraceCompare {
  schema: "c3d.boolean30.delaunay-trace-compare.v1";
  case_id: string;
  source_set: number;
  source_triangle: number;
  native_steps: number;
  rust_steps: number;
  compared_steps: number;
  exact: boolean;
  first_mismatch: null | {
    step: number;
    point: number | null;
    slot: number | null;
    native: number | Triangle | null;
    rust: number | Triangle | null;
    reason: "point" | "triangle" | "missing-step";
  };
}

function parseTriangles(line: string): (Triangle | null)[] {
  const triangles: (Triangle | null)[] = [];
  const trianglePattern = /\{off:(\d+),idx:\d+,pts:\[(\d+),(\d+),(\d+)\]\}/g;
  for (const match of line.matchAll(trianglePattern)) {
    const slot = Number(match[1]);
    while (triangles.length <= slot) triangles.push(null);
    triangles[slot] = [Number(match[2]), Number(match[3]), Number(match[4])];
  }
  return triangles;
}

function detailedCall(log: string): {
  text: string;
  pointBase: number;
  primitiveBase: number;
} | null {
  const lines = log.split(/\r?\n/);
  const workBefore = lines.findIndex((line) => line.startsWith("work before point="));
  if (workBefore < 0) return null;
  let call = workBefore - 1;
  while (call >= 0 && !lines[call].startsWith("call ")) call -= 1;
  if (call < 0) throw new Error("detailed native work state has no preceding call");
  let end = lines.findIndex((line, index) => index > call && line.startsWith("call "));
  if (end < 0) end = lines.length;

  const input = lines[call].match(/ input=\[([^\]]*)\]/)?.[1]
    .split(",")
    .filter((value) => value.length > 0) ?? [];
  const initial = parseTriangles(lines[workBefore]);
  let primitiveBase = initial.length - 1;
  while (primitiveBase >= 0 && initial[primitiveBase] === null) primitiveBase -= 1;
  const superTriangle = initial[primitiveBase];
  if (!superTriangle) throw new Error("detailed native work state has no super triangle");
  const sorted = [...superTriangle].sort((left, right) => left - right);
  if (sorted[1] !== sorted[0] + 1 || sorted[2] !== sorted[1] + 1) {
    throw new Error("detailed native work state ends with a non-consecutive super triangle");
  }
  return {
    text: lines.slice(call, end).join("\n"),
    pointBase: sorted[0] - input.length,
    primitiveBase,
  };
}

function parseNativeSteps(log: string): NativeStep[] {
  const steps: NativeStep[] = [];
  const detailed = detailedCall(log);
  const source = detailed?.text ?? log;
  const linePattern = /^work after point=(\d+).* triangles=(\[[^\r\n]*\])$/gm;
  for (const line of source.matchAll(linePattern)) {
    let triangles = parseTriangles(line[2]);
    let point = Number(line[1]);
    if (detailed) {
      point -= detailed.pointBase;
      triangles = triangles.slice(detailed.primitiveBase).map((triangle) =>
        triangle?.map((value) => value - detailed.pointBase) as Triangle | null,
      );
    }
    if (triangles.length === 0) throw new Error(`point ${line[1]} has no native triangles`);
    steps.push({ point, triangles });
  }
  if (steps.length === 0) throw new Error("native log has no 'work after point' states");
  return steps;
}

function sameTriangle(left: Triangle | null, right: Triangle | null): boolean {
  return left === null
    ? right === null
    : right !== null && left.every((point, index) => point === right[index]);
}

export function compareBoolean30DelaunayTrace(
  nativeLog: string,
  rust: Boolean30RustDelaunayTrace,
): Boolean30DelaunayTraceCompare {
  if (rust.schema !== "c3d.boolean30.delaunay-trace.v1") {
    throw new Error("invalid Rust Delaunay trace schema");
  }
  const native = parseNativeSteps(nativeLog);
  const rustSteps = rust.steps.filter((step) => step.point !== null);
  const comparedSteps = Math.min(native.length, rustSteps.length);
  let firstMismatch: Boolean30DelaunayTraceCompare["first_mismatch"] = null;
  for (let step = 0; step < comparedSteps && firstMismatch === null; step += 1) {
    const nativeStep = native[step];
    const rustStep = rustSteps[step];
    if (nativeStep.point !== rustStep.point) {
      firstMismatch = {
        step,
        point: rustStep.point,
        slot: null,
        native: nativeStep.point,
        rust: rustStep.point,
        reason: "point",
      };
      break;
    }
    const slots = Math.max(nativeStep.triangles.length, rustStep.triangles.length);
    for (let slot = 0; slot < slots; slot += 1) {
      const nativeTriangle = nativeStep.triangles[slot] ?? null;
      const rustTriangle = rustStep.triangles[slot] ?? null;
      if (!sameTriangle(nativeTriangle, rustTriangle)) {
        firstMismatch = {
          step,
          point: rustStep.point,
          slot,
          native: nativeTriangle,
          rust: rustTriangle,
          reason: "triangle",
        };
        break;
      }
    }
  }
  if (firstMismatch === null && native.length !== rustSteps.length) {
    const step = comparedSteps;
    firstMismatch = {
      step,
      point: rustSteps[step]?.point ?? native[step]?.point ?? null,
      slot: null,
      native: native[step]?.point ?? null,
      rust: rustSteps[step]?.point ?? null,
      reason: "missing-step",
    };
  }
  return {
    schema: "c3d.boolean30.delaunay-trace-compare.v1",
    case_id: rust.case_id,
    source_set: rust.source_set,
    source_triangle: rust.source_triangle,
    native_steps: native.length,
    rust_steps: rustSteps.length,
    compared_steps: comparedSteps,
    exact: firstMismatch === null,
    first_mismatch: firstMismatch,
  };
}

function flagValue(args: string[], name: string): string | undefined {
  const index = args.indexOf(name);
  return index >= 0 ? args[index + 1] : undefined;
}

export async function runBoolean30DelaunayTrace(args: string[]): Promise<number> {
  const nativeLogPath = flagValue(args, "--native-log");
  const rustTracePath = flagValue(args, "--rust-trace");
  const outputPath = flagValue(args, "--output");
  const run = args.includes("--run");
  const json = args.includes("--json");
  if (!nativeLogPath || !rustTracePath || !outputPath) {
    console.error("houdini-boolean30-delaunay-trace requires --native-log, --rust-trace, and --output.");
    return 2;
  }
  const preview = {
    command: "hot houdini-boolean30-delaunay-trace",
    language: "typescript",
    runtime: `bun ${Bun.version}`,
    cargo_invocations: 0,
    native_log: nativeLogPath,
    rust_trace: rustTracePath,
    output: outputPath,
    run,
  };
  if (!run) {
    console.log(JSON.stringify(preview, null, json ? 2 : 0));
    return 0;
  }
  const receipt = compareBoolean30DelaunayTrace(
    await readFile(nativeLogPath, "utf8"),
    JSON.parse(await readFile(rustTracePath, "utf8")) as Boolean30RustDelaunayTrace,
  );
  await mkdir(dirname(outputPath), { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(receipt, null, 2)}\n`);
  console.log(JSON.stringify({ ...preview, ...receipt }, null, json ? 2 : 0));
  return receipt.exact ? 0 : 1;
}
