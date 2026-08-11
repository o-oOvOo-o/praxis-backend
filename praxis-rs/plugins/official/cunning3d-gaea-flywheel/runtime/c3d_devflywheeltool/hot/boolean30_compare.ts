import { existsSync } from "node:fs";
import { win32 } from "node:path";

export function boolean30CompareInvocation(repoRoot: string, args: string[]) {
  const executable = win32.join(
    win32.parse(repoRoot).root,
    "cargo-target2",
    "Cunning3D-boolean30-compare",
    "debug",
    "boolean30-full-compare.exe",
  );
  return { executable, args, cwd: repoRoot };
}

export async function runBoolean30Compare(args: string[]): Promise<number> {
  const repoRoot = process.env.CUNNING3D_ROOT;
  if (!repoRoot) {
    console.error("CUNNING3D_ROOT is missing; invoke through c3d-flywheel.ps1.");
    return 2;
  }
  const run = args.includes("--run");
  const forwarded = args.filter((argument) => argument !== "--run" && argument !== "--json");
  const invocation = boolean30CompareInvocation(repoRoot, forwarded);
  if (!run) {
    console.log(JSON.stringify({
      command: "hot houdini-boolean30-compare",
      ...invocation,
      run: false,
    }, null, 2));
    return 0;
  }
  if (!existsSync(invocation.executable)) {
    console.error(`Boolean30 comparator is missing: ${invocation.executable}`);
    console.error("Build it with the canonical Cunning3D-boolean30-compare target before running.");
    return 2;
  }
  const result = Bun.spawnSync([invocation.executable, ...invocation.args], {
    cwd: invocation.cwd,
    env: process.env,
    stdout: "inherit",
    stderr: "inherit",
  });
  return result.exitCode;
}
