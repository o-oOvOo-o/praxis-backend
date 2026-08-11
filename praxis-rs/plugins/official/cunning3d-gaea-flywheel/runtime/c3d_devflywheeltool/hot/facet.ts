import { existsSync } from "node:fs";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import {
  cloneDomains,
  compareExactBuffer,
  compareF32Buffer,
  readF32Ref,
  readU32Ref,
  sha256,
  type FirstMismatch,
  type GeometryDomains,
  type GeometrySide,
} from "./core";
import { readFacetMesh, writeFacetMesh } from "./facet/mesh";
import { applyFacetPipeline } from "./facet/pipeline";
import { FACET_DEFAULT_PARAMETERS, type FacetParameters } from "./facet/types";

export { FACET_DEFAULT_PARAMETERS, type FacetParameters } from "./facet/types";

export interface FacetCase {
  case_id: string;
  parameters: FacetParameters;
  input: GeometrySide;
  output?: GeometrySide;
}

interface FacetCapture {
  schema: "c3d.parity.capture.v1";
  provider: { id: string; version?: string; execution_mode?: string };
  subject: { kind: "sop"; id: "facet" };
  profile: "focused" | "semantic";
  parameter_contract?: Record<string, { default?: unknown[]; menu_items?: string[] }>;
  cases: FacetCase[];
  provenance: Record<string, unknown>;
}

const ABS_TOLERANCE = 1e-6;
const REL_TOLERANCE = 1e-6;

function validateFacetParameterContract(capture: FacetCapture): FirstMismatch | null {
  if (!capture.parameter_contract) return null;
  const defaults: Record<keyof FacetParameters, unknown> = {
    group: "", grouptype: 0, prenml: 0, unit: 0, unique: 0, cons: 0, dist: 0.001, accurate: 1,
    inline: 0, inlinedist: 0.001, orientPolys: 0, cusp: 0, angle: 20, remove: 0, mkplanar: 0,
    postnml: 0, reversenml: 0,
  };
  for (const [name, expected] of Object.entries(defaults)) {
    const actual = capture.parameter_contract[name]?.default?.[0];
    if (actual !== expected) return {
      case_id: "<parameter_contract>", stage: "parameter_contract", path: `parameter_contract.${name}.default[0]`,
      expected, actual, reason: "Houdini Facet default differs from the TypeScript contract",
    };
  }
  for (const [name, expected] of Object.entries({
    cons: ["none", "points", "fpoints", "normals", "fnormals"],
    grouptype: ["guess", "points", "prims"],
  })) {
    const actual = capture.parameter_contract[name]?.menu_items ?? [];
    if (JSON.stringify(actual) !== JSON.stringify(expected)) return {
      case_id: "<parameter_contract>", stage: "parameter_contract", path: `parameter_contract.${name}.menu_items`,
      expected, actual, reason: "Houdini Facet menu contract differs from the TypeScript contract",
    };
  }
  return null;
}

export async function applyFacet(oracleRoot: string, source: FacetCase, candidateRoot: string): Promise<FacetCase & { output: GeometrySide }> {
  const input = source.input.domains;
  const result = applyFacetPipeline(await readFacetMesh(oracleRoot, input), source.parameters);
  const slug = source.case_id.replaceAll("/", "_");
  const clonedInput = await cloneDomains(oracleRoot, input, candidateRoot, `typescript_buffers/${slug}.input`);
  const output = await writeFacetMesh(candidateRoot, `typescript_buffers/${slug}.output`, result);
  return { case_id: source.case_id, parameters: source.parameters, input: { domains: clonedInput }, output: { domains: output } };
}

export const applyFocusedFacet = applyFacet;

async function compareCaptures(expectedRoot: string, expected: FacetCapture, actualRoot: string, actual: FacetCapture): Promise<{ cases_passed: number; first_mismatch: FirstMismatch | null }> {
  const compareAttributes = async (
    caseId: string,
    domain: string,
    leftAttributes: GeometryDomains["point"]["attributes"] | undefined,
    rightAttributes: GeometryDomains["point"]["attributes"] | undefined,
  ): Promise<FirstMismatch | null> => {
    const left = leftAttributes ?? {};
    const right = rightAttributes ?? {};
    const leftNames = Object.keys(left).sort();
    const rightNames = Object.keys(right).sort();
    if (JSON.stringify(leftNames) !== JSON.stringify(rightNames)) {
      return { case_id: caseId, stage: `output.${domain}.attributes`, path: `output.${domain}.attributes.names`, expected: leftNames, actual: rightNames, reason: "attribute set differs" };
    }
    for (const name of leftNames) {
      const a = left[name];
      const b = right[name];
      if (a.storage !== b.storage || a.tuple_size !== b.tuple_size) {
        return { case_id: caseId, stage: `output.${domain}.attributes.${name}`, path: `output.${domain}.attributes.${name}.layout`, expected: { storage: a.storage, tuple_size: a.tuple_size }, actual: { storage: b.storage, tuple_size: b.tuple_size }, reason: "attribute layout differs" };
      }
      const stage = `output.${domain}.attributes.${name}`;
      const mismatch = a.storage === "f32"
        ? compareF32Buffer(caseId, stage, await readF32Ref(expectedRoot, a.buffer), await readF32Ref(actualRoot, b.buffer), ABS_TOLERANCE, REL_TOLERANCE)
        : await compareExactBuffer(caseId, stage, expectedRoot, a.buffer, actualRoot, b.buffer);
      if (mismatch) return mismatch;
    }
    return null;
  };
  const compareGroups = async (
    caseId: string,
    domain: string,
    leftGroups: GeometryDomains["point"]["groups"] | undefined,
    rightGroups: GeometryDomains["point"]["groups"] | undefined,
  ): Promise<FirstMismatch | null> => {
    const left = leftGroups ?? {};
    const right = rightGroups ?? {};
    const leftNames = Object.keys(left).sort();
    const rightNames = Object.keys(right).sort();
    if (JSON.stringify(leftNames) !== JSON.stringify(rightNames)) {
      return { case_id: caseId, stage: `output.${domain}.groups`, path: `output.${domain}.groups.names`, expected: leftNames, actual: rightNames, reason: "group set differs" };
    }
    for (const name of leftNames) {
      const mismatch = await compareExactBuffer(caseId, `output.${domain}.groups.${name}`, expectedRoot, left[name], actualRoot, right[name]);
      if (mismatch) return mismatch;
    }
    return null;
  };
  const contractMismatch = validateFacetParameterContract(expected);
  if (contractMismatch) return { cases_passed: 0, first_mismatch: contractMismatch };
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
    const a = left.output.domains;
    const b = right.output.domains;
    for (const domain of ["point", "vertex", "primitive"] as const) {
      if (a[domain].count !== b[domain].count) return { cases_passed: caseIndex, first_mismatch: { case_id: left.case_id, stage: `output.${domain}`, path: `output.${domain}.count`, expected: a[domain].count, actual: b[domain].count, reason: "domain count differs" } };
    }
    for (const [stage, leftRef, rightRef] of [
      ["output.primitive.vertex_counts", a.primitive.vertex_counts, b.primitive.vertex_counts],
      ["output.primitive.closed", a.primitive.closed, b.primitive.closed],
    ] as const) {
      const mismatch = await compareExactBuffer(left.case_id, stage, expectedRoot, leftRef, actualRoot, rightRef);
      if (mismatch) return { cases_passed: caseIndex, first_mismatch: mismatch };
    }
    const leftPoints = await readU32Ref(expectedRoot, a.vertex.point_indices);
    const rightPoints = await readU32Ref(actualRoot, b.vertex.point_indices);
    const canonicalize = (values: Uint32Array): Uint32Array => {
      const labels = new Map<number, number>();
      return Uint32Array.from(values, (value) => {
        let label = labels.get(value);
        if (label === undefined) labels.set(value, label = labels.size);
        return label;
      });
    };
    const leftSharing = canonicalize(leftPoints);
    const rightSharing = canonicalize(rightPoints);
    if (leftSharing.length !== rightSharing.length) {
      return { cases_passed: caseIndex, first_mismatch: { case_id: left.case_id, stage: "output.vertex.point_sharing", path: "output.vertex.point_sharing.length", expected: leftSharing.length, actual: rightSharing.length, reason: "vertex buffer length differs" } };
    }
    for (let vertex = 0; vertex < leftSharing.length; vertex++) {
      if (leftSharing[vertex] !== rightSharing[vertex]) {
        return { cases_passed: caseIndex, first_mismatch: { case_id: left.case_id, stage: "output.vertex.point_sharing", path: `output.vertex.point_sharing[${vertex}]`, expected: leftSharing[vertex], actual: rightSharing[vertex], reason: "point-sharing equivalence differs after canonical point relabeling" } };
      }
    }
    const mismatch = compareF32Buffer(left.case_id, "output.point.P", await readF32Ref(expectedRoot, a.point.attributes.P.buffer), await readF32Ref(actualRoot, b.point.attributes.P.buffer), ABS_TOLERANCE, REL_TOLERANCE);
    if (mismatch) return { cases_passed: caseIndex, first_mismatch: mismatch };
    for (const domain of ["point", "vertex", "primitive"] as const) {
      const attributeMismatch = await compareAttributes(left.case_id, domain, a[domain].attributes, b[domain].attributes);
      if (attributeMismatch) return { cases_passed: caseIndex, first_mismatch: attributeMismatch };
      const groupMismatch = await compareGroups(left.case_id, domain, a[domain].groups, b[domain].groups);
      if (groupMismatch) return { cases_passed: caseIndex, first_mismatch: groupMismatch };
    }
    const detailMismatch = await compareAttributes(left.case_id, "detail", a.detail.attributes, b.detail.attributes);
    if (detailMismatch) return { cases_passed: caseIndex, first_mismatch: detailMismatch };
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

export async function runHoudiniFacet(hotRoot: string, args: string[]): Promise<number> {
  const profile = flagValue(args, "--matrix") ?? "focused";
  if (profile !== "focused" && profile !== "semantic") return console.error(`Unsupported Facet matrix '${profile}'.`), 2;
  const run = args.includes("--run");
  const captureOnly = args.includes("--capture-only");
  const json = args.includes("--json");
  const hython = flagValue(args, "--hython") ?? process.env.HYTHON ?? "F:\\houdini\\bin\\hython.exe";
  const reusedOracle = flagValue(args, "--oracle");
  const artifactRoot = process.env.C3D_DEVFLYWHEEL_ARTIFACT_ROOT;
  if (!artifactRoot) throw new Error("C3D_DEVFLYWHEEL_ARTIFACT_ROOT is not configured by the canonical wrapper.");
  const runDir = join(artifactRoot, "houdini", "facet-sop", `hot_${profile}_${Date.now()}_${process.pid}`);
  const houdiniPath = reusedOracle ? resolve(reusedOracle) : join(runDir, "houdini_capture.json");
  const candidatePath = join(runDir, "typescript_capture.json");
  const repeatPath = join(runDir, "typescript_repeat_capture.json");
  const receiptPath = join(runDir, "parity_receipt.json");
  const provider = join(hotRoot, "providers", "houdini_facet_capture.py");
  if (!run) {
    console.log(JSON.stringify({ command: "hot houdini-facet", language: "typescript", runtime: `bun ${Bun.version}`, cargo_invocations: 0, subject: { kind: "sop", id: "facet" }, profile, artifact_dir: runDir, capture_only: captureOnly, run }, null, json ? 2 : 0));
    return 0;
  }
  if (reusedOracle && !existsSync(houdiniPath)) throw new Error(`Houdini oracle capture was not found: ${houdiniPath}`);
  if (!reusedOracle && !existsSync(hython)) throw new Error(`hython.exe was not found: ${hython}`);
  await mkdir(runDir, { recursive: true });
  const started = performance.now();
  let providerMs = 0;
  if (!reusedOracle) {
    const providerStarted = performance.now();
    const result = Bun.spawnSync([hython, provider, "--matrix", profile, "--output", houdiniPath], { cwd: hotRoot });
    await Promise.all([writeFile(join(runDir, "houdini_stdout.log"), result.stdout), writeFile(join(runDir, "houdini_stderr.log"), result.stderr)]);
    if (result.exitCode !== 0) return console.error(new TextDecoder().decode(result.stderr)), 1;
    providerMs = performance.now() - providerStarted;
  }
  const oracleRoot = dirname(houdiniPath);
  const houdini = JSON.parse(await readFile(houdiniPath, "utf8")) as FacetCapture;
  if (captureOnly) {
    console.log(JSON.stringify({ schema: houdini.schema, provider: houdini.provider, subject: houdini.subject, profile: houdini.profile, cases: houdini.cases.length, capture: houdiniPath, cargo_invocations: 0 }, null, json ? 2 : 0));
    return 0;
  }
  const candidateStarted = performance.now();
  const candidate: FacetCapture = { schema: houdini.schema, provider: { id: "typescript-algorithm-prototype", version: Bun.version }, subject: houdini.subject, profile, cases: [], provenance: { runtime: "bun", language: "typescript", cargo_invocations: 0 } };
  for (const source of houdini.cases) candidate.cases.push(await applyFacet(oracleRoot, source, runDir));
  await writeFile(candidatePath, `${JSON.stringify(candidate, null, 2)}\n`);
  const candidateMs = performance.now() - candidateStarted;
  const compareStarted = performance.now();
  const comparison = await compareCaptures(oracleRoot, houdini, runDir, candidate);
  let determinismComparison: Awaited<ReturnType<typeof compareCaptures>> = { cases_passed: 0, first_mismatch: null };
  let repeatCandidate: FacetCapture | null = null;
  const repeatRoot = join(runDir, "determinism_repeat");
  if (comparison.first_mismatch === null) {
    repeatCandidate = { ...candidate, cases: [], provenance: { ...candidate.provenance, repetition: 2 } };
    for (const source of houdini.cases) repeatCandidate.cases.push(await applyFacet(oracleRoot, source, repeatRoot));
    await writeFile(repeatPath, `${JSON.stringify(repeatCandidate, null, 2)}\n`);
    determinismComparison = await compareCaptures(runDir, candidate, repeatRoot, repeatCandidate);
  }
  const compareMs = performance.now() - compareStarted;
  const algorithmPassed = comparison.first_mismatch === null;
  const deterministic = algorithmPassed && determinismComparison.first_mismatch === null;
  const passed = algorithmPassed && deterministic;
  const firstMismatch = comparison.first_mismatch ?? determinismComparison.first_mismatch;
  const receipt = {
    schema: "c3d.parity.receipt.v1", provider: houdini.provider, implementation: candidate.provider, subject: houdini.subject, profile,
    evidence_level: passed ? (profile === "semantic" ? "matrix-parity" : "focused-parity") : "implemented", passed, algorithm_passed: algorithmPassed, cunning_geometry_roundtrip: "not_run",
    deterministic, determinism_cases_passed: determinismComparison.cases_passed,
    cases_total: houdini.cases.length, cases_passed: comparison.cases_passed, cases_failed: passed ? 0 : 1,
    comparison_order: ["parameter_contract", "domain.counts", "primitive.vertex_counts", "primitive.closed", "vertex.point_sharing", "all_numeric_attributes", "all_groups"],
    topology_comparison: "exact primitive/vertex order and point-sharing equivalence; output point labels are canonicalized",
    point_number_policy: "renumbering-invariant because Houdini cusp ownership depends on hidden upstream point representative state",
    tolerance: { absolute: ABS_TOLERANCE, relative: REL_TOLERANCE }, first_mismatch: firstMismatch,
    hashes: {
      houdini_capture_sha256: sha256(await readFile(houdiniPath)),
      typescript_capture_sha256: sha256(await readFile(candidatePath)),
      ...(repeatCandidate ? { typescript_repeat_capture_sha256: sha256(await readFile(repeatPath)) } : {}),
    },
    timing_ms: { houdini_capture: providerMs, typescript_candidate: candidateMs, comparison: compareMs, total: performance.now() - started },
    hot_loop: { language: "typescript", runtime: `bun ${Bun.version}`, cargo_invocations: 0 }, oracle_mode: reusedOracle ? "reused" : "captured",
    artifacts: { root: runDir, houdini_capture: houdiniPath, typescript_capture: candidatePath, typescript_repeat_capture: repeatCandidate ? repeatPath : null, receipt: receiptPath },
  };
  await writeFile(receiptPath, `${JSON.stringify(receipt, null, 2)}\n`);
  console.log(json ? JSON.stringify(receipt, null, 2) : JSON.stringify(receipt));
  return passed ? 0 : 1;
}
