import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { runHoudiniFacet } from "./facet";
import { runFacetBenchmark } from "./facet_benchmark";
import { runHoudiniMatchSize } from "./match_size";
import { runBoolean30Selection } from "./boolean30_selection";
import { runBoolean30DelaunayTrace } from "./boolean30_delaunay_trace";
import { runBoolean30VertexRefmap } from "./boolean30_vertex_refmap";
import { runBoolean30CoplanarDefaults } from "./boolean30_coplanar_defaults";
import { runBoolean30Compare } from "./boolean30_compare";
import { runBoolean30Membership } from "./boolean30_membership";

const hotRoot = dirname(fileURLToPath(import.meta.url));
const [command = "help", ...args] = Bun.argv.slice(2);

if (command === "test") {
  const result = Bun.spawnSync([process.execPath, "test", join(hotRoot, "tests"), ...args], {
    cwd: hotRoot,
    stdout: "inherit",
    stderr: "inherit",
  });
  process.exit(result.exitCode);
}

if (command === "houdini-match-size") {
  process.exit(await runHoudiniMatchSize(hotRoot, args));
}

if (command === "houdini-facet") {
  process.exit(await runHoudiniFacet(hotRoot, args));
}

if (command === "houdini-facet-benchmark") {
  process.exit(await runFacetBenchmark(hotRoot, args));
}

if (command === "houdini-boolean30-selection") {
  process.exit(await runBoolean30Selection(args));
}

if (command === "houdini-boolean30-delaunay-trace") {
  process.exit(await runBoolean30DelaunayTrace(args));
}

if (command === "houdini-boolean30-vertex-refmap") {
  process.exit(await runBoolean30VertexRefmap(args));
}

if (command === "houdini-boolean30-coplanar-defaults") {
  process.exit(await runBoolean30CoplanarDefaults(args));
}

if (command === "houdini-boolean30-compare") {
  process.exit(await runBoolean30Compare(args));
}

if (command === "houdini-boolean30-membership") {
  process.exit(await runBoolean30Membership(args));
}

if (command === "houdini-verb-info") {
  const subjectIndex = args.indexOf("--subject");
  const subject = subjectIndex >= 0 ? args[subjectIndex + 1] : undefined;
  const hythonIndex = args.indexOf("--hython");
  const hython = hythonIndex >= 0 ? args[hythonIndex + 1] : process.env.HYTHON ?? "F:\\houdini\\bin\\hython.exe";
  if (!subject) {
    console.error("houdini-verb-info requires --subject NAME.");
    process.exit(2);
  }
  const result = Bun.spawnSync([hython, join(hotRoot, "providers", "houdini_verb_info.py"), "--subject", subject], {
    stdout: "inherit",
    stderr: "inherit",
  });
  process.exit(result.exitCode);
}

if (command === "help" || command === "toolbox") {
  console.log("TypeScript hot flywheel commands:");
  console.log("  test");
  console.log("  houdini-verb-info --subject NAME [--hython PATH]");
  console.log("  houdini-facet [--matrix focused|semantic] [--capture-only] [--hython PATH | --oracle PATH] [--run] [--json]");
  console.log("  houdini-facet-benchmark --oracle PATH [--matrix focused|semantic] [--warmup N] [--iterations N] [--json]");
  console.log("  houdini-match-size [--matrix focused] [--hython PATH | --oracle PATH] [--run] [--json]");
  console.log("  houdini-boolean30-selection --stage PATH --output PATH [--order classified|facet-index|a-then-b|b-then-a|native-superfacets] [--native-arrangement-log PATH [--selected-arrangement] | --rotation-actual PATH --decisions PATH --oracle PATH] [--run] [--json]");
  console.log("  houdini-boolean30-delaunay-trace --native-log PATH --rust-trace PATH --output PATH [--run] [--json]");
  console.log("  houdini-boolean30-vertex-refmap --trace PATH --actual PATH --oracle PATH --case ID --output PATH [--run] [--json]");
  console.log("  houdini-boolean30-coplanar-defaults --trace PATH --actual PATH --oracle PATH --case ID --output PATH [--run] [--json]");
  console.log("  houdini-boolean30-compare [--case ID] [--receipt PATH] [--selection-stage PATH] [--selection-decisions PATH] [--run] [--json]");
  console.log("  houdini-boolean30-membership --stage PATH --actual PATH --oracle PATH --case ID --output PATH [--run] [--json]");
  process.exit(0);
}

console.error(`Unknown TypeScript hot flywheel command: ${command}`);
process.exit(2);
