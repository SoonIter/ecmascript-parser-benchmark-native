use std::hint::black_box;
use std::time::Instant;

use swc_next_allocator::Allocator as SwcNextAllocator;
use swc_next_ecma_ast::Lang;
use swc_next_ecma_parser::{NoTokenParserConfig, Options, Parser as SwcNextParser};
use swc_next_ecma_semantic::{AnalyzeOptions, analyze};

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[path = "../../rust/src/measure.rs"]
mod measure;
use measure::{Stats, measure};

fn bench_swc_next(src: &str, lang: Lang, semantic: bool) -> Stats {
    let options = Options {
        lang,
        ..Options::default()
    };
    measure(|| {
        let allocator = SwcNextAllocator::new();
        let start = Instant::now();
        let ret =
            SwcNextParser::init(&allocator, black_box(src), options, NoTokenParserConfig).parse();
        let sem = semantic.then(|| {
            analyze(
                &ret.ast,
                AnalyzeOptions {
                    check_syntax: true,
                    ..Default::default()
                },
            )
        });
        let dt = start.elapsed().as_nanos() as u64;
        black_box(&ret);
        black_box(&sem);
        assert!(
            ret.diagnostics.is_empty(),
            "SWC Next parse diagnostics: {:?}",
            ret.diagnostics
        );
        if let Some(sem) = &sem {
            assert!(
                sem.diagnostics.is_empty(),
                "SWC Next semantic diagnostics: {:?}",
                sem.diagnostics
            );
        }
        dt
    })
}

fn main() {
    let mut out = String::from("{\"results\":[");
    let mut first = true;
    for path in std::env::args().skip(1) {
        let source = std::fs::read_to_string(&path).expect("cannot read benchmark source");
        for (parser, semantic) in [("swc_next", false), ("swc_next_semantic", true)] {
            let s = bench_swc_next(&source, Lang::from_path(&path), semantic);
            if !first {
                out.push(',');
            }
            first = false;
            out.push_str(&format!(
                "{{\"parser\":\"{parser}\",\"file\":\"{path}\",\"median\":{},\"min\":{},\"p99\":{}}}",
                s.median, s.min, s.p99
            ));
        }
    }
    out.push_str("]}");
    println!("{out}");
}
