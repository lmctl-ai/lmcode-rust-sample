# Canonical Shop integration corpus

This is the source of truth for all eight `lmcode-*-sample` hosts.
See [CONTRACT.md](CONTRACT.md) for object semantics and the exact result protocol.
The full schema is [sample.dsl.json](sample.dsl.json); [host.schema.json](host.schema.json)
contains only the small shop/ledger domain. Every process starts with fresh state.

```sh
./sample-suite/run-suite.sh /path/to/lmcode-cpp-sample
./sample-suite/run-suite.sh dotnet /path/to/sample.dll
python3 sample-suite/compile.py
python3 sample-suite/audit.py
python3 sample-suite/test_runner.py
```

`run-suite.sh` checks the discovery/usage protocol, then executes each pinned
`.dslb` in a separate process with a fresh directory in `SAMPLE_TMP`. It compares
stdout bytes (including the JSON result and LF) and exit code; stderr must be empty.
No timestamps, absolute paths, OS error text, or libm decimal approximations are
part of expected output. `assert_close` checks approximate math without unstable
traces. Call arguments are an argv array, not a shell string. Each case has a
30-second harness timeout; the SDK VM itself has no execution limit.

## Change the language or add a standard function

1. Change `programs/NNN-name.lms` (maximum 40 lines), and update `coverage.json`.
2. Merge `host.schema.json` with the current canonical standard:
   `python3 scripts/merge-std.py sample-suite/host.schema.json sample-suite/sample.dsl.json`.
3. Rebuild the C++ sample **against this schema and SDK**, then run `compile.py`.
4. Run `record.py /path/to/lmcode-cpp-sample` explicitly; review its expected-output diff.
   The manifest independently declares expected exits: recording cannot bless an
   unexpected crash/error as success. Normal tests never regenerate expectations.
5. Run the corpus on at least one other SDK before accepting the oracle.
   Initial validation: C++ and Go matched all 50 cases exactly.
6. Commit sources, bytecode, manifest and expectations together. Sync the pinned
   suite into every sample and run every port's CI. `audit.py` parses actual
   bytecode import tables and fails if any canonical std signature is uncovered.

`compile.py --output TEMP_DIR` recompiles without changing the committed manifest,
for reproducibility checks. `LMSCRIPT_BIN` selects the compiler (>=0.2.420);
bytecode format is v5. `protocol.py` independently derives signatures from the
schema, so a host missing/adding registrations fails even if no program calls it.
The malformed-bytecode and schema-hash cases are deterministic mutations of the
first compiler-emitted program, never hand-authored executable bytecode.

## Feature -> program matrix

| Program | Feature / called APIs |
|---|---|
| [001-literals](programs/001-literals.lms) | literals; print(string) |
| [002-arithmetic](programs/002-arithmetic.lms) | arithmetic; int operators; fee(int) |
| [003-boolean](programs/003-boolean.lms) | boolean; comparisons |
| [004-if-else](programs/004-if-else.lms) | if; else |
| [005-while](programs/005-while.lms) | while |
| [006-range-for](programs/006-range-for.lms) | range; for |
| [007-switch](programs/007-switch.lms) | switch; break; default |
| [008-function](programs/008-function.lms) | function; return |
| [009-money-overloads](programs/009-money-overloads.lms) | money(int); money(float); Shop.price(int); Shop.price(float) |
| [010-money-methods](programs/010-money-methods.lms) | Money.amount(); Money.currency(); Money.set(int) |
| [011-money-operators](programs/011-money-operators.lms) | Money.plus(Money); Money.minus(Money); Money.times(int); Money.neg(); Money r* |
| [012-shop-methods](programs/012-shop-methods.lms) | Shop.items(); Shop.total(); Shop.currency(); Shop.label() |
| [013-items-methods](programs/013-items-methods.lms) | Items.len(); Items.at(int) |
| [014-ledger-methods](programs/014-ledger-methods.lms) | Ledger.record(Money); Ledger.balance(); Ledger.note(string); Ledger.note(); Ledger.count() |
| [015-float-list](programs/015-float-list.lms) | float_list(); FloatList.add(float); FloatList.at(int); FloatList.clear(); FloatList.contains(float); FloatList.index_of(float); FloatList.len(); FloatList.set(int,float) |
| [016-int-list](programs/016-int-list.lms) | int_list(); IntList.add(int); IntList.at(int); IntList.clear(); IntList.contains(int); IntList.index_of(int); IntList.len(); IntList.set(int,int) |
| [017-str-list](programs/017-str-list.lms) | str_list(); StrList.add(string); StrList.at(int); StrList.clear(); StrList.contains(string); StrList.index_of(string); StrList.join(string); StrList.len(); StrList.set(int,string) |
| [018-str-map](programs/018-str-map.lms) | str_map(); StrMap.clear(); StrMap.get(string); StrMap.get_or(string,string); StrMap.has(string); StrMap.key_at(int); StrMap.len(); StrMap.remove(string); StrMap.set(string,string) |
| [019-math-int](programs/019-math-int.lms) | math_abs(int); math_ceil(int); math_floor(int); math_max(int,int); math_min(int,int); math_pow(int,int); math_round(int); math_sqrt(int) |
| [020-math-float](programs/020-math-float.lms) | math_abs(float); math_ceil(float); math_floor(float); math_max(float,float); math_min(float,float); math_pow(float,float); math_round(float); math_sqrt(float) |
| [021-string-predicates](programs/021-string-predicates.lms) | str_contains(string,string); str_ends_with(string,string); str_find(string,string); str_is_float(string); str_is_int(string); str_match(string,string); str_starts_with(string,string) |
| [022-string-transform](programs/022-string-transform.lms) | str_len(string); str_lower(string); str_repeat(string,int); str_replace(string,string,string); str_substr(string,int,int); str_trim(string); str_upper(string) |
| [023-string-convert](programs/023-string-convert.lms) | str_to_float(string); str_to_int(string); str_split(string,string) |
| [024-file-functions](programs/024-file-functions.lms) | sample_tmp(); file_write(string,string); file_append(string,string); file_exists(string); file_read(string); file_lines(string) |
| [025-assert-close](programs/025-assert-close.lms) | assert_close(float,float,float) |
| [026-divzero-expected-fail](programs/026-divzero-expected-fail.lms) | runtime divzero |
| [027-modzero-expected-fail](programs/027-modzero-expected-fail.lms) | runtime modzero |
| [028-assert-fail-expected-fail](programs/028-assert-fail-expected-fail.lms) | assert_close failure |
| [029-reject-expected-fail](programs/029-reject-expected-fail.lms) | Ledger.reject(string) |
| [030-negative-balance-expected-fail](programs/030-negative-balance-expected-fail.lms) | application negative balance |
| [031-items-oob-expected-fail](programs/031-items-oob-expected-fail.lms) | Items.at out of bounds |
| [032-intlist-oob-expected-fail](programs/032-intlist-oob-expected-fail.lms) | IntList.at out of bounds |
| [033-map-missing-expected-fail](programs/033-map-missing-expected-fail.lms) | StrMap missing key |
| [034-money-conversion](programs/034-money-conversion.lms) | int to Money conversion |
| [035-context-trace](programs/035-context-trace.lms) | context shop; context ledger |
| [036-nested-control](programs/036-nested-control.lms) | nested control flow |
| [037-map-iteration](programs/037-map-iteration.lms) | collection iteration |
| [038-floatlist-chain](programs/038-floatlist-chain.lms) | method chaining; bool expression |
| [039-enum-currency](programs/039-enum-currency.lms) | Currency enum |
| [040-full-trace](programs/040-full-trace.lms) | full integration trace |
| [041-int64-wrap](programs/041-int64-wrap.lms) | int64 wraparound |
| [042-short-circuit](programs/042-short-circuit.lms) | short circuit; runtime safety |
| [043-break-continue](programs/043-break-continue.lms) | break; continue |
| [044-items-foreach](programs/044-items-foreach.lms) | Items foreach |
| [045-switch-string-enum](programs/045-switch-string-enum.lms) | string switch; enum switch |
| [046-inner-heredoc](programs/046-inner-heredoc.lms) | inner raw heredoc <<TAG; inner dedent heredoc <<~TAG; raw ${} opacity |
| [047-outer-splice](programs/047-outer-splice.lms) | outer int/float/string/bool splices |
| [048-math-derived](programs/048-math-derived.lms) | all 18 MATH opcodes; int/float widening |
| [090-bad-bytecode](bytecode/090-bad-bytecode.dslb) | loader bad-bytecode |
| [091-schema-mismatch](bytecode/091-schema-mismatch.dslb) | loader schema-mismatch |

## CI entry points

SDK pushes/PRs run `.github/workflows/sample-suite.yml` against a pinned reference
host source, the **current** SDK/header and this canonical corpus. That makes a
new standard signature without a corpus import a CI failure. The reference
sample also runs its vendored snapshot natively on Linux/macOS/Windows.
After a compiler/language change, its release pipeline can send the
`lmcode-language-changed` repository_dispatch event with `compiler` in the payload;
or the language owner runs:

```sh
gh workflow run sample-suite.yml -R lmctl-ai/lmcode-cpp -f compiler=latest
```

The job recompiles sources and rejects bytecode drift for explicit review; it
never silently regenerates oracle files. Each additional sample should run the
same `run-suite.sh` in its own CI and sync the reviewed suite lock on updates.
