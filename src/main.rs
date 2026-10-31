//! lmscript compiles source. This executable loads and runs one or more verified programs.
mod shop;
use dslvm::stdlib::{failure_frame, InvocationResults, ResultContract};
use dslvm::{Error, Program, Value};
use std::io::{self, Write};

// Include exact bytes, including whitespace: the schema hash binds every program.
const SCHEMA: &[u8] = include_bytes!("../sample-suite/sample.dsl.json");
const USAGE: &str = "usage: --version | --lmcode-schema [--json] | --lmcode-signatures | [--lmcode|--first|--last] FILE ...";

fn failure(phase: &str, error: Error) -> i32 {
    let mut out = io::stdout().lock();
    failure_frame(&mut out, phase, &error).unwrap_or(4)
}

fn value_text(value: &Value) -> Vec<u8> {
    match value {
        Value::Int(n) => n.to_string().into_bytes(),
        Value::Float(n) => dslvm::format_g15(*n).into_bytes(),
        Value::Bool(b) => b.to_string().into_bytes(),
        Value::Str(s) => s.as_bytes().to_vec(),
        _ => Vec::new(),
    }
}

fn run() -> Result<i32, Error> {
    let raw_args: Vec<_> = std::env::args_os().skip(1).collect();
    let args: Vec<String> = raw_args
        .into_iter()
        .map(|arg| {
            arg.into_string()
                .map_err(|_| Error::new("CLI_USAGE", "arguments must be UTF-8"))
        })
        .collect::<Result<_, _>>()?;
    let mut host = shop::Host::new().map_err(|_| Error::host("cannot initialize sample host"))?;
    if args.len() == 1 && args[0] == "--version" {
        println!("lmcode-rust-sample {}", env!("SAMPLE_BUILD_VERSION"));
        return Ok(0);
    }
    if (args.len() == 1 || (args.len() == 2 && args[1] == "--json")) && args[0] == "--lmcode-schema"
    {
        io::stdout()
            .lock()
            .write_all(SCHEMA)
            .map_err(|_| Error::host("cannot write sample schema"))?;
        return Ok(0);
    }
    if args.len() == 1 && args[0] == "--lmcode-signatures" {
        for signature in host.registry.signatures() {
            println!("{signature}");
        }
        return Ok(0);
    }

    let mut files = Vec::new();
    let mut priorities = Vec::new();
    let mut i = 0;
    while i < args.len() {
        if i + 1 >= args.len() {
            return Err(Error::new("CLI_USAGE", USAGE));
        }
        let priority = match args[i].as_str() {
            "--lmcode" => 1,
            "--first" => 0,
            "--last" => 2,
            _ => return Err(Error::new("CLI_USAGE", USAGE)),
        };
        files.push(args[i + 1].clone());
        priorities.push(priority);
        i += 2;
    }
    if files.is_empty() {
        return Err(Error::new("CLI_USAGE", USAGE));
    }

    let mut results = InvocationResults::from_command(&files)?;
    let mut execution: Vec<usize> = (0..files.len()).collect();
    execution.sort_by_key(|index| priorities[*index]);
    let schema = std::str::from_utf8(SCHEMA).map_err(|_| Error::host("invalid embedded schema"))?;
    let contract = ResultContract::from_schema(schema.as_bytes())?;
    let mut entries_called = 0usize;
    let mut failed = false;

    for position in execution {
        let part = results.registrations()[position].clone();
        let mut phase = "load";
        let execution_result = (|| -> Result<(), Error> {
            let bytes = std::fs::read(&part.file).map_err(|_| {
                phase = "app";
                Error::new("APP_FAIL", "cannot read sample input")
            })?;
            let program = Program::load(&bytes, SCHEMA, &host.registry).map_err(|error| {
                let message = match error.code.as_str() {
                    "LOAD_SCHEMA_MISMATCH" => "sample schema mismatch",
                    "LOAD_UNBOUND" => "sample import mismatch",
                    _ => "invalid sample bytecode",
                };
                Error::new(&error.code, message)
            })?;
            results.select(part.index, &mut host.context, contract.clone())?;
            phase = "run";
            program.run(&mut host.context, &host.registry)?;
            for (index, entry) in program.entries().iter().enumerate() {
                let arguments: Vec<Value> = match entry.annotation.as_str() {
                    "item" => host
                        .item_handles()?
                        .into_iter()
                        .map(|obj| Value::object("Money", obj))
                        .collect(),
                    "total" => vec![host.total_handle()?],
                    _ => continue,
                };
                for argument in arguments {
                    let value =
                        program.call(index, &mut host.context, &host.registry, vec![argument])?;
                    entries_called += 1;
                    if value != Value::None {
                        let mut out = io::stdout().lock();
                        write!(out, "entry {} = ", entry.name)
                            .map_err(|_| Error::host("cannot write entry output"))?;
                        out.write_all(&value_text(&value))
                            .and_then(|_| out.write_all(b"\n"))
                            .map_err(|_| Error::host("cannot write entry output"))?;
                    }
                }
            }
            results.complete(part.index)
        })();
        if let Err(error) = execution_result {
            let _ = results.fail(Some(part.index), phase, error);
            failed = true;
            break;
        }
    }

    let ledger = host.context.get::<shop::Ledger>(host.ledger)?;
    if !failed && ledger.balance < 0 {
        let _ = results.fail(
            None,
            "app",
            Error::new("APP_FAIL", "ledger: negative balance"),
        );
        failed = true;
    }
    if !failed {
        let mut out = io::stdout().lock();
        write!(
            out,
            "{{\"host_summary\":{{\"balance\": {}, \"entries\": {}, \"note\": \"",
            ledger.balance, ledger.entries
        )
        .map_err(|_| Error::host("cannot write sample summary"))?;
        for byte in ledger.note.as_bytes() {
            match byte {
                b'"' => out
                    .write_all(b"\\\"")
                    .map_err(|_| Error::host("cannot write sample summary"))?,
                b'\\' => out
                    .write_all(b"\\\\")
                    .map_err(|_| Error::host("cannot write sample summary"))?,
                b'\n' => out
                    .write_all(b"\\n")
                    .map_err(|_| Error::host("cannot write sample summary"))?,
                b'\r' => out
                    .write_all(b"\\r")
                    .map_err(|_| Error::host("cannot write sample summary"))?,
                b'\t' => out
                    .write_all(b"\\t")
                    .map_err(|_| Error::host("cannot write sample summary"))?,
                0..=0x1f => write!(out, "\\u{byte:04x}")
                    .map_err(|_| Error::host("cannot write sample summary"))?,
                _ => out
                    .write_all(&[*byte])
                    .map_err(|_| Error::host("cannot write sample summary"))?,
            }
        }
        writeln!(out, "\", \"entries_called\": {entries_called}}}}}")
            .map_err(|_| Error::host("cannot write sample summary"))?;
    }
    Ok(results.write_frame(&mut io::stdout().lock()))
}

fn main() {
    let code = match run() {
        Ok(code) => code,
        Err(error) => failure(
            if error.code == "CLI_USAGE" {
                "cli"
            } else {
                "app"
            },
            error,
        ),
    };
    std::process::exit(code);
}
