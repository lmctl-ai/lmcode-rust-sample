# Shop host contract (shared by every sample)

Schema bytes: `sample.dsl.json` (schema 2, merged default std); domain-only declarations: `host.schema.json`.
- Fresh process per case. `shop` and `ledger` are contexts. Own object pointers in a stable per-process arena.
- Money is signed int64 cents. `money(int)` / `shop.price(int)` accept cents;
  their float overloads accept units and round half away from zero to cents (finite/in-range only).
  All integer money operations wrap modulo 2^64. `Money.set(int)` mutates/returns its receiver.
- `Money.currency()` and `Shop.currency()` return string-backed enum `Currency.USD`.
- Shop label = `sample shop`; items contain Money cents [125,250,375]; total = Money(750).
- `Items.at` outside [0,len) throws `items: index out of range`.
- `Ledger.record(Money)` wraps balance and increments entries; returns the same Ledger.
  `note(string)` stores text (void), `note()` retrieves it; initial note empty, balance/entries zero.
  `reject(string)` throws that exact message. `fee(int)` adds 5 with int64 wrapping.
- `sample_tmp()` returns SAMPLE_TMP unchanged, or throws `SAMPLE_TMP is required`.
- Register default std and optional `print(string)` / `assert_close(float,float,float)` testing catalog.
- Every execution prints trace lines via print, then one JSON result line and LF.
  Success result fields/order/spacing: `{"ok": true, "result": {"balance": 0, "entries": 0, "note": ""}}`.
  Negative final ledger balance is application failure (after VM success), exit 4:
  `{"ok": false, "phase": "app", "code": "APP_FAIL", "message": "ledger: negative balance"}`.
- VM errors use SDK codes/location and standard JSON order/spaces: ok,phase,code,message,line,column.
  Canonical load messages: LOAD_BAD_BYTECODE -> `invalid sample bytecode`;
  LOAD_SCHEMA_MISMATCH -> `sample schema mismatch`; LOAD_UNBOUND -> `sample import mismatch`.
  Host/VM run messages retain the SDK's message (including `dsl line N:C: `).
- CLI: --version => `lmcode-cpp-sample 0.1.0` (other ports use their own binary name);
  --dsl-schema [--json] => exact schema bytes; --dsl-signatures => sorted canonical signatures + LF;
  --dsl FILE => run, exit 0/3/4; usage/unknown option => exit 2 and JSON phase cli,
  code CLI_USAGE, message `usage: --version | --dsl-schema [--json] | --dsl-signatures | --dsl FILE`.
  Other command/IO setup errors become APP_FAIL, without machine-specific paths.
