import type { FacetMesh, FacetParameters, FacetSelection } from "./types";

function filled(length: number): Uint8Array {
  const result = new Uint8Array(length);
  result.fill(1);
  return result;
}

function pointsFromPrimitives(mesh: FacetMesh, primitives: Uint8Array): Uint8Array {
  const points = new Uint8Array(mesh.pointCount);
  let cursor = 0;
  for (let primitive = 0; primitive < mesh.vertexCounts.length; primitive++) {
    for (let local = 0; local < mesh.vertexCounts[primitive]; local++, cursor++) {
      if (primitives[primitive]) points[mesh.pointIndices[cursor]] = 1;
    }
  }
  return points;
}

function primitivesFromPoints(mesh: FacetMesh, points: Uint8Array): Uint8Array {
  const primitives = new Uint8Array(mesh.vertexCounts.length);
  let cursor = 0;
  for (let primitive = 0; primitive < mesh.vertexCounts.length; primitive++) {
    for (let local = 0; local < mesh.vertexCounts[primitive]; local++, cursor++) {
      if (points[mesh.pointIndices[cursor]]) primitives[primitive] = 1;
    }
  }
  return primitives;
}

function numericPattern(pattern: string, length: number): Uint8Array | undefined {
  if (pattern === "*") return filled(length);
  const tokens = pattern.split(/[\s,]+/).filter(Boolean);
  if (tokens.length === 0 || tokens.some((token) => !/^\d+(?:-\d+)?$/.test(token))) return undefined;
  const result = new Uint8Array(length);
  for (const token of tokens) {
    const [rawStart, rawEnd = rawStart] = token.split("-").map(Number);
    const start = Math.min(rawStart, rawEnd), end = Math.max(rawStart, rawEnd);
    for (let index = start; index <= end && index < length; index++) result[index] = 1;
  }
  return result;
}

export function resolveSelection(mesh: FacetMesh, parameters: FacetParameters): FacetSelection {
  const name = parameters.group.trim();
  if (!name) return {
    points: filled(mesh.pointCount), primitives: filled(mesh.vertexCounts.length),
    explicitPointGroup: false, explicitPrimitiveGroup: false,
  };

  const pointGroup = mesh.pointGroups[name];
  const primitiveGroup = mesh.primitiveGroups[name];
  const numericPoints = parameters.grouptype === 1 ? numericPattern(name, mesh.pointCount) : undefined;
  const numericPrimitives = parameters.grouptype === 2 ? numericPattern(name, mesh.vertexCounts.length) : undefined;
  if (numericPoints) return { points: numericPoints, primitives: primitivesFromPoints(mesh, numericPoints), explicitPointGroup: true, explicitPrimitiveGroup: false };
  if (numericPrimitives) return { points: pointsFromPrimitives(mesh, numericPrimitives), primitives: numericPrimitives, explicitPointGroup: false, explicitPrimitiveGroup: true };
  if (pointGroup === undefined && primitiveGroup === undefined) return {
    points: filled(mesh.pointCount), primitives: filled(mesh.vertexCounts.length),
    explicitPointGroup: false, explicitPrimitiveGroup: false,
  };
  const usePoints = parameters.grouptype === 1 || (parameters.grouptype === 0 && pointGroup !== undefined);
  const usePrimitives = parameters.grouptype === 2 || (parameters.grouptype === 0 && !usePoints && primitiveGroup !== undefined);
  if (usePoints) {
    const points = pointGroup ? new Uint8Array(pointGroup) : new Uint8Array(mesh.pointCount);
    return { points, primitives: primitivesFromPoints(mesh, points), explicitPointGroup: true, explicitPrimitiveGroup: false };
  }
  if (usePrimitives) {
    const primitives = primitiveGroup ? new Uint8Array(primitiveGroup) : new Uint8Array(mesh.vertexCounts.length);
    return { points: pointsFromPrimitives(mesh, primitives), primitives, explicitPointGroup: false, explicitPrimitiveGroup: true };
  }
  return {
    points: new Uint8Array(mesh.pointCount), primitives: new Uint8Array(mesh.vertexCounts.length),
    explicitPointGroup: false, explicitPrimitiveGroup: false,
  };
}
