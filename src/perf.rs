use std::time::Instant;

pub struct Snapshot(Instant);

impl Snapshot {
    pub fn start() -> Self {
        Self(Instant::now())
    }

    pub fn emit(&self) {
        eprintln!(
            "Time taken: {:.1} ms",
            self.0.elapsed().as_secs_f64() * 1000.0
        );
    }
}
