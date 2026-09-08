use praxis_thread_store_contracts::ThreadId;
use praxis_thread_store_journal::{JournalConfig, fold_snapshot};
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 3 {
        return Err("usage: read_snapshot <thread-store-root> <thread-id>".into());
    }
    let thread_id = ThreadId::parse(&args[2])?;
    for _ in 0..3 {
        let started = Instant::now();
        let count = fold_snapshot(JournalConfig::new(&args[1]), thread_id, 0u64, |count, _| {
            *count += 1;
        })?;
        println!(
            "events={count} elapsed_us={}",
            started.elapsed().as_micros()
        );
    }
    Ok(())
}
