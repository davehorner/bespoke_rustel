//! Hardware-free bridge probe. Set RUSTEL_MUSIC_LINK=1 and start music-hub.
//! Events are scheduled and visualized, but no audio device is opened.
use rustel_runtime::{QUERY_WORKER_STACK_BYTES, Session};
use std::time::{Duration, Instant};

fn main() -> Result<(), String> {
    std::thread::Builder::new()
        .stack_size(QUERY_WORKER_STACK_BYTES)
        .spawn(|| {
            let mut session = Session::new().map_err(|e| e.to_string())?;
            session
                .evaluate("$: note(\"c3 e3 g3 b3\").s(\"sawtooth\")\n$: s(\"bd*4\")")
                .map_err(|e| e.to_string())?;
            let start = Instant::now();
            while start.elapsed() < Duration::from_secs(30) {
                session
                    .schedule_at(start.elapsed().as_secs_f64())
                    .map_err(|e| e.to_string())?;
                std::thread::sleep(Duration::from_millis(10));
            }
            Ok(())
        })
        .map_err(|e| e.to_string())?
        .join()
        .map_err(|_| "worker panicked".to_owned())?
}
