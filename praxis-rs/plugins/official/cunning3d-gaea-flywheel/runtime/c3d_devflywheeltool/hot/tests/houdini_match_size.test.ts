import { expect, test } from "bun:test";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  applyFocusedMatchSize,
  compareMatchSizePositions,
  readF32,
  writeF32,
  writeU32,
  type GeometryDomains,
  type MatchSizeCase,
} from "../match_size";

async function geometry(root: string, name: string, positions: number[]): Promise<GeometryDomains> {
  const position = await writeF32(join(root, `${name}.P.f32le`), new Float32Array(positions));
  const pointIndices = await writeU32(join(root, `${name}.points.u32le`), new Uint32Array([0, 1, 2]));
  const vertexCounts = await writeU32(join(root, `${name}.counts.u32le`), new Uint32Array([3]));
  const closedBytes = new Uint8Array([1]);
  await Bun.write(join(root, `${name}.closed.u8`), closedBytes);
  const closed = {
    path: `${name}.closed.u8`,
    scalar_type: "u8" as const,
    length: 1,
    sha256: new Bun.CryptoHasher("sha256").update(closedBytes).digest("hex"),
  };
  return {
    point: { count: positions.length / 3, attributes: { P: { storage: "f32", tuple_size: 3, buffer: position } } },
    vertex: { count: 3, point_indices: pointIndices },
    primitive: { count: 1, vertex_counts: vertexCounts, closed },
    detail: { attributes: {} },
  };
}

test("focused Match Size maps the source bounds to an explicit target box", async () => {
  const root = await mkdtemp(join(tmpdir(), "c3d-hot-match-size-"));
  try {
    const input = await geometry(root, "input", [0, 0, 0, 2, 4, 6, 1, 1, 1]);
    const source: MatchSizeCase = {
      case_id: "focused/explicit_nonuniform",
      parameters: {
        target_mode: "explicit",
        target_position: [10, 3, -4],
        target_size: [8, 2, 3],
        scale_to_fit: true,
        uniform_scale: false,
        scale_axis: "x",
        translate: true,
        source_justify: ["center", "center", "center"],
        target_justify: ["same", "same", "same"],
      },
      input: { domains: input },
      target: null,
    };
    const candidate = await applyFocusedMatchSize(root, source, root);
    const output = await readF32(join(root, candidate.output.domains.point.attributes.P.buffer.path));
    expect(Array.from(output)).toEqual([6, 2, -5.5, 14, 4, -2.5, 10, 2.5, -5]);
    expect(candidate.output.domains.vertex.point_indices.sha256).toBe(input.vertex.point_indices.sha256);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("focused Match Size uses second-input bounds for uniform X-axis scaling", async () => {
  const root = await mkdtemp(join(tmpdir(), "c3d-hot-match-size-"));
  try {
    const input = await geometry(root, "input", [0, 0, 0, 2, 4, 6, 1, 1, 1]);
    const target = await geometry(root, "target", [-8, 3, -4, -2, 13, 8, -5, 8, 2]);
    const source: MatchSizeCase = {
      case_id: "focused/second_input_uniform_x",
      parameters: {
        target_mode: "second_input",
        target_position: [999, 999, 999],
        target_size: [1, 1, 1],
        scale_to_fit: true,
        uniform_scale: true,
        scale_axis: "x",
        translate: true,
        source_justify: ["center", "center", "center"],
        target_justify: ["same", "same", "same"],
      },
      input: { domains: input },
      target: { domains: target },
    };
    const candidate = await applyFocusedMatchSize(root, source, root);
    const output = await readF32(join(root, candidate.output.domains.point.attributes.P.buffer.path));
    expect(Array.from(output)).toEqual([-8, 2, -7, -2, 14, 11, -5, 5, -4]);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("Match Size comparison reports the first point component outside tolerance", () => {
  expect(compareMatchSizePositions("focused/mismatch", new Float32Array([1, 2, 3]), new Float32Array([1, 2.01, 3]))).toEqual({
    case_id: "focused/mismatch",
    stage: "output.point.P",
    path: "output.domains.point.P[0].y",
    expected: 2,
    actual: expect.closeTo(2.01),
    absolute_error: expect.closeTo(0.01),
    absolute_tolerance: 1e-6,
    relative_tolerance: 1e-6,
  });
});
