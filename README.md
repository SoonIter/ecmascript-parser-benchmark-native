# Native ECMAScript Parser Benchmark

Benchmarks for ECMAScript parsers compiled to native binaries (Zig, Rust), measuring raw parsing speed without any JavaScript runtime overhead.

## System

| Property | Value |
|----------|-------|
| OS | macOS 25.6.0 (arm64) |
| CPU | Apple M3 |
| Cores | 8 |
| Memory | 16 GB |

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
| SWC Next | 15.75 ms | 15.19 ms | 28.00 ms | 1.00× |
| Yuku | 15.88 ms | 15.29 ms | 18.62 ms | 1.01× |
| Oxc | 25.77 ms | 25.09 ms | 26.86 ms | 1.64× |
| SWC | 38.04 ms | 36.44 ms | 119.34 ms | 2.42× |

### [checker.ts](https://raw.githubusercontent.com/yuku-toolchain/parser-benchmark-files/refs/heads/main/checker.ts)

**File size:** 2.95 MB

![Bar chart comparing native parser speeds for checker.ts](charts/checker.svg)

| Parser | Median | Min | p99 | Relative |
|--------|--------|-----|-----|----------|
| Yuku | 5.32 ms | 5.26 ms | 5.63 ms | 1.00× |
| SWC Next | 5.36 ms | 5.33 ms | 5.53 ms | 1.01× |
| Oxc | 7.94 ms | 7.90 ms | 8.96 ms | 1.49× |
| SWC | 12.59 ms | 11.91 ms | 21.46 ms | 2.36× |

### [lib.dom.d.ts](https://raw.githubusercontent.com/yuku-toolchain/parser-benchmark-files/refs/heads/main/lib.dom.d.ts)

**File size:** 2.24 MB

![Bar chart comparing native parser speeds for lib.dom.d.ts](charts/lib_dom.svg)

| Parser | Median | Min | p99 | Relative |
|--------|--------|-----|-----|----------|
| SWC Next | 1.82 ms | 1.80 ms | 2.03 ms | 1.00× |
| Yuku | 2.03 ms | 2.01 ms | 2.12 ms | 1.12× |
| Oxc | 2.38 ms | 2.36 ms | 2.81 ms | 1.31× |
| SWC | 4.46 ms | 4.38 ms | 6.52 ms | 2.46× |

### [react.js](https://raw.githubusercontent.com/yuku-toolchain/parser-benchmark-files/refs/heads/main/react.js)

**File size:** 0.07 MB

![Bar chart comparing native parser speeds for react.js](charts/react.svg)

| Parser | Median | Min | p99 | Relative |
|--------|--------|-----|-----|----------|
| SWC Next | 0.08 ms | 0.08 ms | 0.09 ms | 1.00× |
| Yuku | 0.09 ms | 0.09 ms | 0.21 ms | 1.14× |
| Oxc | 0.16 ms | 0.16 ms | 0.17 ms | 1.97× |
| SWC | 0.23 ms | 0.23 ms | 0.25 ms | 2.87× |

## Semantic

The ECMAScript specification defines a set of early errors that conformant implementations must report before execution. Some of these are detectable during parsing from local context alone, like `return` outside a function, `yield` outside a generator, invalid destructuring, etc. Others require knowledge of the program's scope structure and bindings, such as redeclarations, unresolved exports, private fields used outside their class, etc.

Parsers handle this differently: SWC checks some scope-dependent errors during parsing itself, while Yuku, Oxc, and SWC Next provide a separate semantic analysis pass. This keeps parsing fast and lets each consumer opt in only to the work it actually needs. A formatter, for example, only needs the AST and should not pay the cost of scope resolution.

The benchmarks below measure parsing followed by this additional pass, which builds a scope tree and symbol table, resolves identifier references to their declarations, and reports the remaining early errors. SWC Next enables `SyntaxChecks::all()`; Yuku enables `check = true`. These timings do not establish equivalent conformance coverage.

### [typescript.js](https://raw.githubusercontent.com/yuku-toolchain/parser-benchmark-files/refs/heads/main/typescript.js)

![Bar chart comparing parser speeds with semantic analysis for typescript.js](charts/typescript_semantic.svg)

| Parser | Median | Min | p99 | Relative |
|--------|--------|-----|-----|----------|
| SWC Next + Semantic | 31.55 ms | 31.07 ms | 39.60 ms | 1.00× |
| Yuku + Semantic | 33.51 ms | 32.97 ms | 56.63 ms | 1.06× |
| Oxc + Semantic | 45.16 ms | 44.02 ms | 47.25 ms | 1.43× |

### [checker.ts](https://raw.githubusercontent.com/yuku-toolchain/parser-benchmark-files/refs/heads/main/checker.ts)

![Bar chart comparing parser speeds with semantic analysis for checker.ts](charts/checker_semantic.svg)

| Parser | Median | Min | p99 | Relative |
|--------|--------|-----|-----|----------|
| SWC Next + Semantic | 11.50 ms | 11.42 ms | 12.03 ms | 1.00× |
| Yuku + Semantic | 12.04 ms | 11.65 ms | 12.75 ms | 1.05× |
| Oxc + Semantic | 15.18 ms | 14.87 ms | 19.70 ms | 1.32× |

### [lib.dom.d.ts](https://raw.githubusercontent.com/yuku-toolchain/parser-benchmark-files/refs/heads/main/lib.dom.d.ts)

![Bar chart comparing parser speeds with semantic analysis for lib.dom.d.ts](charts/lib_dom_semantic.svg)

| Parser | Median | Min | p99 | Relative |
|--------|--------|-----|-----|----------|
| SWC Next + Semantic | 3.24 ms | 3.21 ms | 3.48 ms | 1.00× |
| Yuku + Semantic | 3.31 ms | 3.26 ms | 3.59 ms | 1.02× |
| Oxc + Semantic | 3.93 ms | 3.85 ms | 6.75 ms | 1.21× |

### [react.js](https://raw.githubusercontent.com/yuku-toolchain/parser-benchmark-files/refs/heads/main/react.js)

![Bar chart comparing parser speeds with semantic analysis for react.js](charts/react_semantic.svg)

| Parser | Median | Min | p99 | Relative |
|--------|--------|-----|-----|----------|
| SWC Next + Semantic | 0.17 ms | 0.17 ms | 0.18 ms | 1.00× |
| Yuku + Semantic | 0.20 ms | 0.19 ms | 0.73 ms | 1.19× |
| Oxc + Semantic | 0.28 ms | 0.28 ms | 0.30 ms | 1.68× |

## Run Benchmarks

### Prerequisites

- [Bun](https://bun.sh/) - JavaScript runtime and package manager
- [Rust](https://www.rust-lang.org/tools/install) - For building Rust-based parsers
- [Zig](https://ziglang.org/download/) 0.17.0 - Required by Yuku 0.23.0

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