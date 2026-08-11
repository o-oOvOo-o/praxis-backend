import { existsSync } from "node:fs";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { sha256, type GeometrySide } from "./core";
import { readFacetMesh } from "./facet/mesh";
import { applyFacetPipeline } from "./facet/pipeline";
import { FACET_DEFAULT_PARAMETERS, type FacetMesh, type FacetParameters } from "./facet/types";

interface BenchmarkCase { case_id: string; parameters: FacetParameters; input: GeometrySide }
interface FacetCapture { profile: "focused" | "semantic"; cases: BenchmarkCase[] }
interface Stats { median_ms: number; p95_ms: number; min_ms: number }

function flagValue(args: string[], name: string): string | undefined {
  const index = args.indexOf(name);
  return index >= 0 ? args[index + 1] : undefined;
}

function percentile(values: number[], fraction: number): number {
  const ordered = values.slice().sort((left, right) => left - right);
  return ordered[Math.min(ordered.length - 1, Math.floor((ordered.length - 1) * fraction))];
}

function stats(values: number[]): Stats {
  const ordered = values.slice().sort((left, right) => left - right);
  const middle = Math.floor(ordered.length / 2);
  return {
    median_ms: ordered.length % 2 ? ordered[middle] : (ordered[middle - 1] + ordered[middle]) / 2,
    p95_ms: percentile(values, 0.95),
    min_ms: ordered[0],
  };
}

function stressGrid(size: number): FacetMesh {
  const positions = new Float32Array(size * size * 3);
  for (let row = 0; row < size; row++) for (let column = 0; column < size; column++) {
    const point = row * size + column;
    positions[point * 3] = column;
    positions[point * 3 + 1] = row;
  }
  const primitiveCount = (size - 1) * (size - 1);
  const pointIndices = new Uint32Array(primitiveCount * 4);
  let vertex = 0;
  for (let row = 0; row < size - 1; row++) for (let column = 0; column < size - 1; column++) {
    const first = row * size + column;
    pointIndices.set([first, first + 1, first + size + 1, first + size], vertex);
    vertex += 4;
  }
  const vertexCounts = new Uint32Array(primitiveCount); vertexCounts.fill(4);
  const closed = new Uint8Array(primitiveCount); closed.fill(1);
  return {
    pointCount: size * size, pointIndices, vertexCounts, closed,
    pointAttributes: { P: { storage: "f32", tupleSize: 3, values: positions } },
    vertexAttributes: {}, primitiveAttributes: {}, detailAttributes: {},
    pointGroups: {}, vertexGroups: {}, primitiveGroups: {},
  };
}

function stressCases(size: number): Array<BenchmarkCase & { mesh: FacetMesh }> {
  const mesh = stressGrid(size);
  return [
    ["stress/grid_post_normals", { postnml: 1 }],
    ["stress/grid_unique", { unique: 1 }],
    ["stress/grid_cusp", { cusp: 1, angle: 30 }],
    ["stress/grid_orient", { orientPolys: 1 }],
    ["stress/grid_consolidate_zero", { cons: 2, dist: 0 }],
  ].map(([case_id, overrides]) => ({
    case_id: case_id as string,
    parameters: { ...FACET_DEFAULT_PARAMETERS, ...(overrides as Partial<FacetParameters>) },
    input: {} as GeometrySide,
    mesh,
  }));
}

export async function runFacetBenchmark(hotRoot: string, args: string[]): Promise<number> {
  const profile = flagValue(args, "--matrix") ?? "semantic";
  if (profile !== "focused" && profile !== "semantic" && profile !== "stress") throw new Error(`Unsupported Facet benchmark matrix '${profile}'.`);
  const oracleArgument = flagValue(args, "--oracle");
  const oraclePath = oracleArgument ? resolve(oracleArgument) : null;
  if (profile !== "stress" && (!oraclePath || !existsSync(oraclePath))) throw new Error("Facet benchmark requires --oracle <houdini_capture.json>.");
  const stressSize = Number(flagValue(args, "--stress-size") ?? 64);
  if (!Number.isInteger(stressSize) || stressSize < 2) throw new Error("Facet benchmark stress size is invalid.");
  const warmup = Number(flagValue(args, "--warmup") ?? 20);
  const iterations = Number(flagValue(args, "--iterations") ?? 100);
  if (!Number.isInteger(warmup) || warmup < 1 || !Number.isInteger(iterations) || iterations < 2) throw new Error("Facet benchmark warmup/iterations are invalid.");
  const hython = flagValue(args, "--hython") ?? process.env.HYTHON ?? "F:\\houdini\\bin\\hython.exe";
  if (!existsSync(hython)) throw new Error(`hython.exe was not found: ${hython}`);
  const artifactRoot = process.env.C3D_DEVFLYWHEEL_ARTIFACT_ROOT;
  if (!artifactRoot) throw new Error("C3D_DEVFLYWHEEL_ARTIFACT_ROOT is not configured by the canonical wrapper.");
  const runDir = join(artifactRoot, "houdini", "facet-sop", `hot_benchmark_${Date.now()}_${process.pid}`);
  await mkdir(runDir, { recursive: true });
  const houdiniPath = join(runDir, "houdini_benchmark.json");
  const provider = join(hotRoot, "providers", "houdini_facet_benchmark.py");
  const providerResult = Bun.spawnSync([hython, provider, "--matrix", profile, "--stress-size", String(stressSize), "--warmup", String(warmup), "--iterations", String(iterations), "--output", houdiniPath], { cwd: dirname(provider) });
  await Promise.all([
    writeFile(join(runDir, "houdini_stdout.log"), providerResult.stdout),
    writeFile(join(runDir, "houdini_stderr.log"), providerResult.stderr),
  ]);
  if (providerResult.exitCode !== 0) throw new Error(new TextDecoder().decode(providerResult.stderr));

  const cases = profile === "stress"
    ? stressCases(stressSize)
    : await (async () => {
      const oracle = JSON.parse(await readFile(oraclePath!, "utf8")) as FacetCapture;
      if (oracle.profile !== profile) throw new Error(`Oracle profile '${oracle.profile}' does not match benchmark profile '${profile}'.`);
      const oracleRoot = dirname(oraclePath!);
      return Promise.all(oracle.cases.map(async (testCase) => ({ ...testCase, mesh: await readFacetMesh(oracleRoot, testCase.input.domains) })));
    })();
  for (let repetition = 0; repetition < warmup; repetition++) for (const testCase of cases) applyFacetPipeline(testCase.mesh, testCase.parameters);
  const caseSamples = new Map(cases.map((testCase) => [testCase.case_id, [] as number[]]));
  const batchSamples: number[] = [];
  for (let repetition = 0; repetition < iterations; repetition++) {
    const batchStarted = performance.now();
    for (const testCase of cases) {
      const started = performance.now();
      applyFacetPipeline(testCase.mesh, testCase.parameters);
      caseSamples.get(testCase.case_id)!.push(performance.now() - started);
    }
    batchSamples.push(performance.now() - batchStarted);
  }
  const caseStats = Object.fromEntries([...caseSamples].map(([caseId, values]) => [caseId, stats(values)]));
  const typescript = {
    schema: "c3d.facet.benchmark.v1", provider: { id: "typescript", runtime: `bun ${Bun.version}` }, profile,
    warmup, iterations, cases: cases.length, batch: stats(batchSamples),
    per_case_median_sum_ms: Object.values(caseStats).reduce((sum, item) => sum + item.median_ms, 0),
    case_stats: caseStats,
    scope: "preloaded typed geometry; applyFacetPipeline only; capture, comparison, and file I/O excluded",
  };
  const typescriptPath = join(runDir, "typescript_benchmark.json");
  await writeFile(typescriptPath, `${JSON.stringify(typescript, null, 2)}\n`);
  const houdini = JSON.parse(await readFile(houdiniPath, "utf8")) as { batch: Stats; cases: number; per_case_median_sum_ms: number };
  if (houdini.cases !== cases.length) throw new Error("Houdini and TypeScript benchmark case counts differ.");
  const receipt = {
    schema: "c3d.facet.benchmark.receipt.v1", subject: { kind: "sop", id: "facet" }, profile,
    warmup, iterations, cases: cases.length, stress_size: profile === "stress" ? stressSize : null,
    houdini_batch_median_ms: houdini.batch.median_ms,
    typescript_batch_median_ms: typescript.batch.median_ms,
    houdini_over_typescript_ratio: houdini.batch.median_ms / typescript.batch.median_ms,
    faster: houdini.batch.median_ms > typescript.batch.median_ms ? "typescript" : "houdini",
    scope: "warm pure-cook comparison on the same semantic case definitions; startup, capture, buffer I/O, and parity comparison excluded",
    cargo_invocations: 0,
    hashes: {
      ...(oraclePath ? { oracle_sha256: sha256(await readFile(oraclePath)) } : {}),
      houdini_benchmark_sha256: sha256(await readFile(houdiniPath)),
      typescript_benchmark_sha256: sha256(await readFile(typescriptPath)),
    },
    artifacts: { root: runDir, oracle: oraclePath, houdini: houdiniPath, typescript: typescriptPath },
  };
  const receiptPath = join(runDir, "benchmark_receipt.json");
  await writeFile(receiptPath, `${JSON.stringify({ ...receipt, artifacts: { ...receipt.artifacts, receipt: receiptPath } }, null, 2)}\n`);
  console.log(JSON.stringify({ ...receipt, artifacts: { ...receipt.artifacts, receipt: receiptPath } }, null, args.includes("--json") ? 2 : 0));
  return 0;
}
