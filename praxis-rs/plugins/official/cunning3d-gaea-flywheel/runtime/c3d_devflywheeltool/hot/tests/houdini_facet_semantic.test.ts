import { expect, test } from "bun:test";
import { FACET_DEFAULT_PARAMETERS } from "../facet";
import { applyFacetPipeline } from "../facet/pipeline";
import type { FacetMesh } from "../facet/types";

function hinge(): FacetMesh {
  return {
    pointCount: 6,
    pointIndices: new Uint32Array([0, 1, 2, 3, 1, 5, 4, 2]),
    vertexCounts: new Uint32Array([4, 4]),
    closed: new Uint8Array([1, 1]),
    pointAttributes: {
      P: { storage: "f32", tupleSize: 3, values: new Float32Array([0, 0, 0, 1, 0, 0, 1, 1, 0, 0, 1, 0, 1, 1, 1, 1, 0, 1]) },
      id: { storage: "i32", tupleSize: 1, values: new Int32Array([0, 1, 2, 3, 4, 5]) },
      weight: { storage: "f32", tupleSize: 1, values: new Float32Array([0, .2, .4, .6, .8, 1]) },
      N: { storage: "f32", tupleSize: 3, values: new Float32Array([0, 0, 2, 0, 0, 3, 0, 0, 4, 0, 0, 5, 2, 0, 0, 3, 0, 0]) },
    },
    vertexAttributes: {}, primitiveAttributes: {}, detailAttributes: {},
    pointGroups: { selected: new Uint8Array([0, 1, 1, 0, 0, 0]) }, vertexGroups: {}, primitiveGroups: {},
  };
}

test("Facet Unique duplicates every point attribute and group in Houdini order", () => {
  const result = applyFacetPipeline(hinge(), { ...FACET_DEFAULT_PARAMETERS, unique: 1 });
  expect(result.pointCount).toBe(8);
  expect(Array.from(result.pointIndices)).toEqual([0, 1, 2, 3, 7, 5, 4, 6]);
  expect(Array.from(result.pointAttributes.id.values)).toEqual([0, 1, 2, 3, 4, 5, 2, 1]);
  expect(Array.from(result.pointGroups.selected)).toEqual([0, 1, 1, 0, 0, 0, 1, 1]);
});

test("Facet operation ordering distinguishes pre-normal then unique from post-normal", () => {
  const pre = applyFacetPipeline(hinge(), { ...FACET_DEFAULT_PARAMETERS, prenml: 1, unique: 1 });
  const post = applyFacetPipeline(hinge(), { ...FACET_DEFAULT_PARAMETERS, unique: 1, postnml: 1 });
  expect(Array.from(pre.pointAttributes.N.values).slice(18)).toEqual([
    Math.fround(Math.SQRT1_2), 0, Math.fround(-Math.SQRT1_2),
    Math.fround(Math.SQRT1_2), 0, Math.fround(-Math.SQRT1_2),
  ]);
  expect(Array.from(post.pointAttributes.N.values).slice(18)).toEqual([1, 0, 0, 1, 0, 0]);
});

test("Facet point selection limits Unique without losing unselected topology", () => {
  const result = applyFacetPipeline(hinge(), { ...FACET_DEFAULT_PARAMETERS, group: "selected", grouptype: 1, unique: 1 });
  expect(result.pointCount).toBe(8);
  expect(Array.from(result.pointIndices)).toEqual([0, 1, 2, 3, 7, 5, 4, 6]);
});

test("Facet primitive selection does not cusp an edge against an unselected neighbor", () => {
  const input = hinge();
  input.primitiveGroups.second = new Uint8Array([0, 1]);
  const result = applyFacetPipeline(input, { ...FACET_DEFAULT_PARAMETERS, group: "second", grouptype: 2, cusp: 1, angle: 60 });
  expect(result.pointCount).toBe(6);
  expect(Array.from(result.pointIndices)).toEqual(Array.from(input.pointIndices));
});

test("Facet rejects invalid cusp angles and distances", () => {
  expect(() => applyFacetPipeline(hinge(), { ...FACET_DEFAULT_PARAMETERS, cusp: 1, angle: -0.001 })).toThrow("between 0 and 180");
  expect(() => applyFacetPipeline(hinge(), { ...FACET_DEFAULT_PARAMETERS, cons: 1, dist: -0.001 })).toThrow("non-negative");
  expect(() => applyFacetPipeline(hinge(), { ...FACET_DEFAULT_PARAMETERS, inline: 1, inlinedist: -0.001 })).toThrow("non-negative");
});

test("Facet rejects corrupt geometry before executing a stage", () => {
  const input = hinge();
  input.pointIndices[0] = 99;
  expect(() => applyFacetPipeline(input, FACET_DEFAULT_PARAMETERS)).toThrow("out of range");
});

test("Facet cusp leaves open polygon vertices intact", () => {
  const input = hinge();
  input.closed[0] = 0;
  const result = applyFacetPipeline(input, { ...FACET_DEFAULT_PARAMETERS, cusp: 1, angle: 60 });
  expect(result.pointCount).toBe(6);
  expect(Array.from(result.pointIndices)).toEqual(Array.from(input.pointIndices));
});
