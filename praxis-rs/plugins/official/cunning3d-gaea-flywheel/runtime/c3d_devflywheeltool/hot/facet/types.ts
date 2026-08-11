import type { ScalarType, TypedValues } from "../core";

export interface FacetParameters {
  group: string;
  grouptype: 0 | 1 | 2;
  prenml: 0 | 1;
  unit: 0 | 1;
  unique: 0 | 1;
  cons: 0 | 1 | 2 | 3 | 4;
  dist: number;
  accurate: 0 | 1;
  inline: 0 | 1;
  inlinedist: number;
  orientPolys: 0 | 1;
  cusp: 0 | 1;
  angle: number;
  remove: 0 | 1;
  mkplanar: 0 | 1;
  postnml: 0 | 1;
  reversenml: 0 | 1;
}

export const FACET_DEFAULT_PARAMETERS: FacetParameters = {
  group: "", grouptype: 0, prenml: 0, unit: 0, unique: 0, cons: 0, dist: 0.001, accurate: 1,
  inline: 0, inlinedist: 0.001, orientPolys: 0, cusp: 0, angle: 20, remove: 0, mkplanar: 0,
  postnml: 0, reversenml: 0,
};

export interface NumericAttribute {
  storage: ScalarType;
  tupleSize: number;
  values: TypedValues;
}

export type NumericAttributes = Record<string, NumericAttribute>;
export type ElementGroups = Record<string, Uint8Array>;

export interface FacetMesh {
  pointCount: number;
  pointIndices: Uint32Array;
  vertexCounts: Uint32Array;
  closed: Uint8Array;
  pointAttributes: NumericAttributes;
  vertexAttributes: NumericAttributes;
  primitiveAttributes: NumericAttributes;
  detailAttributes: NumericAttributes;
  pointGroups: ElementGroups;
  vertexGroups: ElementGroups;
  primitiveGroups: ElementGroups;
}

export interface FacetSelection {
  points: Uint8Array;
  primitives: Uint8Array;
  explicitPointGroup: boolean;
  explicitPrimitiveGroup: boolean;
}
