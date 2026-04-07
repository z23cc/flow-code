use criterion::{criterion_group, criterion_main, Criterion};
use flowctl_core::events::{EpicEvent, EventMetadata, FlowEvent};
use flowctl_db::pool::open_memory_async;
use flowctl_db::repo::EventStoreRepo;

fn test_metadata() -> EventMetadata {
    EventMetadata {
        actor: "bench".into(),
        source_cmd: "bench".into(),
        session_id: "bench-sess".into(),
        timestamp: None,
    }
}

fn bench_append(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    c.bench_function("event_store_append", |b| {
        b.iter(|| {
            rt.block_on(async {
                let (_db, conn) = open_memory_async().await.unwrap();
                let repo = EventStoreRepo::new(conn);
                for _ in 0..10 {
                    repo.append(
                        "epic:bench-1",
                        &FlowEvent::Epic(EpicEvent::Created),
                        &test_metadata(),
                    )
                    .await
                    .unwrap();
                }
            });
        });
    });
}

fn bench_query_stream(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    // Pre-populate the DB outside the benchmark loop.
    let (db, conn) = rt.block_on(async { open_memory_async().await.unwrap() });
    rt.block_on(async {
        let repo = EventStoreRepo::new(conn.clone());
        for _ in 0..100 {
            repo.append(
                "epic:bench-q",
                &FlowEvent::Epic(EpicEvent::PlanWritten),
                &test_metadata(),
            )
            .await
            .unwrap();
        }
    });

    c.bench_function("event_store_query_stream_100", |b| {
        b.iter(|| {
            rt.block_on(async {
                let repo = EventStoreRepo::new(conn.clone());
                let _events = repo.query_stream("epic:bench-q").await.unwrap();
            });
        });
    });

    drop(db);
}

criterion_group!(benches, bench_append, bench_query_stream);
criterion_main!(benches);
