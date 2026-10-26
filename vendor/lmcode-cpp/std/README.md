# lmcode std v1

`lmcode-std.schema.json` is the canonical, byte-pinned standard surface. Its
schema name `lmcode_std_v1` carries standard-library version 1; `dsl.schema: 2`
is the schema-language version and bytecode version 5 is separate. It is directly
readable by the current `lmscript --dsl-signatures` command. The merge helper adds
only its declarations to a host schema, preserving host metadata.

The file combines string, math, collection and file declarations: 72 canonical
signatures. The old `stdlib/*.schema-fragment.json` files are compatibility
subsets, except the explicitly optional testing fragment, which is outside std. Other SDKs copy this file exactly and pin its SHA-256 and source revision
in `std.lock`; they must not regenerate it locally. The source SDK also checks its canonical bytes against the local `std.lock` digest.

Until implicit compiler injection is available, merge it at build time:

```sh
python3 scripts/merge-std-schema.py host.schema.json merged.schema.json
lmscript --dsl-signatures merged.schema.json
```

The helper retains host metadata and accepts identical declarations or additional
overloads/methods. A conflicting standard signature is an error. Use the exact
merged bytes both to compile bytecode and to load it; schema-hash binding does not
change. `--std PATH` selects a vendored canonical copy, and `-` selects stdin or
stdout for the corresponding positional argument.
