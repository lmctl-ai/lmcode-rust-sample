//! lmscript compiles source. This executable only loads verified bytecode and runs it.
mod shop;
use dslvm::stdlib::ResultSink;
use dslvm::{error_json, report_app_failure, report_error, Error, Program, Value};
use std::io::{self, Write};

// Include exact bytes, including whitespace: the schema hash binds every program.
const SCHEMA: &[u8] = include_bytes!("../sample-suite/sample.dsl.json");
const USAGE: &str = "usage: --version | --lmcode-schema [--json] | --lmcode-signatures | --lmcode FILE";

fn main() {
    std::process::exit(match run() {
        Ok(code) => code,
        Err(_) => report_app_failure("cannot read sample input"),
    });
}

fn run() -> Result<i32, Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let mut host = shop::Host::new()?;
    if args.len() == 1 && args[0] == "--version" {
        println!("lmcode-rust-sample {}", env!("SAMPLE_BUILD_VERSION"));
        return Ok(0);
    }
    if (args.len() == 1 || (args.len() == 2 && args[1] == "--json")) && args[0] == "--lmcode-schema" {
        io::stdout().lock().write_all(SCHEMA)?;
        return Ok(0);
    }
    if args.len() == 1 && args[0] == "--lmcode-signatures" {
        for signature in host.registry.signatures() {
            println!("{signature}");
        }
        return Ok(0);
    }
    if args.len() != 2 || args[0] != "--lmcode" {
        println!("{}", error_json("cli", &Error::new("CLI_USAGE", USAGE)));
        return Ok(2);
    }
    let bytes = std::fs::read(&args[1])?;
    let program = match Program::load(&bytes, SCHEMA, &host.registry) {
        Ok(program) => program,
        Err(error) => {
            // Normalize only load messages, exactly as the reference contract does.
            let message = match error.code.as_str() {
                "LOAD_SCHEMA_MISMATCH" => "sample schema mismatch",
                "LOAD_UNBOUND" => "sample import mismatch",
                _ => "invalid sample bytecode",
            };
            return Ok(report_error("load", &Error::new(&error.code, message)));
        }
    };
    // One logical invocation owns the result across body and entry arenas.
    host.context.result_sink = Some(ResultSink::default());
    if let Err(error) = program.run(&mut host.context, &host.registry) {
        return Ok(report_error("run", &error));
    }
    let mut entries_called = 0usize;
    for (index, entry) in program.entries().iter().enumerate() {
        let arguments = match entry.annotation.as_str() {
            "item" => host
                .item_handles()?
                .into_iter()
                .map(|obj| Value::object("Money", obj))
                .collect(),
            "total" => vec![host.total_handle()?],
            _ => continue,
        };
        for argument in arguments {
            let value = match program.call(index, &mut host.context, &host.registry, vec![argument])
            {
                Ok(value) => value,
                Err(error) => return Ok(report_error("run", &error)),
            };
            entries_called += 1;
            if value != Value::None {
                let text = match value {
                    Value::Int(n) => n.to_string().into_bytes(),
                    Value::Float(n) => dslvm::format_g15(n).into_bytes(),
                    Value::Bool(b) => b.to_string().into_bytes(),
                    Value::Str(s) => s.as_bytes().to_vec(),
                    _ => Vec::new(),
                };
                let mut out = io::stdout().lock();
                write!(out, "entry {} = ", entry.name)?;
                out.write_all(&text)?;
                out.write_all(b"\n")?;
            }
        }
    }
    let ledger = host.context.get::<shop::Ledger>(host.ledger)?;
    if ledger.balance < 0 {
        return Ok(report_app_failure("ledger: negative balance"));
    }
    // This application's result carries VM bytes. Escape controls without lossy text conversion.
    let mut out = io::stdout().lock();
    write!(
        out,
        "{{\"ok\": true, \"result\": {{\"balance\": {}, \"entries\": {}, \"note\": ",
        ledger.balance, ledger.entries
    )?;
    out.write_all(b"\"")?;
    for byte in ledger.note.as_bytes() {
        match byte {
            b'"' => out.write_all(b"\\\"")?,
            b'\\' => out.write_all(b"\\\\")?,
            b'\n' => out.write_all(b"\\n")?,
            b'\r' => out.write_all(b"\\r")?,
            b'\t' => out.write_all(b"\\t")?,
            0..=0x1f => write!(out, "\\u{byte:04x}")?,
            _ => out.write_all(&[*byte])?,
        }
    }
    writeln!(out, "\", \"entries_called\": {entries_called}}}}}")?;
    // Only successful dispatch and application validation may emit a frame.
    let frame = host
        .context
        .result_sink
        .as_ref()
        .expect("invocation result sink")
        .frame();
    out.write_all(frame.as_bytes())?;
    Ok(0)
}
