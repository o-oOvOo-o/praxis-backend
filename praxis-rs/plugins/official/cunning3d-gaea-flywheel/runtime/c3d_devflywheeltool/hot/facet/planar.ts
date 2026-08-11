import type { FacetMesh, FacetSelection } from "./types";

export function makePolygonsPlanar(mesh: FacetMesh, selection: FacetSelection): void {
  const positions = mesh.pointAttributes.P.values as Float32Array;
  let cursor = 0;
  for (let primitive = 0; primitive < mesh.vertexCounts.length; primitive++) {
    const count = mesh.vertexCounts[primitive];
    if (selection.primitives[primitive] && mesh.closed[primitive] && count > 3) {
      const first = mesh.pointIndices[cursor];
      const second = mesh.pointIndices[cursor + 1];
      const last = mesh.pointIndices[cursor + count - 1];
      const ax = positions[first * 3], ay = positions[first * 3 + 1], az = positions[first * 3 + 2];
      const ux = positions[second * 3] - ax, uy = positions[second * 3 + 1] - ay, uz = positions[second * 3 + 2] - az;
      const vx = positions[last * 3] - ax, vy = positions[last * 3 + 1] - ay, vz = positions[last * 3 + 2] - az;
      const nx = uy * vz - uz * vy;
      const ny = uz * vx - ux * vz;
      const nz = ux * vy - uy * vx;
      const normalLength2 = nx * nx + ny * ny + nz * nz;
      if (normalLength2 > 0) {
        for (let local = 2; local < count - 1; local++) {
          const point = mesh.pointIndices[cursor + local];
          if (!selection.points[point]) continue;
          const offset = ((positions[point * 3] - ax) * nx + (positions[point * 3 + 1] - ay) * ny + (positions[point * 3 + 2] - az) * nz) / normalLength2;
          positions[point * 3] -= offset * nx;
          positions[point * 3 + 1] -= offset * ny;
          positions[point * 3 + 2] -= offset * nz;
        }
      }
    }
    cursor += count;
  }
}
