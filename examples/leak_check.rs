//! Memory-leak check.
//!
//! A counting global allocator tracks **live bytes** (allocated − freed). Two
//! phases churn the allocation-heavy paths and assert live bytes return to a
//! flat baseline:
//!
//!   1. **echo** — send + recv round-trips on one long-lived association.
//!   2. **connect/close** — establish an association, exchange a message, and
//!      tear it down, over and over (the per-association fd + AsyncFd path).
//!
//! Exits non-zero on a leak. Driven by `scripts/mem_leak_test.sh`.
//!
//! Run: `cargo run --release --example leak_check`

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

use async_sctp::{ppid, SctpAssociation, SctpListener};

static LIVE: AtomicI64 = AtomicI64::new(0);

struct Counting;
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        let p = System.alloc(l);
        if !p.is_null() {
            LIVE.fetch_add(l.size() as i64, Ordering::Relaxed);
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        System.dealloc(p, l);
        LIVE.fetch_sub(l.size() as i64, Ordering::Relaxed);
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        let p = System.alloc_zeroed(l);
        if !p.is_null() {
            LIVE.fetch_add(l.size() as i64, Ordering::Relaxed);
        }
        p
    }
    unsafe fn realloc(&self, ptr: *mut u8, l: Layout, new: usize) -> *mut u8 {
        let p = System.realloc(ptr, l, new);
        if !p.is_null() {
            LIVE.fetch_add(new as i64 - l.size() as i64, Ordering::Relaxed);
        }
        p
    }
}

#[global_allocator]
static ALLOC: Counting = Counting;

fn live() -> i64 {
    LIVE.load(Ordering::Relaxed)
}

fn report(phase: &str, base: i64) -> i64 {
    let growth = live() - base;
    println!("  {phase}: live = {} bytes (Δ {:+})", live(), growth);
    growth
}

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() {
    const ECHO_ITERS: usize = 20_000;
    const ECHO_CYCLES: usize = 8;
    const CHURN_PER_CYCLE: usize = 50;
    const CHURN_CYCLES: usize = 8;
    const BUDGET: i64 = 256 * 1024;

    // A shared echo server for both phases.
    let listener = SctpListener::bind("127.0.0.1:0".parse().unwrap()).unwrap();
    let bound = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let (assoc, _) = match listener.accept().await {
                Ok(v) => v,
                Err(_) => break,
            };
            tokio::spawn(async move {
                while let Ok((d, i)) = assoc.recv().await {
                    if assoc.send(&d, i.stream, i.ppid).await.is_err() {
                        break;
                    }
                }
            });
        }
    });
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Phase 1: echo on one association.
    println!("[echo] {ECHO_CYCLES} x {ECHO_ITERS} send+recv round-trips");
    let assoc = SctpAssociation::connect(bound).await.unwrap();
    let payload = vec![0xCDu8; 128];
    for _ in 0..ECHO_ITERS {
        assoc.send(&payload, 0, ppid::M3UA).await.unwrap(); // warm up
        let _ = assoc.recv().await.unwrap();
    }
    let echo_base = live();
    for c in 1..=ECHO_CYCLES {
        for _ in 0..ECHO_ITERS {
            assoc.send(&payload, 0, ppid::M3UA).await.unwrap();
            let _ = assoc.recv().await.unwrap();
        }
        report(&format!("cycle {c:>2}/{ECHO_CYCLES}"), echo_base);
    }
    let echo_growth = live() - echo_base;
    drop(assoc);

    // Phase 2: connect / exchange / close churn.
    println!("\n[connect/close] {CHURN_CYCLES} x {CHURN_PER_CYCLE} associations");
    for _ in 0..CHURN_PER_CYCLE {
        let a = SctpAssociation::connect(bound).await.unwrap(); // warm up
        a.send(b"x", 0, ppid::M3UA).await.unwrap();
        let _ = a.recv().await.unwrap();
        a.shutdown().await.ok();
    }
    tokio::time::sleep(Duration::from_millis(200)).await;
    let churn_base = live();
    for c in 1..=CHURN_CYCLES {
        for _ in 0..CHURN_PER_CYCLE {
            let a = SctpAssociation::connect(bound).await.unwrap();
            a.send(b"x", 0, ppid::M3UA).await.unwrap();
            let _ = a.recv().await.unwrap();
            a.shutdown().await.ok();
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
        report(&format!("cycle {c:>2}/{CHURN_CYCLES}"), churn_base);
    }
    let churn_growth = live() - churn_base;

    println!();
    let mut ok = true;
    if echo_growth > BUDGET {
        eprintln!("FAIL: echo live bytes grew {echo_growth} (> {BUDGET})");
        ok = false;
    }
    if churn_growth > BUDGET {
        eprintln!("FAIL: connect/close live bytes grew {churn_growth} (> {BUDGET})");
        ok = false;
    }
    if !ok {
        std::process::exit(1);
    }
    println!("PASS: echo Δ {echo_growth} ≤ {BUDGET}; connect/close Δ {churn_growth} ≤ {BUDGET}");
}
