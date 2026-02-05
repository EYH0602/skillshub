# Fuzzer Debugging & Crash Analysis

## Diagnosing Fuzzer Issues

### Coverage Not Increasing

**Symptoms**: `paths_total` stuck, no new edges discovered

**Checklist**:
1. **Verify instrumentation**
   ```bash
   # AFL++
   afl-showmap -o /dev/null -- ./target < input
   # Should show non-zero coverage

   # LibFuzzer
   ./fuzzer -print_coverage=1 corpus/ -runs=1
   ```

2. **Check harness reaches target code**
   - Add debug prints before/after target function
   - Verify input is passed correctly
   - Check for early returns/validation

3. **Input rejected too early**
   - Seed corpus may be invalid
   - Magic bytes not discovered
   - Enable CmpLog: `afl-fuzz -c ./target_cmplog -l 2`

4. **Coverage map issues**
   - Map too small (collisions)
   - Map not linked correctly
   - Observer not tracking right map

### Crashes Not Detected

**Checklist**:
1. **Sanitizers enabled?**
   ```bash
   clang -fsanitize=address,undefined ...
   ```

2. **Signal handling**
   - Fuzzer must catch SIGSEGV, SIGABRT
   - Check executor configuration

3. **Objective feedback includes crashes?**
   ```rust
   // LibAFL
   let mut objective = CrashFeedback::new();
   ```

4. **Silent corruption**
   - May need MSan for uninitialized reads
   - Consider bounds checking

### Slow Execution

**Checklist**:
1. **I/O in hot path**
   - Move file reads outside fuzzing loop
   - Use in-memory operations

2. **Initialization repeated**
   - Use persistent mode (AFL++)
   - Use `LLVMFuzzerInitialize` (LibFuzzer)
   - Use deferred forkserver

3. **Large inputs**
   - Reduce `max_len`
   - Add input size check in harness

4. **Profile it**
   ```bash
   perf record ./fuzzer corpus/ -runs=10000
   perf report
   ```

### Memory Issues

**OOM**:
- Reduce `-rss_limit_mb`
- Check for memory leaks in target
- Run with ASan to find leaks

**Timeouts**:
- Reduce `-timeout`
- Check for infinite loops
- Profile slow inputs

## Crash Triage

### Deduplication

**By stack trace**:
```bash
# Get stack trace
./target_asan < crash 2>&1 | head -50

# Compare crash signatures
# Same top 3-5 frames = likely same bug
```

**By coverage**:
```bash
# AFL++
afl-showmap -o map1.txt -- ./target < crash1
afl-showmap -o map2.txt -- ./target < crash2
diff map1.txt map2.txt
```

**Tools**:
- ClusterFuzz (Google)
- Igor (root-cause clustering)
- AURORA (statistical crash analysis)

### Minimization

**AFL++**:
```bash
afl-tmin -i crash -o crash.min -- ./target @@
```

**LibFuzzer**:
```bash
./fuzzer -minimize_crash=1 crash
```

**LibAFL**:
```rust
// Use minimization stage
let minimizer = StdMOptMutationalStage::with_max_iterations(
    StdScheduledMutator::new(havoc_mutations()),
    0,  // max iterations (0 = auto)
);
```

### Root Cause Analysis

**Step 1: Reproduce reliably**
```bash
./target_asan < crash.min
```

**Step 2: Get sanitizer report**
```
==12345==ERROR: AddressSanitizer: heap-buffer-overflow
READ of size 4 at 0x... thread T0
    #0 0x... in vulnerable_func file.c:42
    #1 0x... in parse_input file.c:100
```

**Step 3: Analyze with debugger**
```bash
gdb -ex 'run < crash.min' ./target
# Or rr for time-travel debugging
rr record ./target < crash.min
rr replay
```

**Step 4: Identify vulnerable code path**
- What input triggers the bug?
- What constraints lead to it?
- Is it reachable from normal input?

### Severity Assessment

| Type | Severity | Indicators |
|------|----------|------------|
| Heap overflow write | Critical | ASan write OOB |
| Stack overflow write | Critical | Stack buffer overflow |
| Use-after-free | Critical | ASan heap-use-after-free |
| Heap overflow read | High | May leak sensitive data |
| Null dereference | Medium | Usually DoS only |
| Division by zero | Low | DoS |
| Memory leak | Low | Resource exhaustion |

## Reproducing Bugs

### From Fuzzer Output

```bash
# AFL++: crash inputs in output/crashes/
./target < output/crashes/id:000000,...

# LibFuzzer: crash-* files
./fuzzer crash-abc123

# LibAFL: solutions corpus
./target < solutions/id:000000
```

### Environment Matching

Important for reliable reproduction:
- Same sanitizers
- Same compiler version
- Same build flags
- Same target version

```bash
# Document build environment
clang --version
echo $CFLAGS
echo $LDFLAGS
```

### Creating Test Cases

**Minimal reproducer**:
```c
// crash_test.c
#include "target.h"
int main() {
    // Minimized crash input
    char input[] = "\x00\x01\x02...";
    target_function(input, sizeof(input));
    return 0;
}
```

**Regression test**:
```c
void test_crash_issue_123() {
    char input[] = {...};
    // Should not crash (after fix)
    assert(target_function(input, sizeof(input)) == 0);
}
```

## Common Bug Patterns

### Buffer Overflow
```c
// Bug: no bounds check
void process(char *buf, size_t len) {
    char local[100];
    memcpy(local, buf, len);  // len > 100 = overflow
}

// Fix
void process(char *buf, size_t len) {
    if (len > 100) return;
    char local[100];
    memcpy(local, buf, len);
}
```

### Integer Overflow
```c
// Bug: overflow in size calculation
void *alloc(size_t count, size_t size) {
    return malloc(count * size);  // Can overflow
}

// Fix
void *alloc(size_t count, size_t size) {
    if (count > SIZE_MAX / size) return NULL;
    return malloc(count * size);
}
```

### Format String
```c
// Bug
printf(user_input);

// Fix
printf("%s", user_input);
```

### Use After Free
```c
// Bug
free(ptr);
// ... later
use(ptr);

// Fix: nullify after free
free(ptr);
ptr = NULL;
```

## Fuzzing Campaign Management

### Monitoring Progress

**AFL++ stats**:
```
       american fuzzy lop ++
┌─ process timing ─────────────────────────────────────┐
│        run time : 0 days, 1 hrs, 23 min, 45 sec      │
│   last new path : 0 days, 0 hrs, 5 min, 12 sec       │
│   last crash/hang : 0 days, 0 hrs, 0 min, 30 sec     │
└──────────────────────────────────────────────────────┘
```

**Key metrics**:
- `paths_total`: Total unique paths
- `execs_per_sec`: Throughput
- `pending_favs`: Seeds not fully fuzzed
- `unique_crashes`: Deduplicated crashes

### When to Stop

Good stopping criteria:
1. No new paths for 24+ hours
2. Coverage plateaued
3. Pending favs exhausted
4. Time/resource budget reached

### Scaling Up

```bash
# Add more cores
afl-fuzz -S secondary_N ...

# Add different strategies
afl-fuzz -S mopt -L 0 ...      # MOpt
afl-fuzz -S explore -p explore ... # Explore

# Distributed (different machines)
# Sync via shared storage or afl-network-proxy
```
