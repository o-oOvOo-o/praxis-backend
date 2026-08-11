import type { FacetMesh, FacetSelection, NumericAttribute } from "./types";

type Vec3 = [number, number, number];

function position(mesh: FacetMesh): Float32Array {
  const attribute = mesh.pointAttributes.P;
  if (!attribute || attribute.storage !== "f32" || attribute.tupleSize !== 3 || !(attribute.values instanceof Float32Array)) {
    throw new Error("Facet requires float3 point positions.");
  }
  return attribute.values;
}

function normalAttribute(mesh: FacetMesh): NumericAttribute | undefined {
  const attribute = mesh.pointAttributes.N;
  if (!attribute) return undefined;
  if (attribute.storage !== "f32" || attribute.tupleSize !== 3 || !(attribute.values instanceof Float32Array)) {
    throw new Error("Facet N must be a float3 point attribute.");
  }
  return attribute;
}

function normalize(x: number, y: number, z: number): Vec3 {
  const length = Math.hypot(x, y, z);
  return length > 0 ? [x / length, y / length, z / length] : [x, y, z];
}

function faceNormal(mesh: FacetMesh, start: number, count: number): Vec3 {
  const positions = position(mesh);
  let x = 0, y = 0, z = 0;
  for (let local = 0; local < count; local++) {
    const a = mesh.pointIndices[start + local] * 3;
    const b = mesh.pointIndices[start + ((local + 1) % count)] * 3;
    x += (positions[a + 1] - positions[b + 1]) * (positions[a + 2] + positions[b + 2]);
    y += (positions[a + 2] - positions[b + 2]) * (positions[a] + positions[b]);
    z += (positions[a] - positions[b]) * (positions[a + 1] + positions[b + 1]);
  }
  const unit = normalize(x, y, z);
  return [-unit[0], -unit[1], -unit[2]];
}

export function computePointNormals(mesh: FacetMesh, selection: FacetSelection): void {
  const previous = normalAttribute(mesh)?.values as Float32Array | undefined;
  const values = previous ? new Float32Array(previous) : new Float32Array(mesh.pointCount * 3);
  const sums = new Float64Array(mesh.pointCount * 3);
  const touched = new Uint8Array(mesh.pointCount);
  let cursor = 0;
  for (let primitive = 0; primitive < mesh.vertexCounts.length; primitive++) {
    const count = mesh.vertexCounts[primitive];
    if (selection.primitives[primitive] && mesh.closed[primitive] && count >= 3) {
      const normal = faceNormal(mesh, cursor, count);
      for (let local = 0; local < count; local++) {
        const point = mesh.pointIndices[cursor + local];
        if (!selection.points[point]) continue;
        sums[point * 3] += normal[0];
        sums[point * 3 + 1] += normal[1];
        sums[point * 3 + 2] += normal[2];
        touched[point] = 1;
      }
    }
    cursor += count;
  }
  for (let point = 0; point < mesh.pointCount; point++) {
    if (!touched[point]) continue;
    const normal = normalize(sums[point * 3], sums[point * 3 + 1], sums[point * 3 + 2]);
    values[point * 3] = normal[0];
    values[point * 3 + 1] = normal[1];
    values[point * 3 + 2] = normal[2];
  }
  mesh.pointAttributes.N = { storage: "f32", tupleSize: 3, values };
}

export function unitPointNormals(mesh: FacetMesh, selection: FacetSelection): void {
  const attribute = normalAttribute(mesh);
  if (!attribute) return;
  const values = attribute.values as Float32Array;
  for (let point = 0; point < mesh.pointCount; point++) {
    if (!selection.points[point]) continue;
    const normal = normalize(values[point * 3], values[point * 3 + 1], values[point * 3 + 2]);
    values[point * 3] = normal[0];
    values[point * 3 + 1] = normal[1];
    values[point * 3 + 2] = normal[2];
  }
}

export function reversePointNormals(mesh: FacetMesh, selection: FacetSelection): void {
  const attribute = normalAttribute(mesh);
  if (!attribute) return;
  const values = attribute.values as Float32Array;
  for (let point = 0; point < mesh.pointCount; point++) {
    if (!selection.points[point]) continue;
    values[point * 3] = -values[point * 3];
    values[point * 3 + 1] = -values[point * 3 + 1];
    values[point * 3 + 2] = -values[point * 3 + 2];
  }
}

function spatialClusters(mesh: FacetMesh, selection: FacetSelection, distance: number, legacyBoxDistance = false): number[][] {
  const positions = position(mesh);
  const limit2 = distance * distance;
  const clusters: number[][] = [];
  for (let point = 0; point < mesh.pointCount; point++) {
    if (!selection.points[point]) continue;
    let target: number[] | undefined;
    for (const cluster of clusters) {
      const representative = cluster[0];
      const dx = Math.abs(positions[representative * 3] - positions[point * 3]);
      const dy = Math.abs(positions[representative * 3 + 1] - positions[point * 3 + 1]);
      const dz = Math.abs(positions[representative * 3 + 2] - positions[point * 3 + 2]);
      const close = legacyBoxDistance ? Math.max(dx, dy, dz) <= distance : dx * dx + dy * dy + dz * dz <= limit2;
      if (close) { target = cluster; break; }
    }
    if (target) target.push(point); else clusters.push([point]);
  }
  return clusters;
}

export function consolidatePointNormals(mesh: FacetMesh, selection: FacetSelection, distance: number, normalizeResult: boolean): void {
  if (distance < 0) throw new Error("Facet consolidate distance must be non-negative.");
  const attribute = normalAttribute(mesh);
  if (!attribute) return;
  const values = attribute.values as Float32Array;
  for (const cluster of spatialClusters(mesh, selection, distance)) {
    if (cluster.length < 2) continue;
    let x = 0, y = 0, z = 0;
    for (const point of cluster) {
      x += values[point * 3]; y += values[point * 3 + 1]; z += values[point * 3 + 2];
    }
    x /= cluster.length; y /= cluster.length; z /= cluster.length;
    if (normalizeResult) [x, y, z] = normalize(x, y, z);
    for (const point of cluster) {
      values[point * 3] = x; values[point * 3 + 1] = y; values[point * 3 + 2] = z;
    }
  }
}

export { spatialClusters };
