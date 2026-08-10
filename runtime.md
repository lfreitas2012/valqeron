I’ll trace `runtime.rs` and the engine’s surrounding startup/task structure, then explain what is actually implemented
today versus what the future gRPC design implies.

`runtime.rs` is the engine’s orchestration layer. It creates one Tokio runtime, runs the gRPC server and periodic jobs
on it, and bridges all blocking SQLite work to Tokio’s blocking pool.

```text
main()
  └─ runtime::run()
      ├─ acquire engine lock
      ├─ open SQLite + migrations
      ├─ create AsyncStorage
      ├─ create Tokio multi-thread runtime
      └─ block_on(run_loop())
          ├─ tonic gRPC server task
          ├─ maintenance timer
          ├─ heartbeat timer
          └─ signal handlers
```

## 1. Tokio runtime

In [`runtime.rs`](</Users/lfreitas/Development/valqeron/crates/engine/src/runtime.rs:71>), the engine builds:

```rust,ignore
tokio::runtime::Builder::new_multi_thread()
.enable_all()
.build()
```

This enables:

- multiple Tokio worker threads;
- timers;
- Unix signal handling;
- Unix sockets;
- asynchronous task scheduling.
  `runtime.block_on(run_loop(...))` executes the engine’s main async future. `block_on` itself does not create a
  separate “main loop thread”; it drives `run_loop` until it completes while the Tokio runtime schedules other async
  tasks across its workers. Tokio tasks are lightweight cooperative tasks. They run until they reach an `.await` or
  otherwise yield. The important consequence is:

> An async task must not perform blocking SQLite or `Condvar` operations directly, because that would occupy a Tokio
> worker thread.

That is why storage is isolated behind [
`AsyncStorage`](</Users/lfreitas/Development/valqeron/crates/engine/src/storage.rs:52>).

## 2. The gRPC server task

`run_loop` binds a Unix domain socket:

```rust,ignore
let listener = UnixListener::bind(config.socket_path()) ?;
```

Then it creates the tonic server:

```rust,ignore
Server::builder()
.add_service(issuer_service)
.add_service(admin_service)
.serve_with_incoming_shutdown(
UnixListenerStream::new(listener),
async move {
let _ = shutdown_rx.await;
},
)
```

The Unix listener is adapted into a stream of incoming connections using `UnixListenerStream`. Tonic then handles HTTP/2
and protobuf/gRPC dispatch.

The server is itself placed into a Tokio task:

```rust,ignore
let mut server = tokio::spawn(/* tonic server */);
```

That allows `run_loop` to monitor the server concurrently with signals and timers.

Each incoming RPC is handled asynchronously by tonic. For example, an issuer RPC performs:

```text
tonic receives request
  └─ IssuerGrpc::register()
      ├─ parse protobuf request
      ├─ await AsyncStorage::call(...)
      ├─ blocking SQLite operation runs elsewhere
      └─ return protobuf response
```

The admin health and status methods do not use SQLite; they can complete directly on a Tokio worker.

## 3. The blocking storage bridge

The critical mechanism is [
`AsyncStorage::call`](</Users/lfreitas/Development/valqeron/crates/engine/src/storage.rs:72>):

```rust,ignore
let permit = semaphore.acquire_owned().await?;

tokio::task::spawn_blocking(move | | {
let _permit = permit;
f( & engine)
})
.await
```

There are two separate pools involved:

```text
Tokio async worker pool
  ├─ gRPC protocol handling
  ├─ timers
  ├─ signal handling
  └─ futures waiting for storage

Tokio blocking pool
  └─ synchronous SQLite/domain closures
```

The RPC handler remains asynchronous while the storage closure runs on Tokio’s blocking pool. This prevents SQLite
operations and reader-pool `Condvar` waits from blocking the runtime workers.

### Why the semaphore exists

The semaphore limits storage calls to:

```rust,ignore
READER_POOL_SIZE + 1
= 4 readers + 1 writer
= 5 concurrent calls
```

The constant is defined in [`config.rs`](</Users/lfreitas/Development/valqeron/crates/engine/src/config.rs:11>).

This is deliberate. Allowing hundreds of `spawn_blocking` jobs would merely create a large queue of threads waiting for
SQLite’s own pool. Instead:

- at most five storage closures are admitted;
- calls wait asynchronously for a permit;
- after five seconds, excess calls receive an `Overloaded` error;
- they do not accumulate indefinitely.

The semaphore bounds admission, while SQLite still enforces its own actual behavior:

- reads can use the reader pool concurrently;
- writes serialize through the single writer;
- transactions and domain operations remain synchronous.

A complete mutation, such as “check uniqueness, then insert issuer”, runs inside one blocking closure. This prevents
another operation from interleaving between those steps.

## 4. Runtime scheduler and timers

The main scheduler is this loop:

```rust,ignore
loop {
tokio::select ! {
...
}
}
```

`tokio::select!` waits for whichever event becomes ready first:

- `SIGTERM`;
- `SIGINT`;
- gRPC server termination;
- maintenance timer;
- heartbeat timer.

This is not a job scheduler with a queue or worker graph. It is a single orchestration future reacting to events.

### Maintenance

The maintenance timer is created with `interval_at`. Its first execution is delayed by a jittered interval, and
subsequent executions happen periodically:

```rust,ignore
maintenance.set_missed_tick_behavior(MissedTickBehavior::Skip);
```

`Skip` is important. If maintenance takes longer than its interval, the engine does not execute a burst of catch-up
jobs. It skips missed ticks and waits for the next scheduled period.

Only one maintenance task is allowed at a time:

```rust,ignore
let busy = in_flight.as_ref().is_some_and( | h| ! h.is_finished());

if busy {
// skip this tick
} else {
in_flight = Some(tokio::spawn(async move {
storage.call("db_maintenance", run_maintenance_job).await;
}));
}
```

The Tokio task is asynchronous, but `run_maintenance_job` itself runs through `AsyncStorage`, so the actual SQLite
maintenance executes on the blocking pool.

### Heartbeat

The heartbeat is simpler:

```rust,ignore
_ = heartbeat.tick() => {
tracing::info ! (job = "heartbeat", ...);
}
```

It only logs that the engine is alive. It does not perform a health check or database operation.

## 5. Concurrency behavior

Suppose six RPCs arrive simultaneously:

```text
RPC 1 ─┐
RPC 2 ─┤
RPC 3 ─┤── acquire five storage permits ──> spawn_blocking
RPC 4 ─┤
RPC 5 ─┘

RPC 6 ── asynchronously waits for a permit
           ├─ succeeds when one finishes
           └─ returns Overloaded after 5 seconds
```

The Tokio workers are not blocked by RPC 6. Its future is suspended while waiting for the semaphore.

If five admitted operations are all reads, the infrastructure reader pool can process them according to its capacity. If
several are writes, SQLite’s single-writer design serializes them internally.

There is an important cancellation property: cancelling an RPC future does not cancel a closure that has already entered
`spawn_blocking`. The SQLite operation continues, and its semaphore permit is released only when the closure finishes.

## 6. Shutdown sequence

The shutdown process is intentionally ordered:

```text
signal received
  └─ stop accepting new connections
      └─ wait up to 10s for gRPC RPCs to drain
          └─ close AsyncStorage admission
              └─ wait for storage closures
                  └─ shut down Tokio blocking pool, max 20s
                      └─ reclaim storage engine
                          └─ final WAL checkpoint on Drop
                              └─ remove socket
                                  └─ release engine lock
```

The oneshot channel connects the orchestrator to tonic:

```rust,ignore
let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
```

Sending `shutdown_tx` causes `serve_with_incoming_shutdown` to stop accepting new connections and begin graceful
shutdown.

There are two drain boundaries:

- `DRAIN_TIMEOUT = 10s`: gRPC requests and the maintenance task;
- `RUNTIME_SHUTDOWN_TIMEOUT = 20s`: Tokio’s blocking pool.

A second interrupt signal during the gRPC drain forces a nonzero shutdown.

After gRPC draining, `storage.close()` closes the semaphore. This rejects new storage calls while allowing already
admitted closures to finish. Only after the storage is idle does the engine get reclaimed and dropped, which performs
the final SQLite/WAL cleanup.

## 7. Client-side runtime

The engine uses a multi-thread Tokio runtime because it serves concurrent gRPC traffic. The client is different: [
`client/src/lib.rs`](</Users/lfreitas/Development/valqeron/crates/client/src/lib.rs:104>) creates a small
`current_thread` runtime.

The client API is synchronous:

```rust,ignore
client.register_issuer(...)
```

Internally it does:

```rust,ignore
runtime.block_on( async {
grpc_call().await
})
```

So the client’s runtime is only an adapter for blocking callers. It is not shared with the engine, and the client should
not be called from inside another Tokio runtime.

## In short

The engine has three concurrency layers:

1. Tokio worker threads handle gRPC, timers, signals, and async coordination.
2. Tokio’s blocking pool executes synchronous domain and SQLite operations.
3. The infrastructure SQLite pool controls actual database concurrency: multiple readers and one writer.

The semaphore between layers 1 and 2 is the main backpressure mechanism. It prevents synchronous SQLite contention from
turning into an unbounded blocking-task queue, while the `select!` loop acts as the engine’s lightweight scheduler.