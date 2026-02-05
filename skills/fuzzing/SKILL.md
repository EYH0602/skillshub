---
name: fuzzing
description: Design, implement, debug, and maintain fuzzers for security testing. Use when building coverage-guided fuzzers (LibAFL, AFL++, LibFuzzer), writing fuzz targets/harnesses, debugging why a fuzzer isn't finding bugs or coverage isn't increasing, analyzing and triaging crashes, implementing mutation strategies, or working with grammar-based fuzzing for structured inputs.
---

# Fuzzing

Guide for security developers designing, implementing, and debugging fuzzers.

## Decision Tree

```
What do you need?
│
├─ Design new fuzzer
│  ├─ Simple C/C++ target → LibFuzzer (fastest setup)
│  ├─ Binary-only target → AFL++ QEMU/Frida mode
│  ├─ Custom components needed → LibAFL (Rust, modular)
│  └─ Structured input (JSON/XML/protocol) → Grammar-based approach
│
├─ Write fuzz target/harness
│  ├─ LibFuzzer → See "LibFuzzer Harness" below
│  ├─ AFL++ → See "AFL++ Harness" below
│  └─ LibAFL → See references/libafl.md
│
├─ Debug fuzzer issues
│  ├─ Coverage not increasing → See "Coverage Issues"
│  ├─ Crashes not detected → See "Crash Detection Issues"
│  ├─ Slow execution → See "Performance Issues"
│  └─ Detailed debugging → See references/debugging.md
│
├─ Analyze crashes
│  ├─ Triage/dedup → See references/debugging.md
│  ├─ Minimize → afl-tmin or -minimize_crash=1
│  └─ Root cause → ASan + debugger
│
└─ Improve mutations
   ├─ Magic bytes blocking → Enable CmpLog
   ├─ Structured input → Grammar fuzzing
   └─ Custom format → Custom mutator
```

## Quick Reference

### LibFuzzer Harness
```c
#include <stdint.h>
#include <stddef.h>

extern "C" int LLVMFuzzerTestOneInput(const uint8_t *Data, size_t Size) {
    if (Size < 4) return 0;  // Reject too-small
    TargetFunction(Data, Size);
    return 0;
}
```

Compile:
```bash
clang -g -O1 -fsanitize=fuzzer,address target.c -o fuzzer
./fuzzer corpus/
```

### AFL++ Harness (Persistent Mode)
```c
#include "afl-fuzz.h"

__AFL_FUZZ_INIT();

int main() {
    __AFL_INIT();
    unsigned char *buf = __AFL_FUZZ_TESTCASE_BUF;

    while (__AFL_LOOP(10000)) {
        int len = __AFL_FUZZ_TESTCASE_LEN;
        TargetFunction(buf, len);
    }
    return 0;
}
```

Compile and run:
```bash
afl-clang-fast -o target target.c
afl-fuzz -i corpus -o out -- ./target
```

### LibAFL Minimal Setup
```rust
// Harness
let mut harness = |input: &BytesInput| {
    let buf = input.target_bytes();
    target_function(buf.as_slice());
    ExitKind::Ok
};

// Coverage map + observer
static mut MAP: [u8; 65536] = [0; 65536];
let observer = unsafe { StdMapObserver::new("cov", &mut MAP) };

// Feedback: novel coverage = interesting
let mut feedback = MaxMapFeedback::tracking(&observer, true, false);

// Objective: crashes = solutions
let mut objective = CrashFeedback::new();

// Assemble fuzzer
let mut state = StdState::new(rand, corpus, solutions, &mut feedback, &mut objective)?;
let scheduler = QueueScheduler::new();
let mut fuzzer = StdFuzzer::new(scheduler, feedback, objective);
let mutator = StdScheduledMutator::new(havoc_mutations());
let mut stages = tuple_list!(StdMutationalStage::new(mutator));

fuzzer.fuzz_loop(&mut stages, &mut executor, &mut state, &mut mgr)?;
```

## Debugging Common Issues

### Coverage Not Increasing

1. **Check instrumentation exists**
   ```bash
   # AFL++
   afl-showmap -o /dev/null -- ./target < input
   # LibFuzzer
   ./fuzzer -print_coverage=1 corpus/ -runs=1
   ```

2. **Input rejected early?**
   - Seed corpus invalid
   - Magic bytes not discovered → Enable CmpLog
   ```bash
   AFL_LLVM_CMPLOG=1 afl-clang-fast -o target_cmplog target.c
   afl-fuzz -c ./target_cmplog -l 2 -i corpus -o out -- ./target @@
   ```

3. **Harness not reaching target code**
   - Add debug logging
   - Verify input passed correctly

### Crashes Not Detected

1. **Sanitizers enabled?**
   ```bash
   clang -fsanitize=address,undefined ...
   ```

2. **Check crash feedback configured** (LibAFL)
   ```rust
   let mut objective = CrashFeedback::new();
   ```

3. **Signal handling correct?**
   - Executor must catch SIGSEGV, SIGABRT

### Performance Issues

1. **Use persistent mode** (10-100x faster)
2. **Move initialization outside loop**
   ```c
   // LibFuzzer
   extern "C" int LLVMFuzzerInitialize(int *argc, char ***argv) {
       SlowInit();
       return 0;
   }
   ```
3. **Reduce max input size**: `-max_len=1024`
4. **Profile**: `perf record ./fuzzer -runs=10000`

## Mutation Strategy Selection

| Scenario | Strategy |
|----------|----------|
| Binary blob | Havoc (default) |
| Text protocol | Dictionary + havoc |
| Checksum validation | Patch out or custom mutator |
| Magic bytes (e.g., `%PDF`) | CmpLog / LAF-Intel |
| Structured (JSON/XML) | Grammar-based |
| Complex format | Structure-aware custom mutator |

Enable dictionary:
```bash
# AFL++
afl-fuzz -x protocol.dict ...
# LibFuzzer
./fuzzer -dict=protocol.dict corpus/
```

Enable comparison logging:
```bash
# AFL++
AFL_LLVM_CMPLOG=1 afl-clang-fast -o target_cmplog target.c
afl-fuzz -c ./target_cmplog -l 2 ...
```

## Reference Documentation

| Topic | File | When to Read |
|-------|------|--------------|
| LibAFL components | [references/libafl.md](references/libafl.md) | Building custom fuzzer in Rust |
| LibFuzzer patterns | [references/libfuzzer.md](references/libfuzzer.md) | Simple C/C++ fuzzing |
| AFL++ modes | [references/aflpp.md](references/aflpp.md) | Coverage-guided or binary fuzzing |
| Mutation strategies | [references/mutations.md](references/mutations.md) | Improving mutation effectiveness |
| Grammar fuzzing | [references/grammars.md](references/grammars.md) | Structured input formats |
| Debugging & triage | [references/debugging.md](references/debugging.md) | Diagnosing issues, analyzing crashes |

## Choosing a Framework

| Requirement | Recommendation |
|-------------|----------------|
| Fastest setup | LibFuzzer |
| Binary-only target | AFL++ (QEMU/Frida) |
| Maximum customization | LibAFL |
| Parallel scaling | LibAFL (LLMP) or AFL++ (-M/-S) |
| Structure-aware | LibAFL + Gramatron, or libprotobuf-mutator |
| CI integration | LibFuzzer (OSS-Fuzz compatible) |
