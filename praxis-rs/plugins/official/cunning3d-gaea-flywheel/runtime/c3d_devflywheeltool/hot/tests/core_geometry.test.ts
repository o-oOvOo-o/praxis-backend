import { expect, test } from "bun:test";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { cloneDomains, readF32Ref, readI32Ref, readU8Ref, writeF32, writeI32, writeU32, writeU8, type GeometryDomains } from "../core";

test("geometry clone preserves generic numeric attributes and groups", async () => {
  const root = await mkdtemp(join(tmpdir(), "c3d-hot-geometry-"));
  try {
    const [position, normal, id, pointIndices, vertexCounts, closed, selected] = await Promise.all([
      writeF32(join(root, "P.f32le"), new Float32Array([0, 0, 0, 1, 0, 0, 0, 1, 0])),
      writeF32(join(root, "N.f32le"), new Float32Array([0, 0, 2, 0, 0, 2, 0, 0, 2])),
      writeI32(join(root, "id.i32le"), new Int32Array([10, 11, 12])),
      writeU32(join(root, "points.u32le"), new Uint32Array([0, 1, 2])),
      writeU32(join(root, "counts.u32le"), new Uint32Array([3])),
      writeU8(join(root, "closed.u8"), new Uint8Array([1])),
      writeU8(join(root, "selected.u8"), new Uint8Array([0, 1, 0])),
    ]);
    const domains: GeometryDomains = {
      point: {
        count: 3,
        attributes: {
          P: { storage: "f32", tuple_size: 3, buffer: position },
          N: { storage: "f32", tuple_size: 3, buffer: normal },
          id: { storage: "i32", tuple_size: 1, buffer: id },
        },
        groups: { selected },
      },
      vertex: { count: 3, point_indices: pointIndices, attributes: {}, groups: {} },
      primitive: { count: 1, vertex_counts: vertexCounts, closed, attributes: {}, groups: {} },
      detail: { attributes: {} },
    };
    const cloned = await cloneDomains(root, domains, root, "clone/mesh");
    expect(Array.from(await readF32Ref(root, cloned.point.attributes.N.buffer))).toEqual([0, 0, 2, 0, 0, 2, 0, 0, 2]);
    expect(Array.from(await readI32Ref(root, cloned.point.attributes.id.buffer))).toEqual([10, 11, 12]);
    expect(Array.from(await readU8Ref(root, cloned.point.groups.selected))).toEqual([0, 1, 0]);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
