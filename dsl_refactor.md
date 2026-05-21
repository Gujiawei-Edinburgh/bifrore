# Tenon DSL / IR / VM Refactor Plan

## Motivation

Tenon currently uses a SQL parser as the rule front end. This is convenient, but it does not match Tenon's domain cleanly:

- SQL is more expressive than Tenon needs and implies unsupported concepts such as joins, grouping, ordering, and subqueries.
- MQTT-specific concepts are added implicitly instead of being defined as first-class DSL semantics.
- SQL maps messages into a table-like model, while Tenon is fundamentally an MQTT event filtering, projection, and dispatch pipeline.

The DSL is a public traffic-ingress contract. Before v1.x, Tenon should define this contract explicitly and build a dedicated parser/compiler/runtime path.

## V1 Scope

For v1.x, keep the DSL and execution model intentionally small:

- MQTT topic filter as the event source.
- JSON payload only.
- Predicate evaluation.
- Projection.
- Destination dispatch.
- Built-in sinks and IPC custom sink.

Postpone:

- Protobuf payload support.
- Schema registry support.
- Joins, aggregation, grouping, windows, ordering.
- User-defined functions.
- Sink-specific transformation DSL.

Protobuf can be reintroduced later as an input decoder that normalizes into the same value model used by JSON.

## Target Architecture

```text
DSL text
  -> lexer/parser
  -> AST
  -> semantic analyzer
  -> logical IR
  -> optimizer
  -> bytecode IR
  -> VM/runtime
```

The parser should not directly produce executable rules. Syntax, semantic validation, optimization, and execution should be separate layers.

## Proposed DSL Shape

Example:

```text
from topic "sensor/+/data"
where payload.temp > 30 and topic[1] == "room1"
select {
  device: topic[1],
  temp: payload.temp,
  hum: payload.hum,
  score: payload.temp + 10
}
into raw_kafka, local_log
```

Raw forwarding:

```text
from topic "#"
select *
into ipc_out
```

Conceptual structure:

```text
from topic <mqtt_topic_filter>
where <predicate_expr>
select * | select { <name>: <expr>, ... }
into <destination>, ...
```

## Grammar Source of Truth

Use the LALRPOP grammar file as the authoritative grammar:

```text
engine/tenon-core/src/dsl/tenon.lalrpop
```

The grammar file is the public source of truth for review and discussion. It is LALRPOP-specific syntax rather than generic EBNF, but this avoids drift between a public grammar document and the parser generator input.

Parser generation is build-time only. Tenon should never generate a parser during daemon startup.

## Suggested Grammar Sketch

```ebnf
rule            = source where_clause? select_clause into_clause ;

source          = "from" "topic" string_literal ;
where_clause    = "where" expr ;
select_clause   = "select" "*" | "select" object_projection ;
into_clause     = "into" ident_list ;

object_projection = "{" projection_item ("," projection_item)* ","? "}" ;
projection_item   = projection_key ":" expr ;
projection_key    = ident | string_literal ;

expr            = logical_or ;
logical_or      = logical_and ( "or" logical_and )* ;
logical_and     = equality ( "and" equality )* ;
equality        = comparison ( "==" | "!=" ) comparison | comparison ;
comparison      = term ( ">" | ">=" | "<" | "<=" ) term | term ;
term            = factor (("+" | "-") factor)* ;
factor          = unary (("*" | "/" | "%") unary)* ;
unary           = ("not" | "-") unary | primary ;
primary         = literal | payload_ref | topic_ref | property_ref | metadata_ref | function_call | "(" expr ")" ;

payload_ref     = "payload" | "payload" field_path ;
field_path      = ("." ident | "[" string_literal "]" | "[" index_literal "]")+ ;
topic_ref       = "topic" "[" index_literal "]" ;
property_ref    = "properties" "[" string_literal "]" ;
metadata_ref    = "metadata" "." ident | "metadata" "[" string_literal "]" ;
function_call   = ident "(" (expr ("," expr)* ","?)? ")" ;

literal         = string_literal | int_literal | float_literal | bool_literal | null ;
ident_list      = ident ("," ident)* ","? ;
```

Identifiers must reject reserved keywords. String and numeric literal parsing must be fallible so invalid escapes and integer overflow are reported as parser diagnostics rather than panics.

## AST Layer

The AST preserves source-level structure and source locations for diagnostics.

Example shape:

```rust
RuleAst {
    topic_filter: String,
    where_expr: Option<ExprAst>,
    projection: ProjectionAst,
    destinations: Vec<String>,
}
```

The AST should not be optimized and should not be evaluated directly.

## Semantic Analyzer

The semantic analyzer validates:

- MQTT topic filter syntax.
- Destination names.
- Namespace usage: `payload`, `topic`, `properties`, `metadata`.
- Function/operator support.
- Static type errors where possible.
- Unsupported syntax.

The analyzer should produce clear source-positioned diagnostics.

## Logical IR

Logical IR is syntax-independent and describes Tenon's execution semantics.

Example:

```rust
CompiledRuleIr {
    topic_filter: TopicFilter,
    predicate: Option<ExprIr>,
    projection: ProjectionIr,
    destinations: Vec<DestinationId>,
}
```

Expression IR:

```rust
ExprIr::Literal(...)
ExprIr::PayloadField(...)
ExprIr::TopicLevel(...)
ExprIr::Property(...)
ExprIr::Metadata(...)
ExprIr::Binary { op, left, right }
ExprIr::Unary { op, expr }
```

## Optimizer

Initial optimizer scope should stay small:

- Constant folding.
- Boolean simplification.
- Literal projection precomputation.
- Field-path interning.
- Destination interning.
- Topic-level dependency collection.
- Payload-field dependency collection.

Do not add complex optimizer infrastructure before benchmark data requires it.

## Bytecode IR

Bytecode should be public for review, but can be marked experimental until the DSL/runtime contract stabilizes.

Suggested structure:

```text
bytecode_version
constant_pool
field_pool
instructions
```

Suggested initial instructions:

```text
LOAD_CONST
LOAD_PAYLOAD_FIELD
LOAD_TOPIC_LEVEL
LOAD_PROPERTY
LOAD_METADATA

ADD
SUB
MUL
DIV
REM

EQ
NE
GT
GE
LT
LE

AND
OR
NOT

PROJECT_FIELD
RETURN
```

Predicate and projection can be compiled into separate programs:

```rust
CompiledRule {
    predicate_program: Option<Program>,
    projection_program: Program,
    destinations: Vec<DestinationId>,
}
```

## VM Layer

Use a simple stack VM for v1.

Runtime pipeline:

```text
MQTT message
  -> topic trie match
  -> JSON payload decode
  -> EvalContext
  -> predicate VM
  -> projection VM
  -> RuleEvaluation
  -> sink dispatch
```

The VM should evaluate expressions against an `EvalContext`. Topic matching and payload decoding should remain outside the VM.

## Value Model

Initial value types:

```text
null
bool
int
float
string
bytes
object
array
```

V1 behavior should be explicit:

- Arithmetic only accepts numeric values.
- Type mismatch is an evaluation error.
- Payload decode error drops the message and increments metrics.
- Predicate error means no match and increments metrics.
- Projection error drops the result and increments metrics.
- Missing fields should be documented consistently.

## Testing Requirements

Add tests at each layer:

- Grammar examples.
- Parser golden tests: DSL text -> AST.
- Negative parser tests.
- Semantic validation tests.
- Compiler golden tests: DSL -> bytecode.
- VM tests: bytecode + event context -> result.
- End-to-end tests: DSL + MQTT message -> sink output.

The parser should not accept undocumented syntax.

## Refactor Phases

### Phase 1: Public DSL Spec

- Add grammar file.
- Add DSL semantics document.
- Add examples.
- Decide JSON-only v1 syntax.

### Phase 2: Parser and AST

- Implement lexer/parser.
- Produce AST with source spans.
- Add parser tests.

### Phase 3: Semantic Analyzer and Logical IR

- Validate names, topic filters, operators, and destinations.
- Compile AST into logical IR.
- Add semantic error tests.

### Phase 4: Bytecode Compiler

- Define bytecode format.
- Compile logical IR expressions into bytecode.
- Add bytecode golden tests.

### Phase 5: VM

- Implement stack VM.
- Evaluate predicate and projection programs.
- Add VM unit tests.

### Phase 6: Runtime Integration

- Replace current SQL-derived rule execution path.
- Keep MQTT adapter, topic trie, sink dispatch, and metrics integration.
- Add end-to-end tests.

### Phase 7: Remove SQL Frontend

- Remove SQL parser dependency.
- Remove SQL-specific AST/plan code.
- Remove protobuf-specific rule/schema paths from v1.x.

## Compatibility Policy

Proposed policy:

- DSL syntax and semantics become stable after v1.0.
- Bytecode IR is public for community review, but experimental until explicitly declared stable.
- Internal VM implementation can evolve as long as public DSL behavior remains compatible.
