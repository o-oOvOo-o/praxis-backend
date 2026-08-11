import { cloneMesh, validateMesh } from "./mesh";
import { computePointNormals, consolidatePointNormals, reversePointNormals, unitPointNormals } from "./normals";
import { makePolygonsPlanar } from "./planar";
import { resolveSelection } from "./selection";
import { consolidatePoints, cuspPoints, orientPolygons, removeDegeneratePrimitives, removeInlinePoints, uniquePoints } from "./topology";
import type { FacetMesh, FacetParameters } from "./types";

export function applyFacetPipeline(input: FacetMesh, parameters: FacetParameters): FacetMesh {
  validateMesh(input);
  const mesh = cloneMesh(input);
  const selection = () => resolveSelection(mesh, parameters);

  if (parameters.prenml) computePointNormals(mesh, selection());
  if (parameters.unit) unitPointNormals(mesh, selection());
  if (parameters.unique) uniquePoints(mesh, selection());
  if (parameters.cons === 1 || parameters.cons === 2) {
    consolidatePoints(mesh, selection(), parameters.dist, parameters.cons === 2 && parameters.accurate === 0);
  }
  if (parameters.cons === 3 || parameters.cons === 4) consolidatePointNormals(mesh, selection(), parameters.dist, parameters.cons === 4);
  if (parameters.inline) removeInlinePoints(mesh, selection(), parameters.inlinedist);
  if (parameters.orientPolys) orientPolygons(mesh, selection());
  if (parameters.cusp) cuspPoints(mesh, selection(), parameters.angle);
  if (parameters.remove) removeDegeneratePrimitives(mesh, selection());
  if (parameters.mkplanar) makePolygonsPlanar(mesh, selection());
  if (parameters.postnml) computePointNormals(mesh, selection());
  if (parameters.reversenml) reversePointNormals(mesh, selection());

  validateMesh(mesh);
  return mesh;
}
