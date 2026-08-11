import { allocateLike } from "./mesh";
import { spatialClusters } from "./normals";
import type { FacetMesh, FacetSelection, NumericAttributes, ElementGroups } from "./types";

function copyTuple(source: ArrayLike<number>, sourceElement: number, target: { [index: number]: number }, targetElement: number, tupleSize: number): void {
  for (let component = 0; component < tupleSize; component++) target[targetElement * tupleSize + component] = source[sourceElement * tupleSize + component];
}

function duplicatePointSources(mesh: FacetMesh, sources: number[], inheritGroups = true): void {
  if (sources.length === 0) return;
  for (const attribute of Object.values(mesh.pointAttributes)) {
    const output = allocateLike(attribute.values, (mesh.pointCount + sources.length) * attribute.tupleSize);
    output.set(attribute.values as never);
    sources.forEach((source, copy) => copyTuple(attribute.values, source, output, mesh.pointCount + copy, attribute.tupleSize));
    attribute.values = output;
  }
  for (const [name, group] of Object.entries(mesh.pointGroups)) {
    const output = new Uint8Array(mesh.pointCount + sources.length);
    output.set(group);
    if (inheritGroups) sources.forEach((source, copy) => output[mesh.pointCount + copy] = group[source]);
    mesh.pointGroups[name] = output;
  }
  mesh.pointCount += sources.length;
}

export function uniquePoints(mesh: FacetMesh, selection: FacetSelection): void {
  const occurrences = Array.from({ length: mesh.pointCount }, () => [] as number[]);
  for (let vertex = 0; vertex < mesh.pointIndices.length; vertex++) occurrences[mesh.pointIndices[vertex]].push(vertex);
  const copySources: number[] = [];
  const copyVertices: number[] = [];
  for (let point = mesh.pointCount - 1; point >= 0; point--) {
    if (!selection.points[point]) continue;
    for (const vertex of occurrences[point].slice(1)) {
      copySources.push(point);
      copyVertices.push(vertex);
    }
  }
  const firstCopy = mesh.pointCount;
  duplicatePointSources(mesh, copySources);
  copyVertices.forEach((vertex, copy) => mesh.pointIndices[vertex] = firstCopy + copy);
}

class DisjointSet {
  private readonly parent: Uint32Array;
  constructor(length: number) { this.parent = Uint32Array.from({ length }, (_, index) => index); }
  find(value: number): number {
    let root = value;
    while (this.parent[root] !== root) root = this.parent[root];
    while (this.parent[value] !== value) { const next = this.parent[value]; this.parent[value] = root; value = next; }
    return root;
  }
  union(left: number, right: number): void {
    const a = this.find(left), b = this.find(right);
    if (a !== b) this.parent[Math.max(a, b)] = Math.min(a, b);
  }
}

type Vec3 = [number, number, number];

function polygonNormal(mesh: FacetMesh, start: number, count: number): Vec3 {
  const positions = mesh.pointAttributes.P.values as Float32Array;
  let x = 0, y = 0, z = 0;
  for (let local = 0; local < count; local++) {
    const a = mesh.pointIndices[start + local] * 3;
    const b = mesh.pointIndices[start + ((local + 1) % count)] * 3;
    x += (positions[a + 1] - positions[b + 1]) * (positions[a + 2] + positions[b + 2]);
    y += (positions[a + 2] - positions[b + 2]) * (positions[a] + positions[b]);
    z += (positions[a] - positions[b]) * (positions[a + 1] + positions[b + 1]);
  }
  const length = Math.hypot(x, y, z);
  return length > 0 ? [x / length, y / length, z / length] : [0, 0, 0];
}

export function cuspPoints(mesh: FacetMesh, selection: FacetSelection, angleDegrees: number): void {
  if (!(angleDegrees >= 0 && angleDegrees <= 180)) throw new Error("Facet cusp angle must be between 0 and 180 degrees.");
  const starts = new Uint32Array(mesh.vertexCounts.length);
  const normals: Vec3[] = [];
  let cursor = 0;
  for (let primitive = 0; primitive < mesh.vertexCounts.length; primitive++) {
    starts[primitive] = cursor;
    normals.push(polygonNormal(mesh, cursor, mesh.vertexCounts[primitive]));
    cursor += mesh.vertexCounts[primitive];
  }
  const pointFaces = Array.from({ length: mesh.pointCount }, () => [] as number[]);
  const edges = new Map<string, number[]>();
  for (let primitive = 0; primitive < mesh.vertexCounts.length; primitive++) {
    const start = starts[primitive], count = mesh.vertexCounts[primitive];
    if (!mesh.closed[primitive] || count < 2) continue;
    for (let local = 0; local < count; local++) {
      const point = mesh.pointIndices[start + local];
      const next = mesh.pointIndices[start + ((local + 1) % count)];
      if (!pointFaces[point].includes(primitive)) pointFaces[point].push(primitive);
      const key = `${Math.min(point, next)}:${Math.max(point, next)}`;
      const faces = edges.get(key);
      if (faces) { if (!faces.includes(primitive)) faces.push(primitive); } else edges.set(key, [primitive]);
    }
  }
  const sets = pointFaces.map((faces) => new DisjointSet(faces.length));
  const slots = pointFaces.map((faces) => new Map(faces.map((face, index) => [face, index])));
  const cosine = Math.cos(angleDegrees * Math.PI / 180);
  for (const [key, faces] of edges) {
    if (faces.length !== 2) continue;
    const [left, right] = faces;
    const dot = normals[left][0] * normals[right][0] + normals[left][1] * normals[right][1] + normals[left][2] * normals[right][2];
    for (const point of key.split(":").map(Number)) {
      const selectedPair = selection.points[point]
        && (!selection.explicitPrimitiveGroup || (selection.primitives[left] && selection.primitives[right]));
      if (!selectedPair || dot + 1e-12 >= cosine) sets[point].union(slots[point].get(left)!, slots[point].get(right)!);
    }
  }
  const roots = pointFaces.map((faces, point) => {
    const distinct: number[] = [];
    for (const face of faces.slice().sort((a, b) => a - b)) {
      const root = sets[point].find(slots[point].get(face)!);
      if (!distinct.includes(root)) distinct.push(root);
    }
    return distinct;
  });
  const sourcePointCount = mesh.pointCount;
  const copies: number[] = [];
  const pointForRoot = roots.map((components, point) => new Map([[components[0], point]]));
  for (let point = sourcePointCount - 1; point >= 0; point--) {
    if (!selection.points[point]) continue;
    for (const root of roots[point].slice(1)) {
      pointForRoot[point].set(root, sourcePointCount + copies.length);
      copies.push(point);
    }
  }
  duplicatePointSources(mesh, copies, false);
  cursor = 0;
  for (let primitive = 0; primitive < mesh.vertexCounts.length; primitive++) {
    for (let local = 0; local < mesh.vertexCounts[primitive]; local++, cursor++) {
      const point = mesh.pointIndices[cursor];
      if (!selection.points[point]) continue;
      const slot = slots[point].get(primitive);
      if (slot === undefined) continue;
      const root = sets[point].find(slot);
      mesh.pointIndices[cursor] = pointForRoot[point].get(root)!;
    }
  }
}

function subsetAttributes(attributes: NumericAttributes, elements: number[]): void {
  for (const attribute of Object.values(attributes)) {
    const output = allocateLike(attribute.values, elements.length * attribute.tupleSize);
    elements.forEach((source, target) => copyTuple(attribute.values, source, output, target, attribute.tupleSize));
    attribute.values = output;
  }
}

function subsetGroups(groups: ElementGroups, elements: number[]): void {
  for (const [name, group] of Object.entries(groups)) groups[name] = Uint8Array.from(elements, (source) => group[source]);
}

export function compactPoints(mesh: FacetMesh): void {
  const used = new Uint8Array(mesh.pointCount);
  for (const point of mesh.pointIndices) used[point] = 1;
  const kept: number[] = [];
  const remap = new Uint32Array(mesh.pointCount);
  for (let point = 0; point < mesh.pointCount; point++) if (used[point]) { remap[point] = kept.length; kept.push(point); }
  for (let vertex = 0; vertex < mesh.pointIndices.length; vertex++) mesh.pointIndices[vertex] = remap[mesh.pointIndices[vertex]];
  subsetAttributes(mesh.pointAttributes, kept);
  subsetGroups(mesh.pointGroups, kept);
  mesh.pointCount = kept.length;
}

export function consolidatePoints(mesh: FacetMesh, selection: FacetSelection, distance: number, legacyBoxDistance = false): void {
  if (distance < 0) throw new Error("Facet consolidate distance must be non-negative.");
  const clusters = spatialClusters(mesh, selection, distance, legacyBoxDistance);
  const representative = Uint32Array.from({ length: mesh.pointCount }, (_, point) => point);
  const members = new Map<number, number[]>();
  for (const cluster of clusters) {
    const root = Math.min(...cluster);
    members.set(root, cluster);
    for (const point of cluster) representative[point] = root;
  }
  const kept: number[] = [];
  const outputForRepresentative = new Uint32Array(mesh.pointCount);
  for (let point = 0; point < mesh.pointCount; point++) {
    if (representative[point] === point) { outputForRepresentative[point] = kept.length; kept.push(point); }
  }
  for (let point = 0; point < mesh.pointCount; point++) outputForRepresentative[point] = outputForRepresentative[representative[point]];
  for (let vertex = 0; vertex < mesh.pointIndices.length; vertex++) mesh.pointIndices[vertex] = outputForRepresentative[mesh.pointIndices[vertex]];
  for (const [name, attribute] of Object.entries(mesh.pointAttributes)) {
    const output = allocateLike(attribute.values, kept.length * attribute.tupleSize);
    kept.forEach((root, target) => {
      const cluster = members.get(root) ?? [root];
      for (let component = 0; component < attribute.tupleSize; component++) {
        if (attribute.storage === "f32" && name !== "P") {
          let sum = 0;
          for (const point of cluster) sum += attribute.values[point * attribute.tupleSize + component];
          output[target * attribute.tupleSize + component] = sum / cluster.length;
        } else {
          output[target * attribute.tupleSize + component] = attribute.values[root * attribute.tupleSize + component];
        }
      }
    });
    attribute.values = output;
  }
  for (const [name, group] of Object.entries(mesh.pointGroups)) {
    mesh.pointGroups[name] = Uint8Array.from(kept, (root) => (members.get(root) ?? [root]).some((point) => group[point]) ? 1 : 0);
  }
  mesh.pointCount = kept.length;
  compactPoints(mesh);
}

function reorderVertices(mesh: FacetMesh, sourceVertices: number[]): void {
  mesh.pointIndices = Uint32Array.from(sourceVertices, (source) => mesh.pointIndices[source]);
  subsetAttributes(mesh.vertexAttributes, sourceVertices);
  subsetGroups(mesh.vertexGroups, sourceVertices);
}

export function removeInlinePoints(mesh: FacetMesh, selection: FacetSelection, distance: number): void {
  if (distance < 0) throw new Error("Facet inline distance must be non-negative.");
  const positions = mesh.pointAttributes.P.values as Float32Array;
  const outputVertices: number[] = [];
  const sourceCounts = new Uint32Array(mesh.vertexCounts);
  let cursor = 0;
  for (let primitive = 0; primitive < sourceCounts.length; primitive++) {
    const vertices = Array.from({ length: sourceCounts[primitive] }, (_, local) => cursor + local);
    if (selection.primitives[primitive] && mesh.closed[primitive]) {
      let changed = true;
      while (changed && vertices.length > 3) {
        changed = false;
        for (let local = 0; local < vertices.length; local++) {
          const vertex = vertices[local];
          const point = mesh.pointIndices[vertex];
          if (!selection.points[point]) continue;
          const previous = mesh.pointIndices[vertices[(local + vertices.length - 1) % vertices.length]];
          const next = mesh.pointIndices[vertices[(local + 1) % vertices.length]];
          const ax = positions[previous * 3], ay = positions[previous * 3 + 1], az = positions[previous * 3 + 2];
          const bx = positions[next * 3], by = positions[next * 3 + 1], bz = positions[next * 3 + 2];
          const px = positions[point * 3], py = positions[point * 3 + 1], pz = positions[point * 3 + 2];
          const dx = bx - ax, dy = by - ay, dz = bz - az;
          const length2 = dx * dx + dy * dy + dz * dz;
          const t = length2 > 0 ? Math.max(0, Math.min(1, ((px - ax) * dx + (py - ay) * dy + (pz - az) * dz) / length2)) : 0;
          const ex = px - (ax + t * dx), ey = py - (ay + t * dy), ez = pz - (az + t * dz);
          if (Math.hypot(ex, ey, ez) <= distance) { vertices.splice(local, 1); changed = true; break; }
        }
      }
    }
    mesh.vertexCounts[primitive] = vertices.length;
    outputVertices.push(...vertices);
    cursor += sourceCounts[primitive];
  }
  reorderVertices(mesh, outputVertices);
  compactPoints(mesh);
}

export function orientPolygons(mesh: FacetMesh, selection: FacetSelection): void {
  const starts = new Uint32Array(mesh.vertexCounts.length);
  let cursor = 0;
  const edgeUses = new Map<string, Array<{ primitive: number; a: number; b: number }>>();
  for (let primitive = 0; primitive < mesh.vertexCounts.length; primitive++) {
    starts[primitive] = cursor;
    const count = mesh.vertexCounts[primitive];
    if (selection.primitives[primitive] && mesh.closed[primitive]) {
      for (let local = 0; local < count; local++) {
        const a = mesh.pointIndices[cursor + local], b = mesh.pointIndices[cursor + ((local + 1) % count)];
        const key = `${Math.min(a, b)}:${Math.max(a, b)}`;
        const uses = edgeUses.get(key) ?? [];
        uses.push({ primitive, a, b }); edgeUses.set(key, uses);
      }
    }
    cursor += count;
  }
  const adjacency = Array.from({ length: mesh.vertexCounts.length }, () => [] as Array<{ primitive: number; xor: number }>);
  for (const uses of edgeUses.values()) {
    if (uses.length !== 2) continue;
    const [left, right] = uses;
    const same = left.a === right.a && left.b === right.b ? 1 : 0;
    adjacency[left.primitive].push({ primitive: right.primitive, xor: same });
    adjacency[right.primitive].push({ primitive: left.primitive, xor: same });
  }
  const flips = new Int8Array(mesh.vertexCounts.length); flips.fill(-1);
  for (let seed = 0; seed < mesh.vertexCounts.length; seed++) {
    if (!selection.primitives[seed] || flips[seed] !== -1) continue;
    flips[seed] = 0;
    const queue = [seed];
    for (let head = 0; head < queue.length; head++) {
      const primitive = queue[head];
      for (const edge of adjacency[primitive]) {
        const desired = flips[primitive] ^ edge.xor;
        if (flips[edge.primitive] === -1) { flips[edge.primitive] = desired; queue.push(edge.primitive); }
      }
    }
  }
  const order: number[] = [];
  for (let primitive = 0; primitive < mesh.vertexCounts.length; primitive++) {
    const start = starts[primitive], count = mesh.vertexCounts[primitive];
    if (count === 0) continue;
    order.push(start);
    if (flips[primitive] === 1) for (let local = count - 1; local >= 1; local--) order.push(start + local);
    else for (let local = 1; local < count; local++) order.push(start + local);
  }
  reorderVertices(mesh, order);
}

export function removeDegeneratePrimitives(mesh: FacetMesh, selection: FacetSelection): void {
  const keptPrimitives: number[] = [];
  const keptVertices: number[] = [];
  const counts: number[] = [];
  const closed: number[] = [];
  let cursor = 0;
  for (let primitive = 0; primitive < mesh.vertexCounts.length; primitive++) {
    const count = mesh.vertexCounts[primitive];
    const unique = new Set(mesh.pointIndices.slice(cursor, cursor + count));
    const degenerate = selection.primitives[primitive] && (mesh.closed[primitive] ? unique.size < 3 : unique.size < 2);
    if (!degenerate) {
      keptPrimitives.push(primitive); counts.push(count); closed.push(mesh.closed[primitive]);
      for (let local = 0; local < count; local++) keptVertices.push(cursor + local);
    }
    cursor += count;
  }
  reorderVertices(mesh, keptVertices);
  subsetAttributes(mesh.primitiveAttributes, keptPrimitives);
  subsetGroups(mesh.primitiveGroups, keptPrimitives);
  mesh.vertexCounts = Uint32Array.from(counts);
  mesh.closed = Uint8Array.from(closed);
  compactPoints(mesh);
}
