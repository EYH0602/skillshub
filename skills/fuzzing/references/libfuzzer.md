# LibFuzzer Reference

LibFuzzer is LLVM's in-process, coverage-guided fuzzing engine.

## Fuzz Target Structure

```c
// Required entry point
extern "C" int LLVMFuzzerTestOneInput(const uint8_t *Data, size_t Size) {
    // Exercise target with Data
    DoSomethingWithInput(Data, Size);
    return 0;  // Non-zero only to reject input from corpus
}

// Optional: One-time initialization
extern "C" int LLVMFuzzerInitialize(int *argc, char ***argv) {
    // Setup code (load models, init libraries)
    return 0;
}
```

## Compilation

### Basic
```bash
clang -g -O1 -fsanitize=fuzzer target.c -o fuzzer
```

### With Sanitizers (Recommended)
```bash
# AddressSanitizer - memory errors
clang -g -O1 -fsanitize=fuzzer,address target.c -o fuzzer

# UndefinedBehaviorSanitizer
clang -g -O1 -fsanitize=fuzzer,undefined target.c -o fuzzer

# MemorySanitizer - uninitialized reads
clang -g -O1 -fsanitize=fuzzer,memory target.c -o fuzzer

# Combined
clang -g -O1 -fsanitize=fuzzer,address,undefined target.c -o fuzzer
```

### Library Linking
```bash
# When linking against instrumented library
clang -g -O1 -fsanitize=fuzzer-no-link fuzz_target.c -c
clang -g -O1 -fsanitize=fuzzer fuzz_target.o -lmylib -o fuzzer
```

## Runtime Options

### Essential
| Option | Description | Default |
|--------|-------------|---------|
| `-max_len=N` | Maximum input size | 4096 |
| `-timeout=N` | Per-input timeout (seconds) | 1200 |
| `-rss_limit_mb=N` | Memory limit | 2048 |
| `-runs=N` | Number of runs (0=infinite) | -1 |
| `-jobs=N` | Parallel jobs | 1 |
| `-workers=N` | Workers per job | min(jobs, cores/2) |

### Corpus Management
| Option | Description |
|--------|-------------|
| `-merge=1` | Merge corpora, keeping minimal set |
| `-minimize_crash=1` | Minimize crash-inducing input |
| `-artifact_prefix=path/` | Where to save crashes |
| `-reload=N` | Reload corpus every N seconds |

### Mutation Control
| Option | Description |
|--------|-------------|
| `-dict=file` | Dictionary file |
| `-only_ascii=1` | Generate only ASCII |
| `-mutate_depth=N` | Mutations per input | 5 |
| `-use_value_profile=1` | Track comparison args |
| `-use_cmp=1` | Use CMP instrumentation |

### Debugging
| Option | Description |
|--------|-------------|
| `-print_final_stats=1` | Stats at end |
| `-print_corpus_stats=1` | Corpus info |
| `-verbosity=N` | Verbosity level |
| `-help=1` | All options |

## Usage Examples

### Basic Fuzzing
```bash
# Create corpus directory
mkdir corpus

# Run fuzzer
./fuzzer corpus/

# With seed inputs
./fuzzer corpus/ seed_corpus/
```

### Parallel Fuzzing
```bash
# Multiple jobs
./fuzzer corpus/ -jobs=4 -workers=4

# Or run multiple instances
./fuzzer corpus/ &
./fuzzer corpus/ &
```

### Corpus Minimization
```bash
mkdir minimized
./fuzzer -merge=1 minimized/ corpus/
```

### Crash Minimization
```bash
./fuzzer -minimize_crash=1 crash-input
```

### Reproduce Crash
```bash
./fuzzer crash-abc123
```

## Dictionary Format

AFL-style syntax:
```
# Comments start with #
keyword1="GET"
keyword2="HTTP/1.1"
hex_value="\x00\x01\x02"
quotes="\""
```

## Harness Patterns

### Parsing Structured Data
```c
extern "C" int LLVMFuzzerTestOneInput(const uint8_t *Data, size_t Size) {
    // Reject too-small inputs early
    if (Size < 4) return 0;

    ParseStructuredData(Data, Size);
    return 0;
}
```

### With Cleanup
```c
extern "C" int LLVMFuzzerTestOneInput(const uint8_t *Data, size_t Size) {
    Context *ctx = CreateContext();
    ProcessData(ctx, Data, Size);
    DestroyContext(ctx);  // Always cleanup
    return 0;
}
```

### Rejecting Inputs
```c
extern "C" int LLVMFuzzerTestOneInput(const uint8_t *Data, size_t Size) {
    // Return -1 to reject from corpus (not "interesting")
    if (!IsValidHeader(Data, Size)) return -1;

    Process(Data, Size);
    return 0;
}
```

### Protobuf/Structure-Aware
```c
// Use libprotobuf-mutator for structured inputs
DEFINE_PROTO_FUZZER(const MyProto& proto) {
    ProcessProto(proto);
}
```

## Debugging

### Why No Coverage?
1. Check `-print_coverage=1` output
2. Verify target code is instrumented (`nm fuzzer | grep sancov`)
3. Check input reaches target code
4. Review `-verbosity=2` output

### Slow Fuzzing
1. Profile with `perf record ./fuzzer corpus/ -runs=1000`
2. Check for I/O in hot paths
3. Reduce `max_len` if inputs too large
4. Check for timeouts with `-timeout=1`

### Memory Issues
1. Run with ASan enabled
2. Check `-rss_limit_mb` setting
3. Look for leaks with `-detect_leaks=1`

## Integration with Build Systems

### CMake
```cmake
add_executable(fuzzer fuzz_target.c)
target_compile_options(fuzzer PRIVATE -fsanitize=fuzzer,address)
target_link_options(fuzzer PRIVATE -fsanitize=fuzzer,address)
```

### Bazel
```python
cc_test(
    name = "fuzzer",
    srcs = ["fuzz_target.c"],
    copts = ["-fsanitize=fuzzer,address"],
    linkopts = ["-fsanitize=fuzzer,address"],
)
```

## Limitations

LibFuzzer is not suitable when:
- Target uses assertions that abort
- Significant global state between runs
- Threads outlive individual test runs
- Single execution takes significant time

Consider AFL++/ForkserverExecutor for these cases.
