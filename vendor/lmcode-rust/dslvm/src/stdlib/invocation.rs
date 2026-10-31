//! One collector per process invocation; hosts drain accepted work before framing.
use super::{
    json::{parse_document, JsonNode, Kind},
    result::{emit, json_node, OwnedJson, ResultSink},
    result_contract::{member, node_integer, ResultContract},
};
use crate::{json_escape, Context, Error};
use std::{io::Write, path::Path};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvocationRegistration {
    pub file: String,
    pub index: usize,
}
#[derive(Default)]
struct Slot {
    sink: Option<ResultSink>,
    result: Option<OwnedJson>,
}
struct Failure {
    phase: String,
    error: Error,
    part: Option<usize>,
}
pub struct InvocationResults {
    builder: bool,
    writing: bool,
    registrations: Vec<InvocationRegistration>,
    slots: Vec<Slot>,
    failure: Option<Failure>,
}
fn metadata(s: impl Into<String>) -> Error {
    Error::new("CLI_USAGE", format!("lmcode invocation: {}", s.into()))
}
fn keys(n: &JsonNode, allowed: &[&str]) -> Result<(), Error> {
    if n.kind != Kind::Object {
        return Err(metadata("object required"));
    }
    for (k, _, _) in &n.object {
        let key = std::str::from_utf8(k).unwrap();
        if !allowed.contains(&key) {
            return Err(metadata(format!("unknown key {key}")));
        }
    }
    Ok(())
}
fn phase_code(phase: &str) -> Result<i32, Error> {
    match phase {
        "cli" => Ok(2),
        "load" | "run" => Ok(3),
        "app" => Ok(4),
        _ => Err(Error::host("lmcode invocation: invalid failure phase")),
    }
}
fn failure_json(phase: &str, e: &Error, part: Option<usize>) -> Result<String, Error> {
    phase_code(phase)?;
    let mut s = format!(
        "{{\"ok\":false,\"phase\":{},\"code\":{},\"message\":{}",
        json_escape(phase),
        json_escape(&e.code),
        json_escape(&e.message)
    );
    if e.column.is_some() && (e.line.unwrap_or(0) == 0 || e.column == Some(0)) {
        return Err(Error::host(
            "lmcode invocation: column requires a positive line",
        ));
    }
    if let Some(line) = e.line {
        if line == 0 {
            return Err(Error::host("lmcode invocation: line must be positive"));
        }
        s.push_str(&format!(",\"line\":{line}"));
        if let Some(column) = e.column {
            s.push_str(&format!(",\"column\":{column}"));
        }
    }
    if let Some(part) = part {
        s.push_str(&format!(",\"part\":{part}"));
    }
    s.push('}');
    Ok(s)
}
fn output<W: Write>(out: &mut W, json: &str, code: i32) -> i32 {
    let bytes = format!("LMCODE_RETURN_BEGIN\n{json}\nLMCODE_RETURN_END\n");
    if out
        .write_all(bytes.as_bytes())
        .and_then(|_| out.flush())
        .is_ok()
    {
        code
    } else {
        let _ = writeln!(
            std::io::stderr(),
            "lmcode invocation: cannot write envelope"
        );
        4
    }
}
/// Pre-collector failures only. Return immediately after emitting, even on I/O failure.
pub fn failure_frame<W: Write>(out: &mut W, phase: &str, error: &Error) -> Result<i32, Error> {
    Ok(output(
        out,
        &failure_json(phase, error, None)?,
        phase_code(phase)?,
    ))
}
impl InvocationResults {
    fn new(builder: bool, registrations: Vec<InvocationRegistration>) -> Self {
        let slots = (0..registrations.len()).map(|_| Slot::default()).collect();
        Self {
            builder,
            writing: false,
            registrations,
            slots,
            failure: None,
        }
    }
    pub fn from_metadata(raw: &[u8], parsed_files: &[String]) -> Result<Self, Error> {
        let parse = || -> Result<Self, Error> {
            let root = parse_document(raw).map_err(|e| {
                if e.message == "json_parse: BOM is not allowed at byte 0" {
                    Error::host("JSON BOM is not allowed")
                } else if e.message.starts_with("json_parse: trailing input at byte ") {
                    Error::host("trailing JSON input")
                } else {
                    e
                }
            })?;
            json_node(&root, "")?;
            keys(&root, &["version", "mode", "parts"])?;
            if member(&root, "version").and_then(node_integer) != Some(1) {
                return Err(metadata("unsupported version"));
            }
            let mode = member(&root, "mode")
                .filter(|n| n.kind == Kind::String)
                .map(|n| n.text.as_slice());
            let builder = match mode {
                Some(b"single") => false,
                Some(b"builder") => true,
                _ => return Err(metadata("invalid mode")),
            };
            let parts = member(&root, "parts")
                .filter(|n| {
                    n.kind == Kind::Array && !n.array.is_empty() && (builder || n.array.len() == 1)
                })
                .ok_or_else(|| metadata("invalid parts"))?;
            let mut files = Vec::new();
            for (i, p) in parts.array.iter().enumerate() {
                keys(p, &["index", "file"])?;
                if member(p, "index").and_then(node_integer) != Some(i as i64) {
                    return Err(metadata("noncontiguous index"));
                }
                let file = member(p, "file")
                    .filter(|n| n.kind == Kind::String)
                    .map(|n| String::from_utf8(n.text.clone()).unwrap())
                    .ok_or_else(|| metadata("file must be absolute"))?;
                if file.contains('\0') || !Path::new(&file).is_absolute() {
                    return Err(metadata("file must be absolute"));
                }
                if files.contains(&file) {
                    return Err(metadata("duplicate file"));
                }
                files.push(file);
            }
            if parsed_files.len() != files.len() {
                return Err(metadata("registration count mismatch"));
            }
            let mut seen = vec![false; files.len()];
            let mut registrations = Vec::new();
            for file in parsed_files {
                let index = files
                    .iter()
                    .position(|f| f == file)
                    .ok_or_else(|| metadata("extra registration"))?;
                if seen[index] {
                    return Err(metadata("duplicate registration"));
                }
                seen[index] = true;
                registrations.push(InvocationRegistration {
                    file: file.clone(),
                    index,
                });
            }
            Ok(Self::new(builder, registrations))
        };
        parse().map_err(|e| {
            if e.code == "CLI_USAGE" {
                e
            } else {
                metadata(e.message)
            }
        })
    }
    /// Explicit environment seam; None means standalone, not ambient discovery.
    pub fn from_command_with_manifest(
        files: &[String],
        manifest: Option<&Path>,
    ) -> Result<Self, Error> {
        if let Some(path) = manifest {
            if !path.is_absolute() || path.to_str().is_none_or(|s| s.contains('\0')) {
                return Err(metadata("manifest path must be absolute"));
            }
            let raw = std::fs::read(path).map_err(|_| metadata("cannot read manifest"))?;
            return Self::from_metadata(&raw, files);
        }
        if files.is_empty() {
            return Err(metadata("empty parts"));
        }
        let mut regs = Vec::new();
        for (index, file) in files.iter().enumerate() {
            if file.is_empty() || file.contains('\0') {
                return Err(metadata("invalid command file"));
            }
            regs.push(InvocationRegistration {
                file: file.clone(),
                index,
            });
        }
        Ok(Self::new(files.len() > 1, regs))
    }
    pub fn from_command(files: &[String]) -> Result<Self, Error> {
        let path = std::env::var_os("LMCODE_INVOCATION_FILE");
        Self::from_command_with_manifest(files, path.as_deref().map(Path::new))
    }
    pub fn registrations(&self) -> &[InvocationRegistration] {
        &self.registrations
    }
    fn active(&self) -> Result<(), Error> {
        if self.writing {
            Err(Error::host("lmcode invocation: finalized"))
        } else {
            Ok(())
        }
    }
    fn slot(&mut self, index: usize) -> Result<&mut Slot, Error> {
        self.slots
            .get_mut(index)
            .ok_or_else(|| Error::host("lmcode invocation: invalid slot index"))
    }
    /// Rust routes sinks through Context, not Registry; a cloned binding shares the seal.
    pub fn select(
        &mut self,
        index: usize,
        ctx: &mut Context,
        contract: ResultContract,
    ) -> Result<(), Error> {
        self.active()?;
        let failed = self.failure.is_some();
        let slot = self.slot(index)?;
        if slot.sink.is_none() {
            if failed {
                return Err(Error::host(
                    "lmcode invocation: cannot start work after failure",
                ));
            }
            slot.sink = Some(ResultSink::new(contract.clone()));
        }
        let sink = slot.sink.as_ref().unwrap();
        if sink.sealed() {
            return Err(Error::host("lmcode return: result sink is sealed"));
        }
        if sink.contract() != contract {
            return Err(Error::host("lmcode invocation: result contract changed"));
        }
        ctx.result_sink = Some(sink.clone());
        Ok(())
    }
    pub fn complete(&mut self, index: usize) -> Result<(), Error> {
        self.active()?;
        let slot = self.slot(index)?;
        let sink = slot
            .sink
            .as_ref()
            .ok_or_else(|| Error::host("lmcode invocation: slot was not selected"))?;
        match sink.seal() {
            Ok(value) => {
                slot.result = Some(value);
                Ok(())
            }
            Err(e) => {
                self.fail(Some(index), "run", e.clone())?;
                Err(e)
            }
        }
    }
    pub fn fail(&mut self, index: Option<usize>, phase: &str, error: Error) -> Result<(), Error> {
        self.active()?;
        if self.failure.is_some() {
            return Ok(());
        }
        if let Some(i) = index {
            self.slot(i)?;
        }
        failure_json(phase, &error, None)?;
        self.failure = Some(Failure {
            phase: phase.into(),
            error,
            part: if self.builder { index } else { None },
        });
        Ok(())
    }
    pub fn write_frame<W: Write>(&mut self, out: &mut W) -> i32 {
        if self.writing {
            return 4;
        }
        if self.failure.is_none() {
            if let Some(index) = self.slots.iter().position(|s| s.result.is_none()) {
                let _ = self.fail(
                    Some(index),
                    "run",
                    Error::host("lmcode invocation: slot not completed"),
                );
            }
        }
        let (json, code) = if let Some(f) = &self.failure {
            (
                failure_json(&f.phase, &f.error, f.part).unwrap(),
                phase_code(&f.phase).unwrap(),
            )
        } else {
            let mut s = String::from("{\"ok\":true,\"result\":");
            if self.builder {
                s.push('[');
                for (i, slot) in self.slots.iter().enumerate() {
                    if i > 0 {
                        s.push(',')
                    }
                    emit(slot.result.as_ref().unwrap(), &mut s);
                }
                s.push_str(&format!("],\"parts\":{}", self.slots.len()));
            } else {
                emit(self.slots[0].result.as_ref().unwrap(), &mut s);
            }
            s.push('}');
            (s, 0)
        };
        self.writing = true;
        output(out, &json, code)
    }
}
