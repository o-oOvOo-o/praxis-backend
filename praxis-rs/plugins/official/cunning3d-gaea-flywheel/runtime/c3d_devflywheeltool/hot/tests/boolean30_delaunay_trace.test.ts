import { expect, test } from "bun:test";
import { compareBoolean30DelaunayTrace } from "../boolean30_delaunay_trace";

const nativeLog = [
  "work after point=4 counts=8/3 triangles=[{off:0,idx:0,pts:[4,5,6]},{off:1,idx:1,pts:[4,6,7]}]",
  "work after point=2 counts=8/3 triangles=[{off:0,idx:0,pts:[2,5,6]},{off:1,idx:1,pts:[4,6,7]}]",
].join("\n");

const rustTrace = {
  schema: "c3d.boolean30.delaunay-trace.v1" as const,
  case_id: "focused/trace",
  source_set: 0,
  source_triangle: 0,
  steps: [
    { point: 4, constraint: null, triangles: [[4, 5, 6], [4, 6, 7]] },
    { point: 2, constraint: null, triangles: [[2, 5, 6], [4, 6, 7]] },
  ],
};

test("Boolean30 Delaunay trace reports exact native slot parity", () => {
  expect(compareBoolean30DelaunayTrace(nativeLog, rustTrace)).toEqual({
    schema: "c3d.boolean30.delaunay-trace-compare.v1",
    case_id: "focused/trace",
    source_set: 0,
    source_triangle: 0,
    native_steps: 2,
    rust_steps: 2,
    compared_steps: 2,
    exact: true,
    first_mismatch: null,
  });
});

test("Boolean30 Delaunay trace localizes the first differing slot", () => {
  const changed = structuredClone(rustTrace);
  changed.steps[1].triangles[1] = [4, 7, 6];
  expect(compareBoolean30DelaunayTrace(nativeLog, changed).first_mismatch).toEqual({
    step: 1,
    point: 2,
    slot: 1,
    native: [4, 6, 7],
    rust: [4, 7, 6],
    reason: "triangle",
  });
});

test("Boolean30 Delaunay trace isolates and rebases the detailed native call", () => {
  const sharedWorkLog = [
    "call thread=7 input=[20,21,22] boundary=[80]",
    "call thread=7 input=[0,1,2,24,26] boundary=[81]",
    "work before point=56 counts=65/73 triangles=[{off:71,idx:71,pts:[1,2,3]},{off:72,idx:72,pts:[61,62,63]}]",
    "work after point=56 counts=65/75 triangles=[{off:71,idx:71,pts:[1,2,3]},{off:72,idx:72,pts:[56,61,62]},{off:73,idx:73,pts:[56,62,63]},{off:74,idx:74,pts:[56,63,61]}]",
    "work after point=58 counts=65/77 triangles=[{off:71,idx:71,pts:[1,2,3]},{off:72,idx:72,pts:[56,61,62]},{off:73,idx:73,pts:[58,62,63]},{off:74,idx:74,pts:[56,63,61]}]",
    "call thread=7 input=[0,2,3] boundary=[82]",
    "work after point=70 counts=74/80 triangles=[{off:79,idx:79,pts:[70,71,72]}]",
  ].join("\n");
  const localTrace = {
    ...rustTrace,
    steps: [
      { point: 0, constraint: null, triangles: [[0, 5, 6], [0, 6, 7], [0, 7, 5]] },
      { point: 2, constraint: null, triangles: [[0, 5, 6], [2, 6, 7], [0, 7, 5]] },
    ],
  };

  expect(compareBoolean30DelaunayTrace(sharedWorkLog, localTrace).exact).toBe(true);
});
