import {
  readTypedRef,
  readU32Ref,
  readU8Ref,
  sha256,
  writeRelativeTyped,
  type AttributeRefs,
  type GeometryDomains,
  type GroupRefs,
  type ScalarType,
  type TypedValues,
} from "../core";
import type { ElementGroups, FacetMesh, NumericAttribute, NumericAttributes } from "./types";

function copyValues(values: TypedValues): TypedValues {
  if (values instanceof Float32Array) return new Float32Array(values);
  if (values instanceof Int32Array) return new Int32Array(values);
  if (values instanceof Uint32Array) return new Uint32Array(values);
  return new Uint8Array(values);
}

export function allocateLike(values: TypedValues, length: number): TypedValues {
  if (values instanceof Float32Array) return new Float32Array(length);
  if (values instanceof Int32Array) return new Int32Array(length);
  if (values instanceof Uint32Array) return new Uint32Array(length);
  return new Uint8Array(length);
}

export function cloneAttribute(attribute: NumericAttribute): NumericAttribute {
  return { storage: attribute.storage, tupleSize: attribute.tupleSize, values: copyValues(attribute.values) };
}

function cloneAttributes(attributes: NumericAttributes): NumericAttributes {
  return Object.fromEntries(Object.entries(attributes).map(([name, attribute]) => [name, cloneAttribute(attribute)]));
}

function cloneGroups(groups: ElementGroups): ElementGroups {
  return Object.fromEntries(Object.entries(groups).map(([name, values]) => [name, new Uint8Array(values)]));
}

export function cloneMesh(mesh: FacetMesh): FacetMesh {
  return {
    pointCount: mesh.pointCount,
    pointIndices: new Uint32Array(mesh.pointIndices),
    vertexCounts: new Uint32Array(mesh.vertexCounts),
    closed: new Uint8Array(mesh.closed),
    pointAttributes: cloneAttributes(mesh.pointAttributes),
    vertexAttributes: cloneAttributes(mesh.vertexAttributes),
    primitiveAttributes: cloneAttributes(mesh.primitiveAttributes),
    detailAttributes: cloneAttributes(mesh.detailAttributes),
    pointGroups: cloneGroups(mesh.pointGroups),
    vertexGroups: cloneGroups(mesh.vertexGroups),
    primitiveGroups: cloneGroups(mesh.primitiveGroups),
  };
}

function validateAttribute(domain: string, name: string, count: number, attribute: NumericAttribute): void {
  if (!Number.isInteger(attribute.tupleSize) || attribute.tupleSize < 1) throw new Error(`${domain}.${name} has an invalid tuple size.`);
  if (attribute.values.length !== count * attribute.tupleSize) {
    throw new Error(`${domain}.${name} contains ${attribute.values.length} scalars for ${count} elements.`);
  }
  if (attribute.storage === "f32" && !(attribute.values instanceof Float32Array)) throw new Error(`${domain}.${name} storage/view mismatch.`);
  if (attribute.storage === "i32" && !(attribute.values instanceof Int32Array)) throw new Error(`${domain}.${name} storage/view mismatch.`);
  if (attribute.storage === "u32" && !(attribute.values instanceof Uint32Array)) throw new Error(`${domain}.${name} storage/view mismatch.`);
  if (attribute.storage === "u8" && !(attribute.values instanceof Uint8Array)) throw new Error(`${domain}.${name} storage/view mismatch.`);
}

export function validateMesh(mesh: FacetMesh): void {
  if (mesh.vertexCounts.length !== mesh.closed.length) throw new Error("Facet primitive topology buffers disagree.");
  const vertexCount = mesh.vertexCounts.reduce((sum, count) => sum + count, 0);
  if (vertexCount !== mesh.pointIndices.length) throw new Error("Facet vertex topology buffers disagree.");
  for (const point of mesh.pointIndices) if (point >= mesh.pointCount) throw new Error(`Facet point index ${point} is out of range.`);
  for (const [name, attribute] of Object.entries(mesh.pointAttributes)) validateAttribute("point", name, mesh.pointCount, attribute);
  for (const [name, attribute] of Object.entries(mesh.vertexAttributes)) validateAttribute("vertex", name, vertexCount, attribute);
  for (const [name, attribute] of Object.entries(mesh.primitiveAttributes)) validateAttribute("primitive", name, mesh.vertexCounts.length, attribute);
  for (const [name, attribute] of Object.entries(mesh.detailAttributes)) validateAttribute("detail", name, 1, attribute);
  for (const [domain, groups, count] of [
    ["point", mesh.pointGroups, mesh.pointCount],
    ["vertex", mesh.vertexGroups, vertexCount],
    ["primitive", mesh.primitiveGroups, mesh.vertexCounts.length],
  ] as const) {
    for (const [name, group] of Object.entries(groups)) if (group.length !== count) throw new Error(`${domain} group ${name} has the wrong length.`);
  }
  const position = mesh.pointAttributes.P;
  if (!position || position.storage !== "f32" || position.tupleSize !== 3) throw new Error("Facet requires a float3 point P attribute.");
}

async function readAttributes(root: string, refs: AttributeRefs = {}): Promise<NumericAttributes> {
  return Object.fromEntries(await Promise.all(Object.entries(refs).map(async ([name, ref]) => [
    name,
    { storage: ref.storage, tupleSize: ref.tuple_size, values: await readTypedRef(root, ref.buffer) },
  ])));
}

async function readGroups(root: string, refs: GroupRefs = {}): Promise<ElementGroups> {
  return Object.fromEntries(await Promise.all(Object.entries(refs).map(async ([name, ref]) => [name, await readU8Ref(root, ref)])));
}

export async function readFacetMesh(root: string, domains: GeometryDomains): Promise<FacetMesh> {
  const [pointIndices, vertexCounts, closed, pointAttributes, vertexAttributes, primitiveAttributes, detailAttributes, pointGroups, vertexGroups, primitiveGroups] = await Promise.all([
    readU32Ref(root, domains.vertex.point_indices),
    readU32Ref(root, domains.primitive.vertex_counts),
    readU8Ref(root, domains.primitive.closed),
    readAttributes(root, domains.point.attributes),
    readAttributes(root, domains.vertex.attributes),
    readAttributes(root, domains.primitive.attributes),
    readAttributes(root, domains.detail.attributes),
    readGroups(root, domains.point.groups),
    readGroups(root, domains.vertex.groups),
    readGroups(root, domains.primitive.groups),
  ]);
  const mesh: FacetMesh = {
    pointCount: domains.point.count, pointIndices, vertexCounts, closed,
    pointAttributes, vertexAttributes, primitiveAttributes, detailAttributes,
    pointGroups, vertexGroups, primitiveGroups,
  };
  validateMesh(mesh);
  return mesh;
}

function safeName(name: string): string {
  return `${name.replaceAll(/[^A-Za-z0-9_.-]/g, "_")}_${sha256(name).slice(0, 8)}`;
}

async function writeAttributes(root: string, prefix: string, domain: string, attributes: NumericAttributes): Promise<AttributeRefs> {
  return Object.fromEntries(await Promise.all(Object.entries(attributes).map(async ([name, attribute]) => {
    const path = `${prefix}.${domain}.attr.${safeName(name)}.${attribute.storage}le`;
    return [name, {
      storage: attribute.storage,
      tuple_size: attribute.tupleSize,
      buffer: await writeRelativeTyped(root, path, attribute.storage, attribute.values),
    }];
  })));
}

async function writeGroups(root: string, prefix: string, domain: string, groups: ElementGroups): Promise<GroupRefs> {
  return Object.fromEntries(await Promise.all(Object.entries(groups).map(async ([name, values]) => [
    name,
    await writeRelativeTyped(root, `${prefix}.${domain}.group.${safeName(name)}.u8`, "u8", values),
  ])));
}

export async function writeFacetMesh(root: string, prefix: string, mesh: FacetMesh): Promise<GeometryDomains> {
  validateMesh(mesh);
  const [pointAttributes, vertexAttributes, primitiveAttributes, detailAttributes, pointGroups, vertexGroups, primitiveGroups, pointIndices, vertexCounts, closed] = await Promise.all([
    writeAttributes(root, prefix, "point", mesh.pointAttributes),
    writeAttributes(root, prefix, "vertex", mesh.vertexAttributes),
    writeAttributes(root, prefix, "primitive", mesh.primitiveAttributes),
    writeAttributes(root, prefix, "detail", mesh.detailAttributes),
    writeGroups(root, prefix, "point", mesh.pointGroups),
    writeGroups(root, prefix, "vertex", mesh.vertexGroups),
    writeGroups(root, prefix, "primitive", mesh.primitiveGroups),
    writeRelativeTyped(root, `${prefix}.vertex.point_indices.u32le`, "u32", mesh.pointIndices),
    writeRelativeTyped(root, `${prefix}.primitive.vertex_counts.u32le`, "u32", mesh.vertexCounts),
    writeRelativeTyped(root, `${prefix}.primitive.closed.u8`, "u8", mesh.closed),
  ]);
  return {
    point: { count: mesh.pointCount, attributes: pointAttributes as GeometryDomains["point"]["attributes"], groups: pointGroups },
    vertex: { count: mesh.pointIndices.length, point_indices: pointIndices, attributes: vertexAttributes, groups: vertexGroups },
    primitive: { count: mesh.vertexCounts.length, vertex_counts: vertexCounts, closed, attributes: primitiveAttributes, groups: primitiveGroups },
    detail: { attributes: detailAttributes },
  };
}

export function typedArray(storage: ScalarType, values: ArrayLike<number>): TypedValues {
  switch (storage) {
    case "f32": return Float32Array.from(values);
    case "i32": return Int32Array.from(values);
    case "u32": return Uint32Array.from(values);
    case "u8": return Uint8Array.from(values);
  }
}
