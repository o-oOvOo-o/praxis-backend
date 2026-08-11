import { existsSync } from "node:fs";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import {
  cloneDomains,
  readF32,
  readF32Ref,
  readU32Ref,
  readU8Ref,
  sha256,
  writeF32,
  writeRelativeF32,
  writeU32,
  type GeometryDomains,
  type GeometrySide,
  type TypedBufferRef,
} from "./core";

export { readF32, writeF32, writeU32, type GeometryDomains } from "./core";

export type Vec3 = [number, number, number];
type Justify = "none" | "min" | "center" | "max";
type Goal = "same" | "min" | "center" | "max";

export interface MatchSizeCase {
  case_id: string;
  parameters: {
    target_mode: "explicit" | "second_input";
    target_position: Vec3;
    target_size: Vec3;
    scale_to_fit: boolean;
    uniform_scale: boolean;
    scale_axis: "x" | "y" | "z" | "best_fit";
    translate: boolean;
    source_justify: [Justify, Justify, Justify];
    target_justify: [Goal, Goal, Goal];
  };
  input: GeometrySide;
  target: GeometrySide | null;
  output?: GeometrySide;
}

interface MatchSizeCapture {
  schema: "c3d.parity.capture.v1";
  provider: { id: string; version?: string; execution_mode?: string };
  subject: { kind: "sop"; id: "matchsize" };
  profile: "focused";
  cases: MatchSizeCase[];
  provenance: Record<string, unknown>;
}

interface FirstMismatch {
  case_id: string;
  stage: string;
  path: string;
  expected: unknown;
  actual: unknown;
  reason?: string;
  absolute_error?: number;
  absolute_tolerance?: number;
  relative_tolerance?: number;
}

const ABS_TOLERANCE = 1e-6;
const REL_TOLERANCE = 1e-6;

function bounds(positions: Float32Array): { min: Vec3; max: Vec3; center: Vec3; size: Vec3 } {
  if (positions.length === 0 || positions.length % 3 !== 0) throw new Error("Match Size needs a non-empty point P buffer.");
  const min: Vec3 = [Number.POSITIVE_INFINITY, Number.POSITIVE_INFINITY, Number.POSITIVE_INFINITY];
  const max: Vec3 = [Number.NEGATIVE_INFINITY, Number.NEGATIVE_INFINITY, Number.NEGATIVE_INFINITY];
  for (let index = 0; index < positions.length; index += 3) {
    for (let lane = 0; lane < 3; lane++) {
      min[lane] = Math.min(min[lane], positions[index + lane]);
      max[lane] = Math.max(max[lane], positions[index + lane]);
    }
  }
  const size: Vec3 = [max[0] - min[0], max[1] - min[1], max[2] - min[2]];
  const center: Vec3 = [(min[0] + max[0]) * 0.5, (min[1] + max[1]) * 0.5, (min[2] + max[2]) * 0.5];
  return { min, max, center, size };
}

function validateFocusedContract(source: MatchSizeCase): void {
  if (!source.parameters.scale_to_fit || !source.parameters.translate) {
    throw new Error(`${source.case_id}: focused Match Size requires scale and translation.`);
  }
  if (source.parameters.source_justify.some((value) => value !== "center") || source.parameters.target_justify.some((value) => value !== "same")) {
    throw new Error(`${source.case_id}: non-center justification is outside the focused contract.`);
  }
  if (source.parameters.uniform_scale && source.parameters.scale_axis !== "x") {
    throw new Error(`${source.case_id}: focused uniform scaling currently requires the X scale axis.`);
  }
  if (source.parameters.target_mode === "second_input" && !source.target) {
    throw new Error(`${source.case_id}: second-input target geometry is missing.`);
  }
}

export async function applyFocusedMatchSize(
  oracleRoot: string,
  source: MatchSizeCase,
  candidateRoot: string,
): Promise<MatchSizeCase & { output: GeometrySide }> {
  validateFocusedContract(source);
  const inputPositions = await readF32Ref(oracleRoot, source.input.domains.point.attributes.P.buffer);
  const sourceBounds = bounds(inputPositions);
  let targetCenter = source.parameters.target_position;
  let targetSize = source.parameters.target_size;
  if (source.parameters.target_mode === "second_input") {
    const targetPositions = await readF32Ref(oracleRoot, source.target!.domains.point.attributes.P.buffer);
    const targetBounds = bounds(targetPositions);
    targetCenter = targetBounds.center;
    targetSize = targetBounds.size;
  }
  if (sourceBounds.size.some((value) => value === 0)) throw new Error(`${source.case_id}: zero-sized source bounds are outside the focused contract.`);
  const ratios: Vec3 = [
    targetSize[0] / sourceBounds.size[0],
    targetSize[1] / sourceBounds.size[1],
    targetSize[2] / sourceBounds.size[2],
  ];
  const scale: Vec3 = source.parameters.uniform_scale
    ? [ratios[0], ratios[0], ratios[0]]
    : ratios;
  const output = new Float32Array(inputPositions.length);
  for (let index = 0; index < inputPositions.length; index++) {
    const lane = index % 3;
    const centered = Math.fround(inputPositions[index] - sourceBounds.center[lane]);
    output[index] = Math.fround(Math.fround(centered * scale[lane]) + targetCenter[lane]);
  }
  const slug = source.case_id.replaceAll("/", "_");
  const input = await cloneDomains(oracleRoot, source.input.domains, candidateRoot, `typescript_buffers/${slug}.input`);
  const target = source.target
    ? { domains: await cloneDomains(oracleRoot, source.target.domains, candidateRoot, `typescript_buffers/${slug}.target`) }
    : null;
  const outputPosition = await writeRelativeF32(candidateRoot, `typescript_buffers/${slug}.output.point.P.f32le`, output);
  return {
    ...source,
    input: { domains: input },
    target,
    output: {
      domains: {
        ...input,
        point: { ...input.point, attributes: { P: { storage: "f32", tuple_size: 3, buffer: outputPosition } } },
      },
    },
  };
}

export function compareMatchSizePositions(caseId: string, expected: Float32Array, actual: Float32Array): FirstMismatch | null {
  if (expected.length !== actual.length) {
    return { case_id: caseId, stage: "output.point.P", path: "output.domains.point.P.length", expected: expected.length, actual: actual.length, reason: "buffer length differs" };
  }
  for (let index = 0; index < expected.length; index++) {
    const absoluteError = Math.abs(expected[index] - actual[index]);
    const tolerance = ABS_TOLERANCE + REL_TOLERANCE * Math.max(Math.abs(expected[index]), Math.abs(actual[index]));
    if (!Number.isFinite(absoluteError) || absoluteError > tolerance) {
      return {
        case_id: caseId,
        stage: "output.point.P",
        path: `output.domains.point.P[${Math.floor(index / 3)}].${"xyz"[index % 3]}`,
        expected: expected[index],
        actual: actual[index],
        absolute_error: absoluteError,
        absolute_tolerance: ABS_TOLERANCE,
        relative_tolerance: REL_TOLERANCE,
      };
    }
  }
  return null;
}

async function exactMismatch(
  caseId: string,
  stage: string,
  expectedRoot: string,
  expectedRef: TypedBufferRef,
  actualRoot: string,
  actualRef: TypedBufferRef,
): Promise<FirstMismatch | null> {
  const expected = expectedRef.scalar_type === "u32" ? await readU32Ref(expectedRoot, expectedRef) : await readU8Ref(expectedRoot, expectedRef);
  const actual = actualRef.scalar_type === "u32" ? await readU32Ref(actualRoot, actualRef) : await readU8Ref(actualRoot, actualRef);
  if (expectedRef.scalar_type !== actualRef.scalar_type || expected.length !== actual.length) {
    return { case_id: caseId, stage, path: `${stage}.length`, expected: expected.length, actual: actual.length, reason: "typed buffer shape differs" };
  }
  for (let index = 0; index < expected.length; index++) {
    if (expected[index] !== actual[index]) return { case_id: caseId, stage, path: `${stage}[${index}]`, expected: expected[index], actual: actual[index], reason: "discrete topology differs" };
  }
  return null;
}

async function compareCaptures(
  expectedRoot: string,
  expected: MatchSizeCapture,
  actualRoot: string,
  actual: MatchSizeCapture,
): Promise<{ cases_passed: number; first_mismatch: FirstMismatch | null }> {
  if (expected.schema !== actual.schema || expected.subject.id !== actual.subject.id || expected.profile !== actual.profile) {
    return { cases_passed: 0, first_mismatch: { case_id: "<capture>", stage: "identity", path: "$", expected: expected.subject, actual: actual.subject, reason: "capture identity differs" } };
  }
  for (let caseIndex = 0; caseIndex < expected.cases.length; caseIndex++) {
    const left = expected.cases[caseIndex];
    const right = actual.cases[caseIndex];
    if (!right || left.case_id !== right.case_id || !left.output || !right.output) {
      return { cases_passed: caseIndex, first_mismatch: { case_id: left.case_id, stage: "identity", path: `cases[${caseIndex}]`, expected: left.case_id, actual: right?.case_id, reason: "case identity or output differs" } };
    }
    if (JSON.stringify(left.parameters) !== JSON.stringify(right.parameters)) {
      return { cases_passed: caseIndex, first_mismatch: { case_id: left.case_id, stage: "parameters", path: "parameters", expected: left.parameters, actual: right.parameters, reason: "parameter contract differs" } };
    }
    const expectedDomains = left.output.domains;
    const actualDomains = right.output.domains;
    for (const domain of ["point", "vertex", "primitive"] as const) {
      if (expectedDomains[domain].count !== actualDomains[domain].count) {
        return { cases_passed: caseIndex, first_mismatch: { case_id: left.case_id, stage: `output.${domain}`, path: `output.domains.${domain}.count`, expected: expectedDomains[domain].count, actual: actualDomains[domain].count, reason: "domain count differs" } };
      }
    }
    for (const [stage, leftRef, rightRef] of [
      ["output.vertex.point_indices", expectedDomains.vertex.point_indices, actualDomains.vertex.point_indices],
      ["output.primitive.vertex_counts", expectedDomains.primitive.vertex_counts, actualDomains.primitive.vertex_counts],
      ["output.primitive.closed", expectedDomains.primitive.closed, actualDomains.primitive.closed],
    ] as const) {
      const mismatch = await exactMismatch(left.case_id, stage, expectedRoot, leftRef, actualRoot, rightRef);
      if (mismatch) return { cases_passed: caseIndex, first_mismatch: mismatch };
    }
    const mismatch = compareMatchSizePositions(
      left.case_id,
      await readF32Ref(expectedRoot, expectedDomains.point.attributes.P.buffer),
      await readF32Ref(actualRoot, actualDomains.point.attributes.P.buffer),
    );
    if (mismatch) return { cases_passed: caseIndex, first_mismatch: mismatch };
  }
  if (expected.cases.length !== actual.cases.length) {
    return { cases_passed: expected.cases.length, first_mismatch: { case_id: "<capture>", stage: "identity", path: "cases.length", expected: expected.cases.length, actual: actual.cases.length, reason: "case count differs" } };
  }
  return { cases_passed: expected.cases.length, first_mismatch: null };
}

function flagValue(args: string[], name: string): string | undefined {
  const index = args.indexOf(name);
  return index >= 0 ? args[index + 1] : undefined;
}

export async function runHoudiniMatchSize(hotRoot: string, args: string[]): Promise<number> {
  const profile = flagValue(args, "--matrix") ?? "focused";
  if (profile !== "focused") {
    console.error(`Unsupported Match Size matrix '${profile}'; only focused is implemented.`);
    return 2;
  }
  const run = args.includes("--run");
  const json = args.includes("--json");
  const hython = flagValue(args, "--hython") ?? process.env.HYTHON ?? "F:\\houdini\\bin\\hython.exe";
  const reusedOracle = flagValue(args, "--oracle");
  const provider = join(hotRoot, "providers", "houdini_match_size_capture.py");
  const artifactRoot = process.env.C3D_DEVFLYWHEEL_ARTIFACT_ROOT;
  if (!artifactRoot) throw new Error("C3D_DEVFLYWHEEL_ARTIFACT_ROOT is not configured by the canonical wrapper.");
  const runDir = join(artifactRoot, "houdini", "match-size-sop", `hot_${profile}_${Date.now()}_${process.pid}`);
  const houdiniPath = reusedOracle ? resolve(reusedOracle) : join(runDir, "houdini_capture.json");
  const candidatePath = join(runDir, "typescript_capture.json");
  const receiptPath = join(runDir, "parity_receipt.json");
  const preview = {
    command: "hot houdini-match-size",
    language: "typescript",
    runtime: `bun ${Bun.version}`,
    cargo_invocations: 0,
    provider: { id: "houdini", executable: hython, execution_mode: "headless_node_network" },
    subject: { kind: "sop", id: "matchsize" },
    profile,
    artifact_dir: runDir,
    oracle_mode: reusedOracle ? "reused" : "captured",
    command_preview: reusedOracle ? null : [hython, provider, "--matrix", profile, "--output", houdiniPath],
    run,
  };
  if (!run) {
    console.log(json ? JSON.stringify(preview, null, 2) : JSON.stringify(preview));
    return 0;
  }
  if (reusedOracle && !existsSync(houdiniPath)) throw new Error(`Houdini oracle capture was not found: ${houdiniPath}`);
  if (!reusedOracle && !existsSync(hython)) throw new Error(`hython.exe was not found: ${hython}`);
  await mkdir(runDir, { recursive: true });
  const started = performance.now();
  let providerMs = 0;
  if (!reusedOracle) {
    const providerStarted = performance.now();
    const providerResult = Bun.spawnSync([hython, provider, "--matrix", profile, "--output", houdiniPath], { cwd: hotRoot });
    await Promise.all([
      writeFile(join(runDir, "houdini_stdout.log"), providerResult.stdout),
      writeFile(join(runDir, "houdini_stderr.log"), providerResult.stderr),
    ]);
    if (providerResult.exitCode !== 0) {
      console.error(`Houdini Match Size capture failed with ${providerResult.exitCode}: ${new TextDecoder().decode(providerResult.stderr)}`);
      return 1;
    }
    providerMs = performance.now() - providerStarted;
  }
  const oracleRoot = dirname(houdiniPath);
  const houdini = JSON.parse(await readFile(houdiniPath, "utf8")) as MatchSizeCapture;
  const candidateStarted = performance.now();
  const candidate: MatchSizeCapture = {
    schema: houdini.schema,
    provider: { id: "typescript-algorithm-prototype", version: Bun.version },
    subject: houdini.subject,
    profile: "focused",
    cases: [],
    provenance: { runtime: "bun", language: "typescript", cargo_invocations: 0 },
  };
  for (const source of houdini.cases) candidate.cases.push(await applyFocusedMatchSize(oracleRoot, source, runDir));
  await writeFile(candidatePath, `${JSON.stringify(candidate, null, 2)}\n`);
  const candidateMs = performance.now() - candidateStarted;
  const compareStarted = performance.now();
  const comparison = await compareCaptures(oracleRoot, houdini, runDir, candidate);
  const compareMs = performance.now() - compareStarted;
  const passed = comparison.first_mismatch === null;
  const receipt = {
    schema: "c3d.parity.receipt.v1",
    provider: houdini.provider,
    implementation: candidate.provider,
    subject: houdini.subject,
    profile,
    evidence_level: passed ? "algorithm-focused-parity" : "implemented",
    passed,
    algorithm_passed: passed,
    cunning_geometry_roundtrip: "not_run",
    cases_total: houdini.cases.length,
    cases_compared: passed ? houdini.cases.length : comparison.cases_passed + 1,
    cases_passed: comparison.cases_passed,
    cases_failed: passed ? 0 : 1,
    cases_unchecked: passed ? 0 : houdini.cases.length - comparison.cases_passed - 1,
    comparison_order: ["point.count", "vertex.point_indices", "primitive.vertex_counts", "primitive.closed", "point.P"],
    topology_comparison: "exact",
    tolerance: { absolute: ABS_TOLERANCE, relative: REL_TOLERANCE },
    first_mismatch: comparison.first_mismatch,
    hashes: {
      houdini_capture_sha256: sha256(await readFile(houdiniPath)),
      typescript_capture_sha256: sha256(await readFile(candidatePath)),
    },
    timing_ms: { houdini_capture: providerMs, typescript_candidate: candidateMs, comparison: compareMs, total: performance.now() - started },
    hot_loop: { language: "typescript", runtime: `bun ${Bun.version}`, cargo_invocations: 0 },
    oracle_mode: reusedOracle ? "reused" : "captured",
    artifacts: { root: runDir, houdini_capture: houdiniPath, typescript_capture: candidatePath, receipt: receiptPath },
  };
  await writeFile(receiptPath, `${JSON.stringify(receipt, null, 2)}\n`);
  console.log(json ? JSON.stringify(receipt, null, 2) : JSON.stringify(receipt));
  return passed ? 0 : 1;
}
