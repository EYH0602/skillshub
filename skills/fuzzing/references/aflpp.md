# AFL++ Reference

AFL++ is a community fork of AFL with enhanced features for coverage-guided fuzzing.

## Instrumentation Modes

### Source Instrumentation (Preferred)
```bash
# LLVM mode (recommended)
afl-clang-fast -o target target.c

# With comparison logging (better for magic bytes)
AFL_LLVM_CMPLOG=1 afl-clang-fast -o target_cmplog target.c

# GCC plugin mode
afl-gcc-fast -o target target.c

# Classic GCC mode (fallback)
afl-gcc -o target target.c
```

### Binary-Only Modes
```bash
# QEMU mode
afl-fuzz -Q -i corpus -o out -- ./binary @@

# Frida mode
afl-fuzz -O -i corpus -o out -- ./binary @@

# Unicorn mode (emulation)
# Requires custom harness

# QEMU persistent mode (faster)
AFL_QEMU_PERSISTENT_ADDR=0x... afl-fuzz -Q ...
```

## Running AFL++

### Basic
```bash
afl-fuzz -i input_corpus -o output_dir -- ./target @@
# @@ = placeholder for input file
```

### From Stdin
```bash
afl-fuzz -i corpus -o out -- ./target
```

### With CmpLog (Magic Byte Discovery)
```bash
afl-fuzz -i corpus -o out -c ./target_cmplog -- ./target @@
```

### Parallel Fuzzing
```bash
# Primary instance
afl-fuzz -M primary -i corpus -o out -- ./target @@

# Secondary instances
afl-fuzz -S secondary1 -i corpus -o out -- ./target @@
afl-fuzz -S secondary2 -i corpus -o out -- ./target @@
```

### With Dictionary
```bash
afl-fuzz -i corpus -o out -x dict.txt -- ./target @@
```

## Key Options

### Performance
| Option | Description |
|--------|-------------|
| `-p schedule` | Power schedule: fast, explore, coe, lin, quad, exploit |
| `-L N` | Enable MOpt mutator with limit N |
| `-l N` | CmpLog level (2=transform, 3=extended) |

### Input/Output
| Option | Description |
|--------|-------------|
| `-i dir` | Input corpus |
| `-o dir` | Output directory |
| `-x dict` | Dictionary file |
| `-t ms` | Timeout per execution |
| `-m mb` | Memory limit |

### Modes
| Option | Description |
|--------|-------------|
| `-Q` | QEMU mode (binary) |
| `-O` | Frida mode (binary) |
| `-c prog` | CmpLog binary |
| `-M name` | Primary fuzzer |
| `-S name` | Secondary fuzzer |

## Environment Variables

### Instrumentation
```bash
AFL_LLVM_CMPLOG=1          # Enable CmpLog
AFL_LLVM_LAF_ALL=1         # Enable LAF-Intel (split comparisons)
AFL_HARDEN=1               # Security hardening
AFL_USE_ASAN=1             # Build with ASan
AFL_USE_MSAN=1             # Build with MSan
AFL_USE_UBSAN=1            # Build with UBSan
```

### Runtime
```bash
AFL_SKIP_CPUFREQ=1         # Skip CPU frequency check
AFL_NO_UI=1                # Disable UI (for scripts)
AFL_AUTORESUME=1           # Auto-resume from output dir
AFL_FAST_CAL=1             # Faster calibration
AFL_CMPLOG_ONLY_NEW=1      # CmpLog only on new paths
```

## Power Schedules

| Schedule | Best For |
|----------|----------|
| `fast` | Default, general purpose |
| `explore` | Maximum exploration |
| `coe` | Cut-Off Exponential (depth) |
| `lin` | Linear |
| `quad` | Quadratic |
| `exploit` | Exploiting known paths |

```bash
afl-fuzz -p explore -i corpus -o out -- ./target @@
```

## Dictionary Format

```
# Keywords
kw1="GET"
kw2="POST"
kw3="HTTP/1.1"

# Hex values
header="\x89PNG\x0d\x0a"

# Special chars
quote="\""
null="\x00"
```

## Corpus Management

### Minimization
```bash
# Minimize corpus
afl-cmin -i corpus -o min_corpus -- ./target @@

# Minimize individual inputs
afl-tmin -i crash -o crash.min -- ./target @@
```

### Show Coverage
```bash
afl-showmap -o map.txt -- ./target < input
```

### Analysis
```bash
afl-analyze -i input -- ./target @@
```

## Harness Patterns

### Persistent Mode (Much Faster)
```c
#include "afl-fuzz.h"

__AFL_FUZZ_INIT();

int main() {
    __AFL_INIT();

    unsigned char *buf = __AFL_FUZZ_TESTCASE_BUF;

    while (__AFL_LOOP(10000)) {
        int len = __AFL_FUZZ_TESTCASE_LEN;
        ProcessInput(buf, len);
    }
    return 0;
}
```

### Deferred Initialization
```c
#include "afl-fuzz.h"

int main() {
    // Slow initialization here
    InitializeLibrary();
    LoadModels();

    __AFL_INIT();  // Fork happens here

    // Fast path per input
    char buf[1024];
    int len = read(0, buf, sizeof(buf));
    Process(buf, len);
    return 0;
}
```

## Output Directory Structure

```
output/
├── crashes/         # Crash-inducing inputs
│   ├── id:000000,...
│   └── README.txt
├── hangs/           # Timeout-inducing inputs
├── queue/           # Corpus of interesting inputs
├── fuzzer_stats     # Statistics
├── plot_data        # For afl-plot
└── .cur_input       # Current test case
```

## Debugging

### Why No New Paths?
1. Check `fuzzer_stats` for coverage stability
2. Verify instrumentation: `afl-showmap -o /dev/null -- ./target < input`
3. Try different power schedule: `-p explore`
4. Add dictionary for structured inputs
5. Use CmpLog for magic byte issues

### Performance Tuning
1. Use persistent mode (10-100x faster)
2. Enable deferred init for slow setup
3. Disable unnecessary features in target
4. Use ramdisk for corpus: `-i /dev/shm/corpus`
5. Parallel instances with `-M`/`-S`

### Crash Triage
```bash
# Minimize crash
afl-tmin -i crash -o crash.min -- ./target @@

# Analyze with ASan
AFL_USE_ASAN=1 afl-clang-fast -o target_asan target.c
./target_asan < crash.min
```

## Integration Checklist

1. [ ] Instrument target with `afl-clang-fast`
2. [ ] Build CmpLog version for magic bytes
3. [ ] Create seed corpus (valid inputs)
4. [ ] Create dictionary (if structured)
5. [ ] Test: `afl-fuzz -i corpus -o out -- ./target @@`
6. [ ] Verify coverage: `afl-showmap`
7. [ ] Scale: add secondary fuzzers
