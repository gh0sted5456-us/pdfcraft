//! pdfcraft-js — Acrobat JavaScript for forms (L3).
//!
//! Field scripts (format, keystroke, validate, calculate), button actions and document-level
//! scripts run in [boa](https://boajs.dev), a JavaScript engine written in Rust, with the subset
//! of Acrobat's object model that forms use, as the public *JavaScript for Acrobat API Reference*
//! describes it:
//!
//! - `event`: `value`, `change`, `willCommit`, `rc`, `name`, `type`, `target`, `targetName`.
//! - the document (`this`, also the global object): `getField`, `numFields`,
//!   `getNthFieldName`, `numPages`, `pageNum`, `documentFileName`, `info`, `resetForm`,
//!   `calculateNow`, `print`, `submitForm`, `mailDoc`.
//! - Field objects: `name`, `type`, `value`, `valueAsString`, `defaultValue`, `readonly`,
//!   `required`, `display`, `hidden`, `numItems`, `getItemAt`, `currentValueIndices`,
//!   `isBoxChecked`, `checkThisBox`, `exportValues`, `charLimit`, `textColor`, `fillColor`,
//!   `setFocus`.
//! - `app` (`alert`, `beep`, `response`, `launchURL`, `execMenuItem`, `viewerType`, `viewerVersion`,
//!   `platform`, `setTimeOut`), `util` (`printf`, `printd`, `printx`, `scand`), `console` (`println`,
//!   `show`, `clear`), `display` and `color`.
//!
//! The engine is a sandbox: no file, network or timer access exists, and loops and recursion
//! are bounded. Side effects (alerts, field changes, resets, printing, navigation) are returned
//! in an [`Outcome`] for the caller to apply.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::sync::Arc;

use boa_engine::context::HostHooks;
use boa_engine::object::builtins::{JsArray, JsDate, JsFunction};
use boa_engine::object::{FunctionObjectBuilder, ObjectInitializer};
use boa_engine::property::Attribute;
use boa_engine::realm::Realm;
use boa_engine::{Context, JsError, JsNativeError, JsObject, JsResult, JsString, JsValue, NativeFunction, Source, js_string};

mod document;
mod format;
mod globals;
mod host;
mod model;
mod run;

#[cfg(test)]
pub(crate) use format::PRINTD_SCANNED;
use format::{printd, printf, printx, scand};
pub(crate) use globals::menu_item;
use globals::platform;
use host::{arg, as_number, error, function, s, text};
pub use model::{
    DISPLAY_HIDDEN, DISPLAY_NO_PRINT, DISPLAY_NO_VIEW, DISPLAY_VISIBLE, DocInfo, Event, FieldState, FieldType, Limits, Outcome, Panel, Request,
};
#[cfg(not(target_arch = "wasm32"))]
use run::SCRIPT_STACK;
use run::refuse;
pub(crate) use run::{CheckedCodeFromStrings, MAX_SCRIPT_BYTES};
pub use run::{run, run_shared};

pub mod formcalc;
pub mod xfa;
pub mod xfa_session;

#[cfg(test)]
mod tests;

fn update_doc_info(ctx: &mut Context) -> JsResult<()> {
    let doc = host(ctx)?.borrow().doc.clone();
    let mut info = ObjectInitializer::new(ctx);
    for (key, value) in &doc.info {
        info.property(JsString::from(key.as_str()), s(value), Attribute::all());
    }
    let info = info.build();
    ctx.register_global_property(js_string!("numPages"), JsValue::from(doc.num_pages as f64), Attribute::CONFIGURABLE)?;
    ctx.register_global_property(js_string!("documentFileName"), s(&doc.file_name), Attribute::CONFIGURABLE)?;
    ctx.register_global_property(js_string!("info"), info, Attribute::CONFIGURABLE)?;
    ctx.register_global_property(js_string!("xfa_version"), JsValue::from(XFA_VERSION), Attribute::CONFIGURABLE)?;
    if doc.xfa {
        ctx.register_global_property(js_string!("xfa_installed"), JsValue::from(true), Attribute::CONFIGURABLE)?;
    } else {
        ctx.global_object().delete_property_or_throw(js_string!("xfa_installed"), ctx)?;
    }
    Ok(())
}

/// Legacy toSource for plain form state. Refuse cycles, functions and oversized graphs
/// rather than silently serializing a different value or recursing without a bound.
fn to_source(this: &JsValue, _: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    fn quote(text: &str) -> String {
        let mut out = String::from("\"");
        for c in text.chars() {
            match c {
                '\\' => out.push_str("\\\\"),
                '"' => out.push_str("\\\""),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
                c => out.push(c),
            }
        }
        out.push('"');
        out
    }
    fn serialize(value: &JsValue, ctx: &mut Context, seen: &mut Vec<JsObject>, budget: &mut usize, bytes: &mut usize) -> JsResult<String> {
        if *budget == 0 || seen.len() >= 64 {
            return Err(error("toSource state is too large or deeply nested"));
        }
        *budget -= 1;
        if value.is_callable() {
            return Err(error("toSource does not support functions"));
        }
        let out = if let Some(o) = value.as_object() {
            if seen.contains(&o) {
                return Err(error("toSource does not support cyclic state"));
            }
            seen.push(o.clone());
            let out = if o.is_array() {
                let array = JsArray::from_object(o)?;
                let len = array.length(ctx)?;
                if len > *budget as u64 {
                    return Err(error("toSource state is too large"));
                }
                let mut items = Vec::new();
                for i in 0..len {
                    items.push(serialize(&array.get(i, ctx)?, ctx, seen, budget, bytes)?);
                }
                format!("[{}]", items.join(","))
            } else {
                let mut items = Vec::new();
                for key in o.own_property_keys(ctx)? {
                    if matches!(key, boa_engine::property::PropertyKey::Symbol(_)) {
                        continue;
                    }
                    if let boa_engine::property::PropertyKey::String(key) = &key
                        && key.len() > *bytes
                    {
                        return Err(error("toSource result is too large"));
                    }
                    let label = key.to_string();
                    *bytes =
                        bytes.checked_sub(label.len().saturating_mul(6).saturating_add(3)).ok_or_else(|| error("toSource result is too large"))?;
                    items.push(format!("{}:{}", quote(&label), serialize(&o.get(key, ctx)?, ctx, seen, budget, bytes)?));
                }
                format!("({{{}}})", items.join(","))
            };
            seen.pop();
            out
        } else if let Some(string) = value.as_string() {
            if string.len() > MAX_SCRIPT_BYTES {
                return Err(error("toSource result is too large"));
            }
            quote(&string.to_std_string_escaped())
        } else if value.is_null() {
            "null".into()
        } else if value.is_undefined() {
            "undefined".into()
        } else {
            text(value, ctx)?
        };
        // Charge every intermediate too: repeated large values cannot build a huge Vec
        // before the joined result is checked. This intentionally errs on the safe side.
        *bytes = bytes.checked_sub(out.len()).ok_or_else(|| error("toSource result is too large"))?;
        Ok(out)
    }
    let mut bytes = MAX_SCRIPT_BYTES;
    Ok(s(&serialize(this, ctx, &mut Vec::new(), &mut 10_000, &mut bytes)?))
}

struct Interpreter {
    ctx: Context,
    installed: bool,
    scripts: Vec<String>,
    doc_errors: Vec<String>,
}

impl Interpreter {
    fn new() -> Result<Self, String> {
        let ctx = Context::builder().host_hooks(std::rc::Rc::new(NoCodeFromStrings)).build().map_err(|e| e.to_string())?;
        Ok(Self { ctx, installed: false, scripts: Vec::new(), doc_errors: Vec::new() })
    }
}

impl Interpreter {
    fn run(&mut self, script: &str, event: &Event, doc: &DocInfo, fields: Arc<Vec<FieldState>>, doc_scripts: &[String], limits: Limits) -> Outcome {
        if let Some(why) = refuse(script) {
            return Outcome { rc: true, error: Some(why), ..Default::default() };
        }
        let ctx = &mut self.ctx;
        ctx.runtime_limits_mut().set_loop_iteration_limit(limits.loop_iterations);
        ctx.runtime_limits_mut().set_recursion_limit(limits.recursion);
        ctx.insert_data(RefCell::new(Host {
            fields,
            changed: BTreeSet::new(),
            alerts: Vec::new(),
            console: Vec::new(),
            requests: Vec::new(),
            doc: doc.clone(),
            kept: 0,
            dropped: 0,
        }));
        let mut out = Outcome { rc: true, value: event.value.clone(), change: event.change.clone(), ..Default::default() };
        let result = (|| -> JsResult<()> {
            if !self.installed {
                install(ctx)?;
                self.installed = true;
            } else {
                update_doc_info(ctx)?;
            }
            let ev = event_object(ctx, event);
            ctx.register_global_property(js_string!("event"), ev.clone(), Attribute::all())?;
            if self.scripts != doc_scripts {
                self.doc_errors.clear();
                for (index, d) in doc_scripts.iter().enumerate() {
                    if let Some(why) = refuse(d) {
                        self.doc_errors.push(doc_error(index, &why));
                    } else if let Err(error) = ctx.eval(Source::from_bytes(d)) {
                        self.doc_errors.push(doc_error(index, &error));
                    }
                }
                self.scripts = doc_scripts.to_vec();
            }
            let r = ctx.eval(Source::from_bytes(script))?;
            if !r.is_undefined() {
                out.result = Some(text(&r, ctx)?);
            }
            let rc = ev.get(js_string!("rc"), ctx)?;
            out.rc = rc.to_boolean();
            let v = ev.get(js_string!("value"), ctx)?;
            out.value = text(&v, ctx)?;
            let c = ev.get(js_string!("change"), ctx)?;
            out.change = text(&c, ctx)?;
            Ok(())
        })();
        if let Err(e) = result {
            out.error = Some(e.to_string());
        }
        out.doc_errors = self.doc_errors.clone();
        if let Some(h) = ctx.remove_data::<Shared>() {
            let h = h.into_inner();
            out.changed = h.changed.iter().map(|i| h.fields[*i].clone()).collect();
            out.alerts = h.alerts;
            out.console = h.console;
            out.requests = h.requests;
            if h.dropped > 0 {
                out.console.push(format!("the script's messages, console lines and requests passed their limits; {} more were left out", h.dropped));
            }
        }
        out
    }
}

fn failed_runtime(error: impl Into<String>) -> Outcome {
    Outcome { rc: true, error: Some(error.into()), ..Default::default() }
}

#[cfg(not(target_arch = "wasm32"))]
struct Invocation {
    script: String,
    event: Event,
    doc: DocInfo,
    fields: Arc<Vec<FieldState>>,
    scripts: Vec<String>,
    limits: Limits,
    reply: std::sync::mpsc::SyncSender<Outcome>,
}

/// One isolated JavaScript realm per open PDF; closing it releases the worker and globals.
#[derive(Clone)]
pub struct Runtime {
    #[cfg(not(target_arch = "wasm32"))]
    sender: std::sync::mpsc::SyncSender<Invocation>,
    #[cfg(not(target_arch = "wasm32"))]
    status: Arc<std::sync::atomic::AtomicU8>,
    #[cfg(target_arch = "wasm32")]
    interpreter: std::rc::Rc<RefCell<Interpreter>>,
}

impl Runtime {
    pub fn new() -> Result<Self, String> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let (sender, receiver) = std::sync::mpsc::sync_channel::<Invocation>(1);
            std::thread::Builder::new()
                .name("pdfcraft-document-js".into())
                .stack_size(SCRIPT_STACK)
                .spawn(move || {
                    let mut interpreter = match Interpreter::new() {
                        Ok(i) => i,
                        Err(_) => return,
                    };
                    while let Ok(call) = receiver.recv() {
                        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            interpreter.run(&call.script, &call.event, &call.doc, call.fields, &call.scripts, call.limits)
                        }));
                        match result {
                            Ok(out) => {
                                let _ = call.reply.send(out);
                            }
                            Err(_) => {
                                let _ = call
                                    .reply
                                    .send(failed_runtime("the script stopped with an internal error; reopen this PDF to run scripts again"));
                                break;
                            }
                        }
                    }
                })
                .map_err(|e| e.to_string())?;
            Ok(Self { sender, status: Arc::new(std::sync::atomic::AtomicU8::new(0)) })
        }
        #[cfg(target_arch = "wasm32")]
        Ok(Self { interpreter: std::rc::Rc::new(RefCell::new(Interpreter::new()?)) })
    }
}

impl Runtime {
    pub fn run(&self, script: &str, event: &Event, doc: &DocInfo, fields: &[FieldState], scripts: &[String], limits: Limits) -> Outcome {
        self.run_shared(script, event, doc, Arc::new(fields.to_vec()), scripts, limits)
    }
}

impl Runtime {
    pub fn run_shared(
        &self,
        script: &str,
        event: &Event,
        doc: &DocInfo,
        fields: Arc<Vec<FieldState>>,
        scripts: &[String],
        limits: Limits,
    ) -> Outcome {
        #[cfg(not(target_arch = "wasm32"))]
        {
            use std::sync::atomic::Ordering::SeqCst;
            match self.status.compare_exchange(0, 1, SeqCst, SeqCst) {
                Ok(_) => {}
                Err(1) => return failed_runtime("a script is already running"),
                Err(_) => return failed_runtime("scripts were stopped; reopen this PDF to run them again"),
            }
            let (reply, receive) = std::sync::mpsc::sync_channel(1);
            let call = Invocation { script: script.into(), event: event.clone(), doc: doc.clone(), fields, scripts: scripts.to_vec(), limits, reply };
            if self.sender.send(call).is_err() {
                self.status.store(2, SeqCst);
                return failed_runtime("the script engine stopped; reopen this PDF to run scripts again");
            }
            match receive.recv_timeout(std::time::Duration::from_secs(2)) {
                Ok(out) => {
                    self.status.store(0, SeqCst);
                    out
                }
                Err(_) => {
                    // Like XFA's deadline: abandon effects and stop this realm; never join a stuck worker.
                    self.status.store(2, SeqCst);
                    failed_runtime("the script ran too long or stopped; scripts are disabled until this PDF is reopened")
                }
            }
        }
        #[cfg(target_arch = "wasm32")]
        match self.interpreter.try_borrow_mut() {
            Ok(mut i) => i.run(script, event, doc, fields, scripts, limits),
            Err(_) => failed_runtime("a script is already running"),
        }
    }
}

use globals::{XFA_VERSION, event_object, install};
use host::{Host, Shared, host};
use run::{NoCodeFromStrings, doc_error};
