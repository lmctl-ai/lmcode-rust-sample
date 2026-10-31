# lmcode Rust shop sample

**Agent entry point:** start with [domain types and bindings](src/shop.rs),
[CLI and result protocol](src/main.rs), and the [portable contract](sample-suite/CONTRACT.md).
This is a native Rust host for compiled lmscript bytecode, not an interpreter.
It uses the vendored MIT [dslvm crate](vendor/lmcode-rust/dslvm) and Rust's standard
library, with no crates.io dependencies or sibling checkout needed to build.

## Prerequisites

Use a compiler built from mono main at **177e38ca or later**. Published
`@lmctl-ai/lmctl` 0.2.448 lacks `out=`; a source build may report 0.2.38, so
its source commit and successful corpus reproduction establish capability.
Set `LMSCRIPT_BIN` to that build's `lmscript/bin/lmscript`, then run `./test.sh`.
The compiler is external to this host; no global installation is required.


Use Rust stable, Python 3 (vendor locks), Bash, and Node 24 with current lmscript.
CI runs the same test on Linux x64/arm64, macOS x64/arm64, and Windows x64.

## Write a host in ten steps

1. **Choose domain objects.** [shop.rs](src/shop.rs) defines `Money` (signed cents),
   `Items`, `Shop` and `Ledger`; a fresh shop contains 125, 250 and 375 cents.
2. **Declare the API.** [host.schema.json](sample-suite/host.schema.json) declares
   contexts, overloads, operators, conversions, enum values, void and fluent methods.
   [`sample.dsl.json`](sample-suite/sample.dsl.json) merges those declarations with std.
3. **Pin every dependency byte.** [sdk.lock](sdk.lock) and [suite.lock](suite.lock)
   record commit IDs and SHA-256s; [sync.py](scripts/sync.py) checks drift or archives
   reviewed Git commits. Build and verification use the checked-in copies.
4. **Make a registry.** [Host::new and register](src/shop.rs) start with
   `Registry::new()` (all 206 standard signatures), declare `shop`/`ledger`, then opt into
   [testing helpers](vendor/lmcode-rust/dslvm/src/stdlib/testing.rs). The host contributes
   45 signatures; the catalog audit imports all 206 standard signatures.
5. **Own live objects in Context.** [Host::new](src/shop.rs) inserts the domain
   objects and binds context names. `ObjRef` is an arena index, not a native pointer;
   `Context::get/get_mut` borrow typed Rust objects. No `unsafe` or leaked allocation.
6. **Bind callbacks.** [register](src/shop.rs) uses `function` and `method` with
   complete parameter/return signatures. Closures receive `&mut Context` plus values;
   methods also receive the receiver handle. Distinct tuples implement overloads.
7. **Translate lmcode concepts.** [Money/Items/Ledger methods](src/shop.rs) show
   operators, `money(int)` conversion, string-backed `Currency`, `len/at` foreach,
   `returns:self` registered as its owner type, and `Value::None` for void.
8. **Compile outside the host.** [program wrappers](sample-suite/programs) use
   `_lmcode_ schema="..." out=ARGV[0]`. [main.rs](src/main.rs) embeds the exact schema bytes,
   `Program::load` verifies/binds `.dslb`, and `run` executes against this context.
9. **Keep error boundaries separate.** [main.rs](src/main.rs) uses SDK result/error
   helpers: CLI 2, load/run 3, application 4, success 0. Only loader wording is
   normalized; the VM adds lmcode line/column. A negative final balance is an app rule.
10. **Check the shared oracle.** [test.sh](test.sh) checks locks and all imports,
    reproduces bytecode with the compiler, runs every case, and runs negative harness
    controls. Read the [coverage matrix](sample-suite/README.md) before adding APIs.

## Conversions at the Rust boundary

| lmcode type | Rust type | One-line rule |
|---|---|---|
| `int` | `i64` / `Value::Int` | Use explicit `wrapping_*`; never platform-sized `isize`. |
| `float` | `f64` / `Value::Float` | IEEE binary64; VM `str(float)` uses SDK `format_g15` (C `%.15g`), including signed zero/non-finite values. |
| `bool` | `bool` / `Value::Bool` | Preserve the boolean tag; do not coerce integers. |
| `string` | `ByteString` / byte slices | Preserve NUL and invalid UTF-8; offsets are bytes, and `as_str()` explicitly validates UTF-8. |
| enum `Currency` | string-backed `Value` | Register return type `Currency`; return `Value::string("USD")`. |
| object | `ObjRef` plus schema type name | `Context` owns `Box<dyn Any>`; clone handles, borrow objects, and drop the arena after the run. |
| list/map | SDK collection handles | Default std manages [collection storage](vendor/lmcode-rust/dslvm/src/stdlib/collections.rs) per run; schema methods expose it to bytecode. |
| `void` | `Value::None` | No value is pushed; fluent methods return the same typed object handle instead. |
| error | `Result<Value, dslvm::Error>` | `Error::host` carries UTF-8 error text; `Ledger.reject` validates that text, and the VM adds location. |

Ledger notes stay byte strings. The final JSON writer in [main.rs](src/main.rs)
escapes controls and preserves other bytes exactly like the C++ reference; the
optional SDK `print` helper likewise writes bytes and LF, without lossy decoding.
File functions use the real host filesystem. `SAMPLE_TMP` is the suite's scratch
convention, not a sandbox. Native collection adapters copy values through
`Context::insert_collection`, keeping std quota and cleanup rules. The host models
a typed null Money with a private sentinel object and checks it before access.

## Feature coverage

| Feature | Cases |
|---|---|
| Literals, arithmetic, booleans, branches, inline functions, loops/switch | 001–008, 036, 043, 045 |
| Annotated SAM rule called from the chunk | 067 |
| Host-dispatched item/total rules and returned scores | 068–069 |
| Invocation return values, owned copies, entry lifetime and failure suppression | 360–368 |
| v5 bytecode refused by the v6 runtime | 093 |
| Pure host function `fee(int)` | 002 |
| Overloads, handles, operators, conversion, chaining, enums, contexts | 009–014, 034–035, 039–040 |
| All std collection methods, typed maps/stacks/queues/priority queues and host foreach | 015–018, 037–038, 044, 101–104 |
| All std math functions and all 18 intrinsic math operations | 019–020, 025, 048 |
| All std string and file functions | 021–024 |
| Located VM/host/testing/application errors | 026–033 |
| int64 wrap, short-circuit execution | 041–042 |
| Raw/dedented heredocs and all outer splice types | 046–047 |
| Native conversions, byte/float boundaries, null/wrong handles, large inputs and 255 arguments | 049–066 |
| JSON navigation and JSON type errors | 105, 108 |
| Contextual typed lists/maps, JSONC documents and legacy bindings | 201–205 |
| Byte codecs, strict rejection and independent map snapshots | 301–305, 311–313 |
| Container empty/nonfinite-priority errors | 106–107 |
| Bad bytecode and schema binding rejection | 090–092 |

The current suite has 119 source programs plus four bytecode mutations: **123 cases**.
Every stdout byte, JSON field order/spacing, newline and process exit must match.
Expected files are vendored unchanged from `lmctl-ai/lmcode-cpp/sample-suite`.
If an oracle appears wrong, record both implementations and reasoning in that
canonical repository's `sample-suite/ISSUES.md`; do not edit local expectations.

## Run or update

Build with `cargo build --release --locked --offline`. The executable is
`target/release/lmcode-rust-sample`; inspect it with `--version`,
`--dsl-schema --json` or `--dsl-signatures`, and execute pinned bytes with
`--dsl sample-suite/bytecode/040-full-trace.dslb`.

Update reviewed pins with `scripts/sync-sdk.sh SOURCE COMMIT` and
`scripts/sync-suite.sh SOURCE COMMIT`. Run the full gate with `./test.sh` after
setting `LMSCRIPT_BIN`. Vendored files are copied from committed upstream bytes;
do not reformat them or record expectations from this port.


Windows produces `target/release/lmcode-rust-sample.exe`; the test script handles
that suffix. The application is MIT licensed; the vendored SDK's MIT text is
[retained alongside it](vendor/lmcode-rust/LICENSE).

## Automated tests and releases

Use Rust stable and the compatible compiler described above. From the repository root run `./test.sh`; it forwards to that compiler and `test.lms`. Existing Actions compiler defaults require a compatible published compiler before this new corpus can qualify; no future package version is assumed here.

Every push to main builds and tests Linux x64/arm64, macOS x64/arm64, and Windows x64 executables. Only after all five jobs pass does Actions publish a GitHub release with platform binaries, licenses and SHA256SUMS. Pull requests run the same tests without publishing.

`examples/rule-entries.lms` demonstrates annotated functions implementing `Adjuster`. After running the body, this host dispatches `@item` once per item and `@total` once for the total; other annotations are ignored. Case 069 demonstrates returned values.

The lmscript driver verifies locks, audits 206 standard and 45 host signatures, runs all 123 cases with exact bytes/exits, reproduces bytecode, exercises negative runner controls and checks the annotated example. Node provides binary/process adapters; Python verifies/synchronizes vendored locks, checks the shared frame oracles and compiles host mutation controls. Canonical suite changes belong in lmcode-cpp first.

## Text templates

[examples/text-template.lms](examples/text-template.lms) renders ordinary text from a nested context with indexed lists, loops and boolean conditions. The standard `template_render(string, Json) -> string` function performs one pass; inserted values stay literal. Compile it with `$LMSCRIPT_BIN --no-preflight examples/text-template.lms build/text-template.dslb`, then run the sample host with `--dsl build/text-template.dslb`.

The lmscript-driven suite runs 123 cases, including template success/error cases. See the [portable template contract](https://github.com/lmctl-ai/lmcode-cpp/blob/main/std/template.md).

## Return protocol

`Context.result_sink` owns one `ResultSink` for the body and all selected entry
calls. Standard arena cleanup must not reset it. `$return` does not change the
existing shop diagnostic or an entry's non-void HALT trace. After dispatch and
negative-balance validation, the CLI writes the shop diagnostic and the sink's
terminal frame. Unset means null; load, body, entry and application errors emit
no frame. Frame floats use the SDK's shortest-round-trip formatter, independent
of the VM's `str(float)` formatting.

### Program

For a source file in `examples/`:

```lmscript
program = _lmcode_ schema="../sample-suite/sample.dsl.json" out=ARGV[0];
$return.name = "shop";
$return.total = shop.total().amount();
end _lmcode_
```

### Expected result

```text
{"ok": true, "result": {"balance": 0, "entries": 0, "note": "", "entries_called": 0}}
LMCODE_RETURN_BEGIN
{"name":"shop","total":750}
LMCODE_RETURN_END
```

### Validation

The normal gate runs ten shared Python/Node protocol controls and the actual
compiled-host controls in `scripts/test-return-controls.py`. Missing framing and
per-entry sink reset must fail independent canonical expectations; compilation
failure or a changed process exit is not counted as a caught fault.
See [durable notes](durable-memory/index.md) for pins and ownership boundaries.

The host loads lmcode bytecode (DSLB v6).

## Historical literal names

The documentation calls the language lmcode. `--dsl`, `--dsl-schema` and `--dsl-signatures` are current host flag literals; `DSLB` is the bytecode magic and `DSLB v6` is the current format name; `.dslb` is the current bytecode filename extension. Other code-formatted API names, paths, manifest names and diagnostic text retain their current literal spellings.
