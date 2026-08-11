import { expect, test } from "bun:test";
import {
  usesBoolean30DefaultPointAttributes,
  type Boolean30CoplanarPoint,
} from "../boolean30_coplanar_defaults";

const constructed: Boolean30CoplanarPoint = {
  direct_source_point: false,
  nearby_crossings: [
    { kind: "CoplanarBoundary" },
    { kind: "CoplanarBoundary" },
  ],
};

test("Boolean30 pure coplanar constructed points keep default attributes", () => {
  expect(usesBoolean30DefaultPointAttributes(constructed)).toBe(true);
});

test("Boolean30 direct source and transverse points retain transfer samples", () => {
  expect(usesBoolean30DefaultPointAttributes({ ...constructed, direct_source_point: true })).toBe(false);
  expect(usesBoolean30DefaultPointAttributes({
    ...constructed,
    nearby_crossings: [{ kind: "CoplanarBoundary" }, { kind: "Transverse" }],
  })).toBe(false);
  expect(usesBoolean30DefaultPointAttributes({ ...constructed, nearby_crossings: [] })).toBe(false);
});
