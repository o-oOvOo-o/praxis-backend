import { expect, test } from "bun:test";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { applyFacet, FACET_DEFAULT_PARAMETERS, type FacetCase } from "../facet";
import { readF32Ref, readI32Ref, readU32Ref, readU8Ref, writeF32, writeI32, writeU32, writeU8, type GeometryDomains } from "../core";

async function hinge(root: string): Promise<GeometryDomains> {
  const positions = await writeF32(
    join(root, "hinge.P.f32le"),
    new Float32Array([
      0, 0, 0,
      1, 0, 0,
      1, 1, 0,
      0, 1, 0,
      1, 1, 1,
      1, 0, 1,
    ]),
  );
  const pointIndices = await writeU32(
    join(root, "hinge.point_indices.u32le"),
    new Uint32Array([0, 1, 2, 3, 1, 5, 4, 2]),
  );
  const vertexCounts = await writeU32(join(root, "hinge.vertex_counts.u32le"), new Uint32Array([4, 4]));
  const closed = await writeU8(join(root, "hinge.closed.u8"), new Uint8Array([1, 1]));
  return {
    point: { count: 6, attributes: { P: { storage: "f32", tuple_size: 3, buffer: positions } } },
    vertex: { count: 8, point_indices: pointIndices },
    primitive: { count: 2, vertex_counts: vertexCounts, closed },
    detail: { attributes: {} },
  };
}

function source(input: GeometryDomains, angle: number): FacetCase {
  return {
    case_id: `focused/cusp_${angle}`,
    parameters: { ...FACET_DEFAULT_PARAMETERS, cusp: 1, angle },
    input: { domains: input },
  };
}

test("Facet cusp splits a shared 90-degree edge below the threshold", async () => {
  const root = await mkdtemp(join(tmpdir(), "c3d-hot-facet-"));
  try {
    const candidate = await applyFacet(root, source(await hinge(root), 60), root);
    const domains = candidate.output.domains;
    expect(domains.point.count).toBe(8);
    expect(Array.from(await readU32Ref(root, domains.vertex.point_indices))).toEqual([0, 1, 2, 3, 7, 5, 4, 6]);
    expect(Array.from(await readF32Ref(root, domains.point.attributes.P.buffer)).slice(18)).toEqual([1, 1, 0, 1, 0, 0]);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("Facet cusp preserves sharing when the threshold exceeds the dihedral angle", async () => {
  const root = await mkdtemp(join(tmpdir(), "c3d-hot-facet-"));
  try {
    const input = await hinge(root);
    const candidate = await applyFacet(root, source(input, 120), root);
    expect(candidate.output.domains.point.count).toBe(6);
    expect(candidate.output.domains.vertex.point_indices.sha256).toBe(input.vertex.point_indices.sha256);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("Facet Unique duplicates point attributes and group membership", async () => {
  const root = await mkdtemp(join(tmpdir(), "c3d-hot-facet-"));
  try {
    const input = await hinge(root);
    input.point.attributes.id = { storage: "i32", tuple_size: 1, buffer: await writeI32(join(root, "hinge.id.i32le"), new Int32Array([0, 1, 2, 3, 4, 5])) };
    input.point.groups = { selected: await writeU8(join(root, "hinge.selected.u8"), new Uint8Array([0, 1, 1, 0, 0, 0])) };
    const candidate = await applyFacet(root, {
      case_id: "semantic/unique_all",
      parameters: { ...FACET_DEFAULT_PARAMETERS, unique: 1 },
      input: { domains: input },
    }, root);
    const output = candidate.output.domains;
    expect(output.point.count).toBe(8);
    expect(Array.from(await readI32Ref(root, output.point.attributes.id.buffer))).toEqual([0, 1, 2, 3, 4, 5, 2, 1]);
    expect(Array.from(await readU8Ref(root, output.point.groups!.selected))).toEqual([0, 1, 1, 0, 0, 0, 1, 1]);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("Facet Consolidate Points preserves representative positions and averages float attributes", async () => {
  const root = await mkdtemp(join(tmpdir(), "c3d-hot-facet-"));
  try {
    const positions = await writeF32(join(root, "near.P.f32le"), new Float32Array([
      0, 0, 0, 1, 0, 0, 0, 1, 0,
      0.0005, 0, 0, 1.0004, 0, 0, 0, 1.0003, 0,
    ]));
    const weight = await writeF32(join(root, "near.weight.f32le"), new Float32Array([0, 1, 2, 10, 11, 12]));
    const id = await writeI32(join(root, "near.id.i32le"), new Int32Array([0, 1, 2, 3, 4, 5]));
    const input: GeometryDomains = {
      point: { count: 6, attributes: { P: { storage: "f32", tuple_size: 3, buffer: positions }, weight: { storage: "f32", tuple_size: 1, buffer: weight }, id: { storage: "i32", tuple_size: 1, buffer: id } } },
      vertex: { count: 6, point_indices: await writeU32(join(root, "near.points.u32le"), new Uint32Array([0, 1, 2, 3, 4, 5])) },
      primitive: { count: 2, vertex_counts: await writeU32(join(root, "near.counts.u32le"), new Uint32Array([3, 3])), closed: await writeU8(join(root, "near.closed.u8"), new Uint8Array([1, 1])) },
      detail: { attributes: {} },
    };
    const candidate = await applyFacet(root, {
      case_id: "semantic/consolidate_points",
      parameters: { ...FACET_DEFAULT_PARAMETERS, cons: 1, dist: 0.001 },
      input: { domains: input },
    }, root);
    const output = candidate.output.domains;
    expect(output.point.count).toBe(3);
    expect(Array.from(await readF32Ref(root, output.point.attributes.P.buffer))).toEqual([0, 0, 0, 1, 0, 0, 0, 1, 0]);
    expect(Array.from(await readF32Ref(root, output.point.attributes.weight.buffer))).toEqual([5, 6, 7]);
    expect(Array.from(await readI32Ref(root, output.point.attributes.id.buffer))).toEqual([0, 1, 2]);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
