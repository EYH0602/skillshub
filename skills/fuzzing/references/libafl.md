# LibAFL Architecture Reference

LibAFL is a modular fuzzing framework in Rust that provides reusable components for building custom fuzzers.

## Core Components

### State
Container for all data that evolves during fuzzing:
- Corpus of test cases
- RNG state
- Execution metadata
- Serializable for pause/resume

```rust
let mut state = StdState::new(
    StdRand::with_seed(current_nanos()),
    InMemoryCorpus::new(),
    OnDiskCorpus::new(PathBuf::from("./crashes"))?,
    &mut feedback,
    &mut objective,
)?;
```

### Executor
Runs target with test cases. Types:
- `InProcessExecutor` - runs target in-process (fastest)
- `ForkserverExecutor` - fork before each run (safer)
- `CommandExecutor` - external process execution
- `QemuExecutor` - QEMU-based execution

```rust
let mut executor = InProcessExecutor::new(
    &mut harness,
    tuple_list!(observer),
    &mut fuzzer,
    &mut state,
    &mut mgr,
)?;
```

### Observer
Tracks execution properties:
- `StdMapObserver` - coverage map (edges/blocks)
- `HitcountsMapObserver` - hit counts per edge
- `TimeObserver` - execution time
- `StacktraceObserver` - call stacks

```rust
let observer = unsafe {
    StdMapObserver::new("coverage", COVERAGE_MAP.as_mut_slice())
};
```

### Feedback
Scores inputs to determine "interesting":
- `MaxMapFeedback` - novel coverage (standard)
- `TimeFeedback` - execution time novelty
- `CrashFeedback` - marks crashes as solutions
- `TimeoutFeedback` - marks timeouts

```rust
let mut feedback = feedback_or!(
    MaxMapFeedback::tracking(&observer, true, false),
    TimeFeedback::with_observer(&time_observer)
);

let mut objective = feedback_or_fast!(
    CrashFeedback::new(),
    TimeoutFeedback::new()
);
```

### Scheduler
Selects next testcase from corpus:
- `QueueScheduler` - round-robin
- `RandScheduler` - random selection
- `WeightedScheduler` - probability-weighted
- `PowerScheduler` - AFLfast-style power schedules
- `MinimizerScheduler` - favor smaller inputs

```rust
let scheduler = IndexesLenTimeMinimizerScheduler::new(
    PowerQueueScheduler::new(&mut state, &observer, PowerSchedule::FAST)
);
```

### Mutator
Transforms inputs:
- `havoc_mutations()` - AFL-style havoc (bit flips, arithmetic, etc.)
- `HavocScheduledMutator` - scheduled havoc application
- `TokenMutator` - dictionary-based
- `GramatronMutator` - grammar-aware

```rust
let mutator = StdScheduledMutator::new(havoc_mutations());
let mut stages = tuple_list!(StdMutationalStage::new(mutator));
```

### Monitor
Tracks and displays statistics:
- `SimpleMonitor` - basic stats to stdout
- `MultiMonitor` - aggregates from multiple fuzzers
- `TuiMonitor` - terminal UI

### Fuzzer
Ties everything together:
```rust
let mut fuzzer = StdFuzzer::new(scheduler, feedback, objective);
fuzzer.fuzz_loop(&mut stages, &mut executor, &mut state, &mut mgr)?;
```

## Scaling with LLMP

Low Level Message Passing enables multi-core/multi-machine fuzzing:

```rust
// Launcher handles spawning and coordination
let mut run_client = |state: Option<_>, mut mgr: LlmpRestartingEventManager<_, _, _>, _core_id| {
    // Fuzzer setup per core
};

match Launcher::builder()
    .configuration(EventConfig::AlwaysUnique)
    .monitor(monitor)
    .run_client(&mut run_client)
    .cores(&Cores::all()?)
    .build()
    .launch()
{
    Ok(()) | Err(Error::ShuttingDown) => Ok(()),
    Err(e) => Err(e),
}
```

## Common Patterns

### Basic Coverage-Guided Fuzzer
```rust
// 1. Define coverage map
static mut COVERAGE_MAP: [u8; 65536] = [0; 65536];

// 2. Create observer
let observer = unsafe { StdMapObserver::new("coverage", &mut COVERAGE_MAP) };

// 3. Create feedback (what's interesting)
let mut feedback = MaxMapFeedback::tracking(&observer, true, false);

// 4. Create objective (what's a solution)
let mut objective = CrashFeedback::new();

// 5. Create state with corpora
let mut state = StdState::new(rand, corpus, solutions, &mut feedback, &mut objective)?;

// 6. Create scheduler and fuzzer
let scheduler = QueueScheduler::new();
let mut fuzzer = StdFuzzer::new(scheduler, feedback, objective);

// 7. Create executor with harness
let mut executor = InProcessExecutor::new(&mut harness, tuple_list!(observer), ...)?;

// 8. Create mutation stages
let mutator = StdScheduledMutator::new(havoc_mutations());
let mut stages = tuple_list!(StdMutationalStage::new(mutator));

// 9. Fuzz
fuzzer.fuzz_loop(&mut stages, &mut executor, &mut state, &mut mgr)?;
```

### Custom Harness
```rust
let mut harness = |input: &BytesInput| {
    let target = input.target_bytes();
    let buf = target.as_slice();

    // Call target code
    unsafe { target_function(buf.as_ptr(), buf.len()) };

    ExitKind::Ok
};
```

### Adding Dictionary Support
```rust
let tokens = Tokens::from_file("dictionary.txt")?;
state.add_metadata(tokens);

let mutator = StdScheduledMutator::new(
    tuple_list!(
        havoc_mutations(),
        tokens_mutations()
    )
);
```

## Debugging LibAFL Fuzzers

### Coverage Not Increasing
1. Check observer is properly linked to coverage map
2. Verify harness reaches interesting code paths
3. Check feedback is using correct observer
4. Ensure coverage map size matches instrumentation

### Crashes Not Detected
1. Verify objective feedback includes CrashFeedback
2. Check sanitizers are enabled in target
3. Ensure executor handles signals properly

### Performance Issues
1. Use InProcessExecutor when possible
2. Minimize I/O in harness
3. Consider corpus minimization
4. Profile with `perf` or `samply`
