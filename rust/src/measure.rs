const WARMUP: usize = 50;
const RUNS: usize = 300;

pub struct Stats {
    pub median: f64,
    pub min: f64,
    pub p99: f64,
}

pub fn measure(mut sample: impl FnMut() -> u64) -> Stats {
    for _ in 0..WARMUP {
        sample();
    }
    let mut secs: Vec<f64> = (0..RUNS).map(|_| sample() as f64 / 1e9).collect();
    secs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    Stats {
        median: secs[RUNS / 2],
        min: secs[0],
        p99: secs[RUNS * 99 / 100],
    }
}
