import { mkdir, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
import type { BenchmarkMetadata } from "./metadata";
import type { ChartConfiguration } from "chart.js";
import { ChartJSNodeCanvas } from "chartjs-node-canvas";

const metadata: BenchmarkMetadata = JSON.parse(await readFile("result/metadata.json", "utf-8"));
const FILES_SOURCE_URL_PREFIX =
  `https://raw.githubusercontent.com/yuku-toolchain/parser-benchmark-files/${metadata.fixturesCommit}`;

const PARSERS = {
  yuku: {
    name: "Yuku",
    language: "Zig",
    description:
      "A high-performance & spec-compliant JavaScript/TypeScript compiler toolchain written in Zig.",
    url: "https://github.com/yuku-toolchain/yuku",
    semantic: false,
  },
  oxc: {
    name: "Oxc",
    language: "Rust",
    description: "A high-performance JavaScript and TypeScript parser written in Rust.",
    url: "https://github.com/oxc-project/oxc",
    semantic: false,
  },
  swc: {
    name: "SWC",
    language: "Rust",
    description:
      "An extensible Rust-based platform for compiling and bundling JavaScript and TypeScript.",
    url: "https://github.com/swc-project/swc",
    semantic: false,
  },
  swc_next: {
    name: "SWC Next",
    language: "Rust",
    description: "The next-generation SWC parser, built from its published crates.io release.",
    url: "https://github.com/swc-project/swc-next",
    semantic: false,
  },
  yuku_semantic: {
    name: "Yuku + Semantic",
    language: "Zig",
    description: "Yuku parser with semantic analysis.",
    url: "https://github.com/yuku-toolchain/yuku",
    semantic: true,
  },
  oxc_semantic: {
    name: "Oxc + Semantic",
    language: "Rust",
    description: "Oxc parser with semantic analysis.",
    url: "https://github.com/oxc-project/oxc",
    semantic: true,
  },
  swc_next_semantic: {
    name: "SWC Next + Semantic",
    language: "Rust",
    description: "SWC Next parser with semantic analysis and scope-dependent syntax checks.",
    url: "https://github.com/swc-project/swc-next",
    semantic: true,
  },
} as const;

const CHART_COLORS: Record<string, string> = {
  yuku: "#FF6B35",
  oxc: "#F72585",
  swc: "#4CC9F0",
  swc_next: "#2DBD85",
  yuku_semantic: "#E8890C",
  oxc_semantic: "#B5179E",
  swc_next_semantic: "#218F65",
};

const FILES = {
  typescript: {
    path: "files/typescript.js",
    source_url: `${FILES_SOURCE_URL_PREFIX}/typescript.js`,
  },
  checker: {
    path: "files/checker.ts",
    source_url: `${FILES_SOURCE_URL_PREFIX}/checker.ts`,
  },
  lib_dom: {
    path: "files/lib.dom.d.ts",
    source_url: `${FILES_SOURCE_URL_PREFIX}/lib.dom.d.ts`,
  },
  react: {
    path: "files/react.js",
    source_url: `${FILES_SOURCE_URL_PREFIX}/react.js`,
  },
} as const;

type ParserKey = keyof typeof PARSERS;
type FileKey = keyof typeof FILES;

interface BenchmarkResult {
  parser: string;
  median: number;
  min: number;
  p99: number;
}

interface ParserEntry {
  key: string;
  name: string;
  result: BenchmarkResult | null;
}

function formatBytes(bytes: number): string {
  return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
}

function formatTime(seconds: number): string {
  return `${(seconds * 1000).toFixed(2)} ms`;
}

function formatThroughput(bytes: number, seconds: number): string {
  return `${(bytes / (1024 * 1024) / seconds).toFixed(1)} MB/s`;
}

async function readBenchmarkResults(fileKey: FileKey) {
  const content = await readFile(join(process.cwd(), "result", `${fileKey}.json`), "utf-8");
  return JSON.parse(content) as { results: BenchmarkResult[] };
}

function getParserEntries(data: { results: BenchmarkResult[] }, semantic: boolean): ParserEntry[] {
  const resultsByParser = new Map<string, BenchmarkResult>();
  for (const result of data.results) {
    if (PARSERS[result.parser as ParserKey]) {
      resultsByParser.set(result.parser, result);
    }
  }

  const entries: ParserEntry[] = [];
  for (const [key, parser] of Object.entries(PARSERS)) {
    if (parser.semantic !== semantic) continue;
    entries.push({ key, name: parser.name, result: resultsByParser.get(key) ?? null });
  }

  entries.sort((a, b) => {
    if (a.result && b.result) return a.result.median - b.result.median;
    if (a.result && !b.result) return -1;
    if (!a.result && b.result) return 1;
    return 0;
  });

  return entries;
}

async function generateChart(
  entries: ParserEntry[],
  chartName: string,
  fileSize: number,
): Promise<string> {
  const data = entries.filter((e) => e.result != null);
  if (data.length === 0) return "";

  const labels = data.map((e) => e.name);
  const medianData = data.map((e) => e.result!.median * 1000);
  const colors = data.map((e) => CHART_COLORS[e.key] ?? "#888888");

  const maxTime = Math.max(...medianData);
  const rawStep = maxTime / 4;
  const magnitude = 10 ** Math.floor(Math.log10(rawStep));
  const step = [1, 2, 2.5, 5, 10].map((s) => s * magnitude).find((s) => s >= rawStep)!;
  const chartMax = Math.ceil(maxTime / step) * step;

  const dpr = 3;
  const chartWidth = 500;
  const chartHeight = data.length * 24 + 28;

  const chartJSNodeCanvas = new ChartJSNodeCanvas({
    width: chartWidth * dpr,
    height: chartHeight * dpr,
  });

  const configuration: ChartConfiguration = {
    type: "bar",
    data: {
      labels,
      datasets: [
        {
          data: medianData,
          backgroundColor: colors,
          borderWidth: 0,
          borderRadius: 0,
          barPercentage: 0.75,
          categoryPercentage: 0.92,
        },
      ],
    },
    options: {
      indexAxis: "y",
      responsive: false,
      devicePixelRatio: 1,
      layout: {
        padding: { right: 65 * dpr, top: 2 * dpr, bottom: 0 },
      },
      plugins: {
        legend: { display: false },
        title: { display: false },
      },
      scales: {
        x: { display: false, beginAtZero: true, max: chartMax },
        y: {
          grid: { display: false },
          border: { display: false },
          ticks: {
            color: "#CAC1B0",
            font: { size: 9 * dpr },
            padding: 3 * dpr,
          },
        },
      },
    },
    plugins: [
      {
        id: "value-labels",
        afterDatasetsDraw(chart) {
          const ctx = chart.ctx;
          const meta = chart.getDatasetMeta(0);
          const dataset = chart.data.datasets[0];
          for (let i = 0; i < meta.data.length; i++) {
            const bar = meta.data[i];
            const value = dataset.data[i] as number;
            ctx.save();
            ctx.font = `${9 * dpr}px sans-serif`;
            ctx.textBaseline = "middle";
            ctx.fillStyle = "#CAC1B0";
            ctx.textAlign = "left";
            const msLabel = `${value.toFixed(2)}ms`;
            ctx.fillText(msLabel, bar.x + 8 * dpr, bar.y);
            const throughput = formatThroughput(fileSize, value / 1000);
            const barWidth = bar.x - (bar as unknown as { base: number }).base;
            ctx.font = `${8 * dpr}px sans-serif`;
            const inset = Math.min(8 * dpr, (barWidth - ctx.measureText(throughput).width) / 2);
            if (inset >= 2 * dpr) {
              ctx.fillStyle = "rgba(255, 255, 255, 0.75)";
              ctx.textAlign = "right";
              ctx.fillText(throughput, bar.x - inset, bar.y);
            } else {
              ctx.font = `${9 * dpr}px sans-serif`;
              const msWidth = ctx.measureText(msLabel).width;
              ctx.fillText(`· ${throughput}`, bar.x + 8 * dpr + msWidth + 4 * dpr, bar.y);
            }
            ctx.restore();
          }
        },
      },
    ],
  };

  const imageBuffer = await chartJSNodeCanvas.renderToBuffer(configuration);
  const chartPath = join(process.cwd(), "charts", `${chartName}.png`);
  await mkdir(join(process.cwd(), "charts"), { recursive: true });
  await writeFile(chartPath, imageBuffer);

  return `charts/${chartName}.png`;
}

function generateTable(entries: ParserEntry[]): string {
  const lines = [
    "| Parser | Median | Min | p99 | Relative |",
    "|--------|--------|-----|-----|----------|",
  ];

  const fastest = entries.reduce<number | null>(
    (min, e) => (e.result && (min === null || e.result.median < min) ? e.result.median : min),
    null,
  );

  for (const { name, result } of entries) {
    if (!result) {
      lines.push(`| ${name} | Failed to parse | - | - | - |`);
      continue;
    }
    const relative = fastest ? `${(result.median / fastest).toFixed(2)}×` : "-";
    lines.push(
      `| ${name} | ${formatTime(result.median)} | ${formatTime(result.min)} | ${formatTime(result.p99)} | ${relative} |`,
    );
  }

  return lines.join("\n");
}

async function generateBenchmarksSection(): Promise<string> {
  const lines = ["## Benchmarks", ""];

  for (const [key, file] of Object.entries(FILES)) {
    const fileKey = key as FileKey;
    const fileName = file.path.split("/").pop()!;
    const fileSize = metadata.files.find((entry) => entry.path === file.path)!.bytes;
    const data = await readBenchmarkResults(fileKey);
    const entries = getParserEntries(data, false);

    lines.push(`### [${fileName}](${file.source_url})`);
    lines.push("");
    lines.push(`**File size:** ${formatBytes(fileSize)}`);
    lines.push("");

    const chartPath = await generateChart(entries, fileKey, fileSize);
    if (chartPath) {
      lines.push(`![Bar chart comparing native parser speeds for ${fileName}](${chartPath})`);
      lines.push("");
    }

    lines.push(generateTable(entries));
    lines.push("");
  }

  return lines.join("\n");
}

async function generateSemanticSection(): Promise<string> {
  const lines: string[] = [];

  lines.push(`## Semantic`);
  lines.push("");
  lines.push(
    `The ECMAScript specification defines a set of early errors that conformant implementations must report before execution. Some of these are detectable during parsing from local context alone, like \`return\` outside a function, \`yield\` outside a generator, invalid destructuring, etc. Others require knowledge of the program's scope structure and bindings, such as redeclarations, unresolved exports, private fields used outside their class, etc.`,
  );
  lines.push("");
  lines.push(
    `Parsers handle this differently: SWC checks some scope-dependent errors during parsing itself, while Yuku, Oxc, and SWC Next provide a separate semantic analysis pass. This lets each consumer opt in to scope resolution when needed.`,
  );
  lines.push("");
  lines.push(
    `The benchmarks below measure parsing followed by this additional pass, which builds scopes and symbols, resolves identifier references, and checks scope-dependent early errors. Oxc enables \`with_check_syntax_error(true)\`; SWC Next enables \`AnalyzeOptions::check_syntax\`. These timings do not establish equivalent conformance coverage.`,
  );
  lines.push("");

  for (const [key, file] of Object.entries(FILES)) {
    const fileKey = key as FileKey;
    const fileName = file.path.split("/").pop()!;
    const fileSize = metadata.files.find((entry) => entry.path === file.path)!.bytes;
    const data = await readBenchmarkResults(fileKey);
    const entries = getParserEntries(data, true);
    if (entries.every((e) => e.result == null)) continue;

    lines.push(`### [${fileName}](${file.source_url})`);
    lines.push("");

    const chartPath = await generateChart(entries, `${fileKey}_semantic`, fileSize);
    if (chartPath) {
      lines.push(
        `![Bar chart comparing parser speeds with semantic analysis for ${fileName}](${chartPath})`,
      );
      lines.push("");
    }

    lines.push(generateTable(entries));
    lines.push("");
  }

  return lines.join("\n");
}

function generateParsersSection(): string {
  const lines = ["## Parsers", ""];

  for (const [, parser] of Object.entries(PARSERS)) {
    if (parser.semantic) continue;
    lines.push(`### [${parser.name}](${parser.url})`);
    lines.push("");
    lines.push(`**Language:** ${parser.language}`);
    lines.push("");
    lines.push(parser.description);
    lines.push("");
  }

  return lines.join("\n");
}

function getSystemInfo(metadata: BenchmarkMetadata): string {
  const { platform: os, arch: osArch, release: osRelease, cpu: cpuModel, cores: cpuCores, memoryGB: totalMemoryGB } = metadata.system;
  const osName =
    os === "darwin" ? "macOS (Darwin kernel)" : os === "win32" ? "Windows" : os === "linux" ? "Linux" : os;

  return `## System

| Property | Value |
|----------|-------|
| OS | ${osName} ${osRelease} (${osArch}) |
| CPU | ${cpuModel} |
| Cores | ${cpuCores} |
| Memory | ${totalMemoryGB} GB |
| Run started (UTC) | ${metadata.startedAt} |
| Rust | ${metadata.toolchains.rustc} |
| Zig | ${metadata.toolchains.zig} |
| Bun | ${metadata.toolchains.bun} |

Oxc: \`${metadata.parsers.oxc_parser}\`; SWC: \`${metadata.parsers.swc_ecma_parser}\`; SWC Next: \`${metadata.parsers.swc_next_ecma_parser}\` from [crates.io](https://crates.io/crates/swc_next_ecma_parser/${metadata.parsers.swc_next_ecma_parser}).

Yuku source: [pinned revision](${metadata.yuku.replace("git+", "").replace("/?ref=HEAD#", "/commit/")}). Fixture source: [\`${metadata.fixturesCommit}\`](https://github.com/yuku-toolchain/parser-benchmark-files/commit/${metadata.fixturesCommit}). Toolchain versions, input sizes and SHA-256 hashes, and binary hashes are saved in [result/metadata.json](result/metadata.json).`;
}

function generateRunSection(): string {
  return `## Run Benchmarks

### Prerequisites

- [Bun](https://bun.sh/) - JavaScript runtime and package manager
- [Rust](https://www.rust-lang.org/tools/install) - For building Rust-based parsers
- [Zig](https://ziglang.org/download/) - For building Zig-based parsers (tested version recorded above)

### Steps

1. Clone the repository:

\`\`\`bash
git clone https://github.com/yuku-toolchain/ecmascript-parser-benchmark-native.git
cd ecmascript-parser-benchmark-native
\`\`\`

SWC Next dependencies are pinned to the published crates.io release in \`rust-next/Cargo.toml\` and \`rust-next/Cargo.lock\`. Cargo downloads those registry packages; no sibling SWC Next repository or path dependency is used. Package versions, registry sources, and checksums are recorded in \`result/metadata.json\`.

2. Install dependencies:

\`\`\`bash
bun install --frozen-lockfile
\`\`\`

3. Download the benchmark files:

\`\`\`bash
bun load-files
\`\`\`

The downloader follows the fixture repository's HEAD. To use the exact inputs measured here:

\`\`\`bash
git -C files fetch --depth 1 origin ${metadata.fixturesCommit}
git -C files checkout --detach ${metadata.fixturesCommit}
\`\`\`

4. Build the parsers:

\`\`\`bash
bun run build
\`\`\`

5. Run benchmarks:

\`\`\`bash
bun bench
\`\`\`

This runs all suites and regenerates the README and charts. Results and run metadata are saved to \`result/\`. Use \`bun readme\` to regenerate the report from saved results without rerunning benchmarks.`;
}

function generateMethodologySection(): string {
  return `## Methodology

Parsing is timed in-process to isolate it from process startup, dynamic linking, file I/O, and memory teardown, which would otherwise dominate the measurement on smaller files.

The source is read once, then each parser runs 50 warmup iterations followed by 300 timed iterations. A monotonic clock wraps parser construction and parsing (plus the semantic pass for the semantic variants). Arena construction and teardown happen outside the timed region; allocations performed during parsing remain timed. The result passes through an optimization barrier so the work cannot be elided. Reported figures are the median, minimum, and 99th percentile of the timed runs.

SWC Next uses \`NoTokenParserConfig\`, \`Lang::from_path\` (including declaration-file mode for \`.d.ts\`), module mode, and the default comment and parenthesis handling. Its parser and semantic diagnostics are checked outside the timed region. Rust arena-based parsers create a fresh arena per iteration; Yuku retains arena capacity between iterations. Each suite keeps its parser's existing AST representation and defaults, so these are end-to-end parser API timings rather than identical AST workloads. All suites are rerun locally in sequence; historical timings are not mixed into the tables.

SWC Next is built as a separate native binary in \`rust-next/\` because its allocator dependency conflicts with the version pinned by Oxc 0.102. The Rust binaries share the same measurement helper, release profile, and global allocator, and each has a committed Cargo lockfile.

Binaries are built with release optimizations: Rust with \`cargo build --release\` (LTO, single codegen unit, symbol stripping) and Zig with \`zig build --release=fast\`. Each uses a fast general-purpose allocator (Rust \`mimalloc\`, Zig \`smp_allocator\`).`;
}

async function main() {
  const readme = [
    "# Native ECMAScript Parser Benchmark",
    "",
    "Benchmarks for ECMAScript parsers compiled to native binaries (Zig, Rust), measuring raw parsing speed without any JavaScript runtime overhead.",
    "",
    getSystemInfo(metadata),
    "",
    generateParsersSection(),
    await generateBenchmarksSection(),
    await generateSemanticSection(),
    generateRunSection(),
    "",
    generateMethodologySection(),
  ].join("\n");

  await writeFile(join(process.cwd(), "README.md"), readme);
  console.log("README.md generated successfully!");
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
