import { arch, cpus, platform, release, totalmem } from "node:os";

function command(cmd: string[], cwd?: string): string {
  const proc = Bun.spawnSync({ cmd, cwd, stdout: "pipe", stderr: "pipe" });
  if (!proc.success) throw new Error(`${cmd.join(" ")}: ${proc.stderr.toString()}`);
  return proc.stdout.toString().trim();
}

export async function collectMetadata(paths: string[], binaries: readonly string[]) {
  const packages = (await Promise.all(["rust", "rust-next"].map(async (dir) => {
    const lock = Bun.TOML.parse(await Bun.file(`${dir}/Cargo.lock`).text()) as {
      package: { name: string; version: string; source?: string; checksum?: string }[];
    };
    return lock.package;
  }))).flat();
  const names = ["oxc_parser", "swc_ecma_parser", "swc_next_ecma_parser"];
  const swcNextCrates = packages.filter((p) => p.name.startsWith("swc_next_") && p.name !== "swc_next_parser");
  for (const pkg of swcNextCrates) {
    if (pkg.source !== "registry+https://github.com/rust-lang/crates.io-index" || !pkg.checksum) {
      throw new Error(`SWC Next must use published crates.io packages: ${pkg.name}`);
    }
  }
  const hash = async (path: string) => new Bun.CryptoHasher("sha256")
    .update(await Bun.file(path).arrayBuffer()).digest("hex");
  return {
    startedAt: new Date().toISOString(),
    system: {
      platform: platform(), release: release(), arch: arch(),
      cpu: cpus()[0]?.model ?? "Unknown CPU", cores: cpus().length,
      memoryGB: Math.round(totalmem() / 1024 ** 3),
    },
    toolchains: { bun: Bun.version, rustc: command(["rustc", "--version"]), zig: command(["zig", "version"]) },
    parsers: Object.fromEntries(packages.filter((p) => names.includes(p.name)).map((p) => [p.name, p.version])),
    swcNext: {
      registry: "crates.io",
      crates: swcNextCrates,
    },
    yuku: (await Bun.file("zig/build.zig.zon").text()).match(/\.url = "([^"]+)"/)![1],
    fixturesCommit: command(["git", "rev-parse", "HEAD"], "files"),
    warmup: 50, runs: 300,
    files: await Promise.all(paths.map(async (path) => ({ path, bytes: Bun.file(path).size, sha256: await hash(path) }))),
    binaries: await Promise.all(binaries.map(async (path) => ({ path, sha256: await hash(path) }))),
    cargoLockSha256: await hash("rust/Cargo.lock"),
    swcNextCargoLockSha256: await hash("rust-next/Cargo.lock"),
  };
}

export type BenchmarkMetadata = Awaited<ReturnType<typeof collectMetadata>>;
