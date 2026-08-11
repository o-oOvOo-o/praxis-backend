import { expect, test } from "bun:test";
import { boolean30CompareInvocation } from "../boolean30_compare";

test("Boolean30 comparator uses the canonical dedicated target", () => {
  expect(boolean30CompareInvocation("F:\\Cunning3D", ["--case", "focused/a"])).toEqual({
    executable: "F:\\cargo-target2\\Cunning3D-boolean30-compare\\debug\\boolean30-full-compare.exe",
    args: ["--case", "focused/a"],
    cwd: "F:\\Cunning3D",
  });
});
