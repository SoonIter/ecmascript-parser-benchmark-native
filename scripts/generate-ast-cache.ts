const FILES = [
  "files/typescript.js",
  "files/checker.ts",
  "files/lib.dom.d.ts",
  "files/react.js",
];

const proc = Bun.spawnSync({
  cmd: ["./bin/rust", "--generate-ast-cache", ...FILES],
  stdout: "inherit",
  stderr: "inherit",
});
if (!proc.success) {
  throw new Error(`Oxc AST cache generation failed with code ${proc.exitCode}`);
}
