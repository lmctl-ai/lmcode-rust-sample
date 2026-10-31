# Rust sample — return-frame adoption

## Pins

Suite: lmcode-cpp 249af7744ab95caa20094493a2934a3916f46956 (123 cases; identical suite bytes
to the slice-6 6ee3d62 pin used in the full gate).
SDK: lmcode-rust ce8584fcf42575db133f53805974898dde8c4832.
Compiler qualification: private mono main build 177e38ca; npm 0.2.448 is too old.
Exact vendored hashes live in suite.lock and sdk.lock. Sync using scripts/sync.py;
never change expectations or reformat SDK source locally.

## Ownership and protocol

src/main.rs configures Context.result_sink before Program.run, then retains it
through all @item/@total calls. Only successful dispatch plus application
validation emits the terminal frame, after the unchanged diagnostic. Entry HALT
values and entries_called stay independent of $return. Registry default std
registers the 24 return overloads; the host supplies the invocation-owned sink.

## Gates

./test.sh uses LMSCRIPT_BIN, checks locks and 206 standard/45 host imports,
runs the 123 cases, reproduces the pinned bytes, tests negative runner controls
and the local examples. Shared return_protocol oracles plus real compiled host
mutations cover missing framing and sink reset between entries. Cases 365–368
cover entry/app/shape/numeric failure without a success frame.

## Limits

Qualification here is local Linux x86_64; Actions keeps its existing platform
matrix and release policy. Its npm compiler default must become compatible with
out= before the adopted corpus can pass there. Do not invent a future npm pin.
