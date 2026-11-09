# Native ECMAScript Parser Benchmark

Benchmarks for ECMAScript parsers compiled to native binaries (Zig, Rust), measuring raw parsing speed without any JavaScript runtime overhead.

## System

| Property | Value |
|----------|-------|
| OS | macOS 25.6.0 (arm64) |
| CPU | Apple M5 Max |
| Cores | 18 |
| Memory | 64 GB |

## Parsers

### [Yuku](https://github.com/yuku-toolchain/yuku)

**Language:** Zig

A high-performance & spec-compliant JavaScript/TypeScript compiler toolchain written in Zig.

### [Oxc](https://github.com/oxc-project/oxc)

**Language:** Rust

A high-performance JavaScript and TypeScript parser written in Rust.

### [SWC](https://github.com/swc-project/swc)

**Language:** Rust

An extensible Rust-based platform for compiling and bundling JavaScript and TypeScript.

### [SWC Next](https://github.com/swc-project/swc-next)

**Language:** Rust

The next-generation SWC JavaScript and TypeScript parser.

## Benchmarks

### [typescript.js](https://raw.githubusercontent.com/yuku-toolchain/parser-benchmark-files/refs/heads/main/typescript.js)

**File size:** 7.83 MB

![Bar chart comparing native parser speeds for typescript.js](charts/typescript.svg)

| Parser | Median | Min | p99 | Relative |
|--------|--------|-----|-----|----------|
| SWC Next | 12.11 ms | 11.77 ms | 13.51 ms | 1.00× |
| Yuku | 13.50 ms | 13.09 ms | 14.42 ms | 1.11× |
| Oxc | 16.99 ms | 15.99 ms | 22.56 ms | 1.40× |
| SWC | 31.51 ms | 30.02 ms | 43.58 ms | 2.60× |

### [checker.ts](https://raw.githubusercontent.com/yuku-toolchain/parser-benchmark-files/refs/heads/main/checker.ts)

**File size:** 2.95 MB

![Bar chart comparing native parser speeds for checker.ts](charts/checker.svg)

| Parser | Median | Min | p99 | Relative |
|--------|--------|-----|-----|----------|
| SWC Next | 4.19 ms | 4.08 ms | 6.16 ms | 1.00× |
| Yuku | 4.60 ms | 4.47 ms | 6.54 ms | 1.10× |
| Oxc | 5.48 ms | 5.19 ms | 7.69 ms | 1.31× |
| SWC | 10.29 ms | 9.57 ms | 13.98 ms | 2.46× |

### [lib.dom.d.ts](https://raw.githubusercontent.com/yuku-toolchain/parser-benchmark-files/refs/heads/main/lib.dom.d.ts)

**File size:** 2.24 MB

![Bar chart comparing native parser speeds for lib.dom.d.ts](charts/lib_dom.svg)

| Parser | Median | Min | p99 | Relative |
|--------|--------|-----|-----|----------|
| SWC Next | 1.28 ms | 1.21 ms | 1.73 ms | 1.00× |
| Yuku | 1.58 ms | 1.53 ms | 1.72 ms | 1.24× |
| Oxc | 1.65 ms | 1.58 ms | 2.25 ms | 1.29× |
| SWC | 3.90 ms | 3.59 ms | 6.12 ms | 3.06× |

### [react.js](https://raw.githubusercontent.com/yuku-toolchain/parser-benchmark-files/refs/heads/main/react.js)

**File size:** 0.07 MB

![Bar chart comparing native parser speeds for react.js](charts/react.svg)

| Parser | Median | Min | p99 | Relative |
|--------|--------|-----|-----|----------|
| SWC Next | 0.06 ms | 0.06 ms | 0.07 ms | 1.00× |
| Yuku | 0.07 ms | 0.07 ms | 0.11 ms | 1.31× |
| Oxc | 0.09 ms | 0.08 ms | 0.14 ms | 1.50× |
| SWC | 0.19 ms | 0.18 ms | 0.27 ms | 3.40× |

## Semantic

The ECMAScript specification defines a set of early errors that conformant implementations must report before execution. Some of these are detectable during parsing from local context alone, like `return` outside a function, `yield` outside a generator, invalid destructuring, etc. Others require knowledge of the program's scope structure and bindings, such as redeclarations, unresolved exports, private fields used outside their class, etc.

Parsers handle this differently: SWC checks some scope-dependent errors during parsing itself, while Yuku, Oxc, and SWC Next provide a separate semantic analysis pass. This keeps parsing fast and lets each consumer opt in only to the work it actually needs. A formatter, for example, only needs the AST and should not pay the cost of scope resolution.

The benchmarks below measure parsing followed by this additional pass, which builds a scope tree and symbol table, resolves identifier references to their declarations, and reports the remaining early errors. SWC Next enables `SyntaxChecks::all()`; Yuku enables `check = true`. These timings do not establish equivalent conformance coverage.

### [typescript.js](https://raw.githubusercontent.com/yuku-toolchain/parser-benchmark-files/refs/heads/main/typescript.js)

![Bar chart comparing parser speeds with semantic analysis for typescript.js](charts/typescript_semantic.svg)

| Parser | Median | Min | p99 | Relative |
|--------|--------|-----|-----|----------|
| SWC Next + Semantic | 25.56 ms | 25.07 ms | 29.78 ms | 1.00× |
| Yuku + Semantic | 28.05 ms | 27.46 ms | 33.57 ms | 1.10× |
| Oxc + Semantic | 33.04 ms | 31.92 ms | 40.39 ms | 1.29× |

### [checker.ts](https://raw.githubusercontent.com/yuku-toolchain/parser-benchmark-files/refs/heads/main/checker.ts)

![Bar chart comparing parser speeds with semantic analysis for checker.ts](charts/checker_semantic.svg)

| Parser | Median | Min | p99 | Relative |
|--------|--------|-----|-----|----------|
| SWC Next + Semantic | 9.86 ms | 9.42 ms | 13.09 ms | 1.00× |
| Yuku + Semantic | 10.28 ms | 10.06 ms | 12.09 ms | 1.04× |
| Oxc + Semantic | 11.90 ms | 11.14 ms | 16.40 ms | 1.21× |

### [lib.dom.d.ts](https://raw.githubusercontent.com/yuku-toolchain/parser-benchmark-files/refs/heads/main/lib.dom.d.ts)

![Bar chart comparing parser speeds with semantic analysis for lib.dom.d.ts](charts/lib_dom_semantic.svg)

| Parser | Median | Min | p99 | Relative |
|--------|--------|-----|-----|----------|
| SWC Next + Semantic | 2.53 ms | 2.38 ms | 3.41 ms | 1.00× |
| Yuku + Semantic | 2.63 ms | 2.56 ms | 2.82 ms | 1.04× |
| Oxc + Semantic | 2.93 ms | 2.77 ms | 4.03 ms | 1.16× |

### [react.js](https://raw.githubusercontent.com/yuku-toolchain/parser-benchmark-files/refs/heads/main/react.js)

![Bar chart comparing parser speeds with semantic analysis for react.js](charts/react_semantic.svg)

| Parser | Median | Min | p99 | Relative |
|--------|--------|-----|-----|----------|
| SWC Next + Semantic | 0.12 ms | 0.12 ms | 0.23 ms | 1.00× |
| Yuku + Semantic | 0.15 ms | 0.15 ms | 0.24 ms | 1.26× |
| Oxc + Semantic | 0.17 ms | 0.17 ms | 0.30 ms | 1.45× |

## Run Benchmarks

### Prerequisites

- [Bun](https://bun.sh/) - JavaScript runtime and package manager
- [Rust](https://www.rust-lang.org/tools/install) - For building Rust-based parsers
- [Zig](https://ziglang.org/download/) 0.17.0 - Required by Yuku 0.27.0

### Steps

1. Clone the repository:

```bash
git clone https://github.com/yuku-toolchain/ecmascript-parser-benchmark-native.git
cd ecmascript-parser-benchmark-native
```

2. Install dependencies:

```bash
bun install
```

3. Download the benchmark files:

```bash
bun load-files
```

4. Build the parsers:

```bash
bun run build
```

5. Run benchmarks:

```bash
bun bench
```

This will run benchmarks on all test files. Results are saved to the `result/` directory.

## Methodology

Parsing is timed in-process to isolate it from process startup, dynamic linking, file I/O, and memory teardown, which would otherwise dominate the measurement on smaller files.

The source is read once, then each parser runs 50 warmup iterations followed by 300 timed iterations. A monotonic clock wraps only the parse call (plus the semantic pass for the semantic variants); allocation and teardown happen outside the timed region, and the result passes through an optimization barrier so the work cannot be elided. Reported figures are the median, minimum, and 99th percentile of the timed runs.

Binaries are built with release optimizations: Rust with `cargo build --release` (LTO, single codegen unit, symbol stripping) and Zig with `zig build --release=fast`. Each uses a fast general-purpose allocator (Rust `mimalloc`, Zig `smp_allocator`).