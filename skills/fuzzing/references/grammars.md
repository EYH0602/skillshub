# Grammar-Based Fuzzing Reference

## When to Use Grammar Fuzzing

Use grammar-based approaches when:
- Input has defined structure (JSON, XML, SQL, protocols)
- Random mutations mostly produce invalid inputs
- Parser rejects most fuzzed inputs early
- Need to reach deep program logic past parsing

## Grammar Specification

### BNF-Style Grammar
```
<start>    ::= <statement>
<statement>::= <select> | <insert> | <update>
<select>   ::= "SELECT" <columns> "FROM" <table>
<columns>  ::= "*" | <column_list>
<column_list> ::= <name> | <name> "," <column_list>
<table>    ::= <name>
<name>     ::= <letter> <alphanum>*
```

### Python Dictionary (Fuzzingbook Style)
```python
GRAMMAR = {
    "<start>": ["<statement>"],
    "<statement>": ["<select>", "<insert>", "<update>"],
    "<select>": ["SELECT <columns> FROM <table>"],
    "<columns>": ["*", "<column_list>"],
    "<column_list>": ["<name>", "<name>, <column_list>"],
    "<table>": ["<name>"],
    "<name>": ["<letter><alphanum>"],
    "<letter>": list("abcdefghijklmnopqrstuvwxyz"),
    "<alphanum>": ["", "<letter><alphanum>", "<digit><alphanum>"],
    "<digit>": list("0123456789"),
}
```

### ANTLR Grammar (for parser generation)
```antlr
grammar SQL;

statement: select | insert | update ;
select: 'SELECT' columns 'FROM' table ;
columns: '*' | columnList ;
columnList: name (',' name)* ;
table: name ;
name: LETTER ALPHANUM* ;

LETTER: [a-zA-Z] ;
ALPHANUM: [a-zA-Z0-9] ;
```

## Grammar-Based Tools

### Nautilus (AFL++)
Grammar-aware coverage-guided fuzzing:
```bash
# Compile grammar
python nautilus_grammar.py grammar.json

# Run with AFL++
afl-fuzz -G grammar.json -i corpus -o out -- ./target @@
```

### Gramatron
Automaton-based grammar fuzzing:
```bash
# Convert grammar to automaton
gramatron-preprocess grammar.json

# Fuzz with LibAFL
cargo run --release -- -i corpus -o out
```

### Domato (Google)
DOM fuzzer with grammar:
```python
# Define grammar rules
grammar = Grammar()
grammar.add_rule('<html>', '<body>')
grammar.add_rule('<body>', '<div> | <span>')

# Generate
html = grammar.generate('<html>')
```

### Fuzzilli (JavaScript)
IL-based JS fuzzing:
```
// Fuzzilli Intermediate Language
v0 <- LoadInt(42)
v1 <- CreateArray([v0])
v2 <- CallMethod(v1, 'push', [v0])
```

## Grammar Mutations

### Subtree Replacement
```
Original: SELECT * FROM users
Mutation: SELECT * FROM (SELECT id FROM admins)
          ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
          Replaced <table> with nested <select>
```

### Rule Expansion
```
Original: SELECT name FROM users
Mutation: SELECT name, email, id FROM users
          ^^^^^^^^^^^^^^^^^^^
          Expanded <column_list>
```

### Subtree Crossover
```
Input 1: SELECT * FROM users WHERE id=1
Input 2: INSERT INTO logs VALUES (1)
Result:  SELECT * FROM logs VALUES (1)
         Combined subtrees
```

### Minimization
```
Original: SELECT a,b,c,d,e FROM t WHERE x=1 AND y=2 AND z=3
Minimal:  SELECT a FROM t WHERE x=1
          Removed unnecessary parts while keeping coverage
```

## Integration Patterns

### With LibAFL
```rust
use libafl::mutators::gramatron::GramatronMutator;

let grammar = Grammar::from_file("grammar.json")?;
let mutator = GramatronMutator::new(&grammar);

let mut stages = tuple_list!(
    GramatronGeneratorStage::new(grammar.clone()),
    StdMutationalStage::new(mutator),
);
```

### With LibFuzzer (Structure-Aware)
```cpp
#include "grammar.h"

extern "C" int LLVMFuzzerTestOneInput(const uint8_t *Data, size_t Size) {
    // Parse grammar tree from data
    auto tree = Grammar::Parse(Data, Size);
    if (!tree) return 0;

    // Generate string and test
    std::string input = tree->Generate();
    TestParser(input.c_str());
    return 0;
}

// Custom mutator operates on grammar trees
extern "C" size_t LLVMFuzzerCustomMutator(...) {
    auto tree = Grammar::Parse(Data, Size);
    tree->Mutate();  // Grammar-aware mutation
    return tree->Serialize(Data, MaxSize);
}
```

### With AFL++ Custom Mutator
```python
# afl_grammar_mutator.py
import afl
import json

grammar = load_grammar("grammar.json")

def mutate(data):
    tree = parse_to_tree(data, grammar)
    mutated_tree = mutate_tree(tree)
    return generate_from_tree(mutated_tree)

afl.register_mutator(mutate)
```

## Common Grammars

### JSON
```python
JSON_GRAMMAR = {
    "<start>": ["<value>"],
    "<value>": ["<object>", "<array>", "<string>", "<number>", "true", "false", "null"],
    "<object>": ["{}", "{<members>}"],
    "<members>": ["<member>", "<member>,<members>"],
    "<member>": ["<string>:<value>"],
    "<array>": ["[]", "[<elements>]"],
    "<elements>": ["<value>", "<value>,<elements>"],
    "<string>": ['"<chars>"'],
    "<chars>": ["", "<char><chars>"],
    "<char>": list("abcdefghijklmnopqrstuvwxyz0123456789"),
    "<number>": ["<int>", "<int>.<digits>"],
    "<int>": ["<digit>", "<nonzero><digits>"],
    "<digits>": ["<digit>", "<digit><digits>"],
    "<digit>": list("0123456789"),
    "<nonzero>": list("123456789"),
}
```

### URL
```python
URL_GRAMMAR = {
    "<start>": ["<url>"],
    "<url>": ["<scheme>://<authority><path>?<query>"],
    "<scheme>": ["http", "https", "ftp"],
    "<authority>": ["<host>", "<host>:<port>"],
    "<host>": ["<hostname>", "<ip>"],
    "<hostname>": ["example.com", "test.org", "<word>.<word>"],
    "<port>": ["80", "443", "8080", "<digits>"],
    "<path>": ["", "/<segment><path>"],
    "<segment>": ["<word>"],
    "<query>": ["", "<param>&<query>"],
    "<param>": ["<word>=<word>"],
    "<word>": ["<letter>", "<letter><word>"],
    "<letter>": list("abcdefghijklmnopqrstuvwxyz"),
    "<digits>": ["<digit>", "<digit><digits>"],
    "<digit>": list("0123456789"),
}
```

## Tips for Effective Grammar Fuzzing

### Grammar Design
1. **Start simple**: Begin with core structure, add complexity
2. **Include edge cases**: Empty strings, max lengths, special chars
3. **Add semantic variations**: Valid but unusual combinations
4. **Balance breadth/depth**: Control recursion limits

### Combining with Coverage
1. Use coverage to guide which rules to expand
2. Prioritize rules that lead to new coverage
3. Track which grammar paths are exercised
4. Minimize inputs while preserving coverage

### Debugging Grammar Fuzzers
1. Log generated inputs to verify validity
2. Check parser acceptance rate (should be high)
3. Verify mutations preserve grammar validity
4. Test grammar against known edge cases
