import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { endianness } from "node:os";
import { basename, dirname, isAbsolute, relative, resolve } from "node:path";

export type ScalarType = "f32" | "i32" | "u32" | "u8";

export interface TypedBufferRef {
  path: string;
  scalar_type: ScalarType;
  length: number;
  sha256: string;
}

export interface NumericAttributeRef {
  storage: ScalarType;
  tuple_size: number;
  buffer: TypedBufferRef;
}

export type AttributeRefs = Record<string, NumericAttributeRef>;
export type GroupRefs = Record<string, TypedBufferRef>;

export interface GeometryDomains {
  point: {
    count: number;
    attributes: AttributeRefs & { P: { storage: "f32"; tuple_size: 3; buffer: TypedBufferRef } };
    groups?: GroupRefs;
  };
  vertex: { count: number; point_indices: TypedBufferRef; attributes?: AttributeRefs; groups?: GroupRefs };
  primitive: { count: number; vertex_counts: TypedBufferRef; closed: TypedBufferRef; attributes?: AttributeRefs; groups?: GroupRefs };
  detail: { attributes: AttributeRefs };
}

export interface GeometrySide {
  domains: GeometryDomains;
}

export interface FirstMismatch {
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

export type TypedValues = Float32Array | Int32Array | Uint32Array | Uint8Array;

function bytesOf(values: TypedValues): Uint8Array {
  if (endianness() !== "LE") throw new Error("The hot flywheel currently requires a little-endian host.");
  return new Uint8Array(values.buffer, values.byteOffset, values.byteLength);
}

export function sha256(bytes: Uint8Array | string): string {
  return createHash("sha256").update(bytes).digest("hex");
}

async function writeTyped(
  path: string,
  scalarType: ScalarType,
  values: TypedValues,
): Promise<TypedBufferRef> {
  await mkdir(dirname(path), { recursive: true });
  const bytes = bytesOf(values);
  await writeFile(path, bytes);
  return { path: basename(path), scalar_type: scalarType, length: values.length, sha256: sha256(bytes) };
}

export function writeF32(path: string, values: Float32Array): Promise<TypedBufferRef> {
  return writeTyped(path, "f32", values);
}

export function writeI32(path: string, values: Int32Array): Promise<TypedBufferRef> {
  return writeTyped(path, "i32", values);
}

export function writeU32(path: string, values: Uint32Array): Promise<TypedBufferRef> {
  return writeTyped(path, "u32", values);
}

export function writeU8(path: string, values: Uint8Array): Promise<TypedBufferRef> {
  return writeTyped(path, "u8", values);
}

export function resolveBuffer(root: string, reference: TypedBufferRef): string {
  if (isAbsolute(reference.path)) throw new Error(`Buffer path must be relative: ${reference.path}`);
  const normalizedRoot = resolve(root);
  const path = resolve(normalizedRoot, reference.path);
  if (relative(normalizedRoot, path).startsWith("..")) throw new Error(`Buffer escapes capture root: ${reference.path}`);
  return path;
}

function resolveOutput(root: string, relativePath: string): string {
  if (isAbsolute(relativePath)) throw new Error(`Output path must be relative: ${relativePath}`);
  const normalizedRoot = resolve(root);
  const path = resolve(normalizedRoot, relativePath);
  if (relative(normalizedRoot, path).startsWith("..")) throw new Error(`Output escapes artifact root: ${relativePath}`);
  return path;
}

export async function validateBuffer(root: string, reference: TypedBufferRef): Promise<void> {
  const bytes = await readFile(resolveBuffer(root, reference));
  const width = reference.scalar_type === "u8" ? 1 : 4;
  if (bytes.byteLength !== reference.length * width || sha256(bytes) !== reference.sha256) {
    throw new Error(`Typed buffer identity mismatch: ${reference.path}`);
  }
}

export async function readF32(path: string): Promise<Float32Array> {
  const bytes = await readFile(path);
  if (bytes.byteLength % 4 !== 0) throw new Error(`Invalid f32 buffer byte length: ${path}`);
  return new Float32Array(bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength));
}

export async function readF32Ref(root: string, reference: TypedBufferRef): Promise<Float32Array> {
  if (reference.scalar_type !== "f32") throw new Error(`Expected f32 buffer: ${reference.path}`);
  await validateBuffer(root, reference);
  return readF32(resolveBuffer(root, reference));
}

export async function readU32Ref(root: string, reference: TypedBufferRef): Promise<Uint32Array> {
  if (reference.scalar_type !== "u32") throw new Error(`Expected u32 buffer: ${reference.path}`);
  await validateBuffer(root, reference);
  const bytes = await readFile(resolveBuffer(root, reference));
  return new Uint32Array(bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength));
}

export async function readI32Ref(root: string, reference: TypedBufferRef): Promise<Int32Array> {
  if (reference.scalar_type !== "i32") throw new Error(`Expected i32 buffer: ${reference.path}`);
  await validateBuffer(root, reference);
  const bytes = await readFile(resolveBuffer(root, reference));
  return new Int32Array(bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength));
}

export async function readU8Ref(root: string, reference: TypedBufferRef): Promise<Uint8Array> {
  if (reference.scalar_type !== "u8") throw new Error(`Expected u8 buffer: ${reference.path}`);
  await validateBuffer(root, reference);
  return new Uint8Array(await readFile(resolveBuffer(root, reference)));
}

export async function readTypedRef(root: string, reference: TypedBufferRef): Promise<TypedValues> {
  switch (reference.scalar_type) {
    case "f32": return readF32Ref(root, reference);
    case "i32": return readI32Ref(root, reference);
    case "u32": return readU32Ref(root, reference);
    case "u8": return readU8Ref(root, reference);
  }
}

export async function copyBuffer(
  sourceRoot: string,
  source: TypedBufferRef,
  destinationRoot: string,
  destinationPath: string,
): Promise<TypedBufferRef> {
  await validateBuffer(sourceRoot, source);
  const bytes = await readFile(resolveBuffer(sourceRoot, source));
  const path = resolveOutput(destinationRoot, destinationPath);
  await mkdir(dirname(path), { recursive: true });
  await writeFile(path, bytes);
  return { ...source, path: relative(destinationRoot, path).replaceAll("\\", "/") };
}

export async function cloneDomains(
  sourceRoot: string,
  source: GeometryDomains,
  destinationRoot: string,
  prefix: string,
): Promise<GeometryDomains> {
  const safeName = (name: string): string => `${name.replaceAll(/[^A-Za-z0-9_.-]/g, "_")}_${sha256(name).slice(0, 8)}`;
  const cloneAttributes = async (domain: string, attributes: AttributeRefs = {}): Promise<AttributeRefs> => Object.fromEntries(
    await Promise.all(Object.entries(attributes).map(async ([name, attribute]) => [
      name,
      { ...attribute, buffer: await copyBuffer(sourceRoot, attribute.buffer, destinationRoot, `${prefix}.${domain}.attr.${safeName(name)}.${attribute.storage}le`) },
    ])),
  );
  const cloneGroups = async (domain: string, groups: GroupRefs = {}): Promise<GroupRefs> => Object.fromEntries(
    await Promise.all(Object.entries(groups).map(async ([name, group]) => [
      name,
      await copyBuffer(sourceRoot, group, destinationRoot, `${prefix}.${domain}.group.${safeName(name)}.u8`),
    ])),
  );
  const [pointAttributes, vertexAttributes, primitiveAttributes, detailAttributes, pointGroups, vertexGroups, primitiveGroups, pointIndices, vertexCounts, closed] = await Promise.all([
    cloneAttributes("point", source.point.attributes),
    cloneAttributes("vertex", source.vertex.attributes),
    cloneAttributes("primitive", source.primitive.attributes),
    cloneAttributes("detail", source.detail.attributes),
    cloneGroups("point", source.point.groups),
    cloneGroups("vertex", source.vertex.groups),
    cloneGroups("primitive", source.primitive.groups),
    copyBuffer(sourceRoot, source.vertex.point_indices, destinationRoot, `${prefix}.vertex.point_indices.u32le`),
    copyBuffer(sourceRoot, source.primitive.vertex_counts, destinationRoot, `${prefix}.primitive.vertex_counts.u32le`),
    copyBuffer(sourceRoot, source.primitive.closed, destinationRoot, `${prefix}.primitive.closed.u8`),
  ]);
  return {
    point: { count: source.point.count, attributes: pointAttributes as GeometryDomains["point"]["attributes"], groups: pointGroups },
    vertex: { count: source.vertex.count, point_indices: pointIndices, attributes: vertexAttributes, groups: vertexGroups },
    primitive: { count: source.primitive.count, vertex_counts: vertexCounts, closed, attributes: primitiveAttributes, groups: primitiveGroups },
    detail: { attributes: detailAttributes },
  };
}

export async function writeRelativeF32(
  root: string,
  relativePath: string,
  values: Float32Array,
): Promise<TypedBufferRef> {
  const path = resolveOutput(root, relativePath);
  const reference = await writeF32(path, values);
  reference.path = relative(root, path).replaceAll("\\", "/");
  return reference;
}

export async function writeRelativeI32(root: string, relativePath: string, values: Int32Array): Promise<TypedBufferRef> {
  const path = resolveOutput(root, relativePath);
  const reference = await writeI32(path, values);
  reference.path = relative(root, path).replaceAll("\\", "/");
  return reference;
}

export async function writeRelativeU32(root: string, relativePath: string, values: Uint32Array): Promise<TypedBufferRef> {
  const path = resolveOutput(root, relativePath);
  const reference = await writeU32(path, values);
  reference.path = relative(root, path).replaceAll("\\", "/");
  return reference;
}

export async function writeRelativeU8(root: string, relativePath: string, values: Uint8Array): Promise<TypedBufferRef> {
  const path = resolveOutput(root, relativePath);
  const reference = await writeU8(path, values);
  reference.path = relative(root, path).replaceAll("\\", "/");
  return reference;
}

export async function writeRelativeTyped(
  root: string,
  relativePath: string,
  scalarType: ScalarType,
  values: TypedValues,
): Promise<TypedBufferRef> {
  const path = resolveOutput(root, relativePath);
  const reference = await writeTyped(path, scalarType, values);
  reference.path = relative(root, path).replaceAll("\\", "/");
  return reference;
}

export async function compareExactBuffer(
  caseId: string,
  stage: string,
  expectedRoot: string,
  expectedRef: TypedBufferRef,
  actualRoot: string,
  actualRef: TypedBufferRef,
): Promise<FirstMismatch | null> {
  if (expectedRef.scalar_type !== actualRef.scalar_type) {
    return { case_id: caseId, stage, path: `${stage}.scalar_type`, expected: expectedRef.scalar_type, actual: actualRef.scalar_type, reason: "scalar type differs" };
  }
  if (expectedRef.scalar_type === "f32") throw new Error(`Use float comparison for ${stage}.`);
  const read = (root: string, reference: TypedBufferRef) => reference.scalar_type === "u32"
    ? readU32Ref(root, reference)
    : reference.scalar_type === "i32"
      ? readI32Ref(root, reference)
      : readU8Ref(root, reference);
  const expected = await read(expectedRoot, expectedRef);
  const actual = await read(actualRoot, actualRef);
  if (expected.length !== actual.length) {
    return { case_id: caseId, stage, path: `${stage}.length`, expected: expected.length, actual: actual.length, reason: "buffer length differs" };
  }
  for (let index = 0; index < expected.length; index++) {
    if (expected[index] !== actual[index]) {
      return { case_id: caseId, stage, path: `${stage}[${index}]`, expected: expected[index], actual: actual[index], reason: "discrete buffer differs" };
    }
  }
  return null;
}

export function compareF32Buffer(
  caseId: string,
  stage: string,
  expected: Float32Array,
  actual: Float32Array,
  absoluteTolerance: number,
  relativeTolerance: number,
): FirstMismatch | null {
  if (expected.length !== actual.length) {
    return { case_id: caseId, stage, path: `${stage}.length`, expected: expected.length, actual: actual.length, reason: "buffer length differs" };
  }
  for (let index = 0; index < expected.length; index++) {
    const absoluteError = Math.abs(expected[index] - actual[index]);
    const tolerance = absoluteTolerance + relativeTolerance * Math.max(Math.abs(expected[index]), Math.abs(actual[index]));
    if (!Number.isFinite(absoluteError) || absoluteError > tolerance) {
      return {
        case_id: caseId,
        stage,
        path: `${stage}[${index}]`,
        expected: expected[index],
        actual: actual[index],
        absolute_error: absoluteError,
        absolute_tolerance: absoluteTolerance,
        relative_tolerance: relativeTolerance,
      };
    }
  }
  return null;
}
