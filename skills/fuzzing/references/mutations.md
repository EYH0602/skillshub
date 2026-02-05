# Mutation Strategies Reference

## Core Mutation Types

### Bit-Level Mutations
| Mutation | Description | Example |
|----------|-------------|---------|
| Bit flip | Flip 1/2/4 bits | `0x41` → `0x40` |
| Byte flip | Flip 1/2/4 bytes | `"AAAA"` → `"BAAA"` |
| Walking bit | Flip each bit position | Sequential single-bit changes |

### Arithmetic Mutations
| Mutation | Description | Example |
|----------|-------------|---------|
| Add/sub small | ±1 to ±35 to integers | `100` → `101` |
| Add/sub large | Larger deltas | `1000` → `1256` |
| Boundary values | Near powers of 2 | `127`, `128`, `255`, `256` |

### Interesting Values
Inject known-problematic values:
```
8-bit:  0, 1, 16, 32, 64, 127, 128, 255
16-bit: 0, 128, 255, 256, 512, 1000, 1024, 4096, 32767, 32768, 65535
32-bit: 0, 1, 32768, 65535, 65536, 100663045, 2147483647, 4294967295
```

### Block Operations
| Mutation | Description |
|----------|-------------|
| Insert | Add random bytes at position |
| Delete | Remove byte range |
| Overwrite | Replace with random/dictionary bytes |
| Clone | Duplicate existing block |
| Swap | Exchange two blocks |

### Dictionary-Based
| Mutation | Description |
|----------|-------------|
| Insert token | Add dictionary entry |
| Overwrite token | Replace bytes with token |
| Splice | Combine two corpus entries |

## AFL Mutation Stages

### Deterministic Stage
Systematic mutations applied in order:
1. **Bit flips**: 1, 2, 4 bits walking
2. **Byte flips**: 1, 2, 4 bytes walking
3. **Arithmetic**: ±1..35 on 8/16/32-bit
4. **Interesting values**: Known-problematic constants
5. **Dictionary**: Insert/overwrite tokens

### Havoc Stage
Random stacked mutations (default 1-128 per input):
- Random bit/byte flips
- Random arithmetic
- Random block operations
- Random dictionary insertions
- Random splicing

### Splice Stage
1. Select two inputs from queue
2. Find common prefix/suffix
3. Crossover at random point
4. Apply havoc to result

## Mutation Scheduling

### Uniform (Default)
Equal probability for each mutation operator.

### MOpt (Particle Swarm Optimization)
Adapts mutation probabilities based on effectiveness:
```bash
# AFL++
afl-fuzz -L 0 ...  # Enable MOpt
```

### Power Schedules (Seed Selection)
Control how much time spent on each seed:
| Schedule | Behavior |
|----------|----------|
| FAST | Favor less-fuzzed seeds |
| COE | Exponential cutoff by depth |
| EXPLORE | Maximize exploration |
| EXPLOIT | Focus on promising seeds |

## Grammar-Aware Mutations

### For Structured Inputs
When input has grammar (JSON, XML, SQL):

```
Grammar: S → A B | C
         A → "aa" | "bb"
         B → "x" | "y"
```

**Grammar mutations**:
1. Rule substitution: Replace subtree
2. Subtree crossover: Swap grammar nodes
3. Rule minimization: Simplify derivation

### Tools
- **Nautilus**: Coverage-guided grammar fuzzing
- **Gramatron**: Automaton-based grammar
- **Superion**: Grammar-aware greybox fuzzing
- **LibAFL Gramatron**: Grammar mutations in LibAFL

## Custom Mutators

### LibFuzzer
```c
extern "C" size_t LLVMFuzzerCustomMutator(
    uint8_t *Data, size_t Size, size_t MaxSize, unsigned int Seed) {
    // Custom mutation logic
    return NewSize;
}

extern "C" size_t LLVMFuzzerCustomCrossOver(
    const uint8_t *Data1, size_t Size1,
    const uint8_t *Data2, size_t Size2,
    uint8_t *Out, size_t MaxOutSize, unsigned int Seed) {
    // Custom crossover logic
    return OutSize;
}
```

### AFL++
```c
// afl-custom-mutator.h
size_t afl_custom_fuzz(void *data, uint8_t *buf, size_t buf_size,
                       uint8_t **out_buf, uint8_t *add_buf,
                       size_t add_buf_size, size_t max_size);
```

### LibAFL
```rust
impl<I, S> Mutator<I, S> for MyMutator
where
    I: HasBytesVec,
{
    fn mutate(&mut self, state: &mut S, input: &mut I, stage_idx: i32) -> Result<MutationResult, Error> {
        let bytes = input.bytes_mut();
        // Mutation logic
        Ok(MutationResult::Mutated)
    }
}
```

## Effectiveness Guidelines

### When to Use What

| Input Type | Recommended Mutations |
|------------|----------------------|
| Binary blobs | Havoc, bit flips |
| Text protocols | Dictionary, havoc |
| Structured (JSON/XML) | Grammar-aware |
| File formats | Dictionary + magic bytes |
| APIs with types | Structure-aware mutators |

### Improving Mutation Effectiveness

1. **Add dictionary** for known keywords
2. **Enable CmpLog** for magic byte discovery
3. **Use LAF-Intel** to split comparisons
4. **Grammar mutations** for structured input
5. **Structure-aware** for complex formats

### Common Issues

**Not finding magic bytes**:
```bash
# AFL++ with CmpLog
afl-fuzz -c ./target_cmplog -l 2 ...
```

**Stuck on checksum**:
- Patch out checksum validation
- Use custom mutator to fix checksums
- Enable `FUZZING_BUILD_MODE_UNSAFE_FOR_PRODUCTION`

**Valid structure required**:
- Use grammar-based generation
- Start with valid seed corpus
- Implement structure-aware mutator
