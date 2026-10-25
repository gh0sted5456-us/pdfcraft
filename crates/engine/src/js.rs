//! Acrobat JavaScript in documents: field scripts run during form edits ([`JsRunner`], wired
//! into `forms` through its [`pdfcraft_forms::Scripts`] hook), and button actions and console
//! input run on request ([`Session::run_javascript`]). What scripts ask of the viewer (alerts,
//! printing, navigation, links) is kept for the front end in [`JsOutput`].
//!
//! Preferences ▸ JavaScript ▸ Enable Acrobat JavaScript is [`Session::set_javascript`]; with it
//! off, scripts don't run and AF calls keep working natively.

use pdfcraft_forms::{Field, FieldChange, FieldEvent, FieldKind, FieldProps, ScriptResult, Scripts};
pub use pdfcraft_js::{Limits, Outcome, Panel, Request};

use crate::{DocId, Edit, EditError, Session};

/// What scripts produced for the viewer since it last looked.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct JsOutput {
    pub alerts: Vec<String>,
    pub console: Vec<String>,
    pub requests: Vec<Request>,
    /// Script errors (as Acrobat's console shows them).
    pub errors: Vec<String>,
    /// Which of `errors` are document scripts' (#3940), told apart by where they came from and
    /// not by their text, which a field script's error can share: each is reported once, however
    /// often the document scripts run again.
    pub doc_errors: Vec<String>,
}

impl JsOutput {
    /// Whether scripts produced nothing: no alerts, console lines, requests or errors.
    pub fn is_empty(&self) -> bool {
        self.alerts.is_empty() && self.console.is_empty() && self.requests.is_empty() && self.errors.is_empty()
    }

    /// Adds what a field's own script produced: its alerts, console lines, errors and requests,
    /// except a request to show a side panel or open Save As, which is noted on the console
    /// and dropped.
    fn absorb(&mut self, o: &Outcome) {
        self.alerts.extend(o.alerts.iter().cloned());
        self.console.extend(o.console.iter().cloned());
        // A field's own scripts (keystroke, format, validate, calculate) run unasked: only a
        // button's may show or hide a side panel, or open Save As.
        for r in &o.requests {
            match r {
                Request::ShowPanel(_) => self.console.push("A field script asked to show a side panel; only a button may show a side panel".into()),
                Request::SaveAs => {
                    self.console.push("A field script asked to save the document as a new file; only a button may open Save As".into())
                }
                r => self.requests.push(r.clone()),
            }
        }
        self.errors.extend(o.error.iter().cloned());
        // The document scripts run before every field script, so a failing one would repeat
        // once per calculation; it is reported once.
        for e in &o.doc_errors {
            if !self.doc_errors.contains(e) {
                self.doc_errors.push(e.clone());
                self.errors.push(e.clone());
            }
        }
    }

    pub(crate) fn append(&mut self, other: JsOutput) {
        self.alerts.extend(other.alerts);
        self.console.extend(other.console);
        self.requests.extend(other.requests);
        // As in `absorb`: a document script's error, already reported, is not repeated; a field
        // script's errors are each their own, even one worded like a document script's.
        let mut docs = other.doc_errors;
        for e in other.errors {
            if let Some(k) = docs.iter().position(|d| *d == e) {
                docs.swap_remove(k);
                if self.doc_errors.contains(&e) {
                    continue;
                }
                self.doc_errors.push(e.clone());
            }
            self.errors.push(e);
        }
    }
}

fn kind(k: FieldKind) -> pdfcraft_js::FieldType {
    use pdfcraft_js::FieldType as T;
    match k {
        FieldKind::Text => T::Text,
        FieldKind::CheckBox => T::CheckBox,
        FieldKind::Radio => T::RadioButton,
        FieldKind::PushButton => T::Button,
        FieldKind::Combo => T::ComboBox,
        FieldKind::List => T::ListBox,
        FieldKind::Signature => T::Signature,
    }
}

/// `field.display` from the first widget's annotation flags.
fn display(doc: &pdfcraft_cos::Document, f: &Field) -> i32 {
    let flags = f.widgets.first().and_then(|w| doc.get(w.obj).as_dict().and_then(|d| d.int(b"F"))).unwrap_or(4);
    if flags & 2 != 0 {
        pdfcraft_js::DISPLAY_HIDDEN
    } else if flags & 32 != 0 {
        pdfcraft_js::DISPLAY_NO_VIEW
    } else if flags & 4 == 0 {
        pdfcraft_js::DISPLAY_NO_PRINT
    } else {
        pdfcraft_js::DISPLAY_VISIBLE
    }
}

/// A form field as scripts see it.
pub fn field_state(doc: &pdfcraft_cos::Document, f: &Field) -> pdfcraft_js::FieldState {
    let mut s = field_state_with_display(f, display(doc, f));
    s.actions = widget_scripts(doc, f);
    for (name, script) in [
        ("Keystroke", &f.actions.scripts.keystroke),
        ("Validate", &f.actions.scripts.validate),
        ("Calculate", &f.actions.scripts.calculate),
        ("Format", &f.actions.scripts.format),
    ] {
        if let Some(script) = script {
            s.actions.push((name.into(), script.clone()));
        }
    }
    if let Some(w) = f.widgets.first() {
        let mk = doc.get(w.obj).as_dict().and_then(|d| d.get(b"MK")).map(|m| doc.resolve(m));
        let color = mk.as_ref().and_then(|m| m.as_dict()).and_then(|m| m.get(b"BC")).map(|c| doc.resolve(c));
        if let Some(values) = color.as_ref().and_then(|c| c.as_array()) {
            let space = match values.len() {
                1 => "G",
                3 => "RGB",
                4 => "CMYK",
                _ => "T",
            };
            let mut parts = vec![space.into()];
            parts.extend(values.iter().filter_map(|v| v.as_f64()).map(|v| v.to_string()));
            s.stroke_color = Some(parts);
        }
    }
    s
}

fn field_state_with_display(f: &Field, display: i32) -> pdfcraft_js::FieldState {
    let mut s = pdfcraft_js::FieldState::new(f.name.clone(), kind(f.kind), f.value.clone());
    s.default = f.default.clone();
    s.readonly = f.read_only();
    s.required = f.has(pdfcraft_forms::flags::REQUIRED);
    s.display = display;
    s.char_limit = f.max_len;
    s.options = match f.kind {
        FieldKind::CheckBox | FieldKind::Radio => f.widgets.iter().filter_map(|w| w.on_state.clone()).map(|o| (o.clone(), o)).collect(),
        _ => f.options.clone(),
    };
    s
}

fn widget_scripts(doc: &pdfcraft_cos::Document, f: &Field) -> Vec<(String, String)> {
    use pdfcraft_cos::Object;
    let Some(w) = f.widgets.first() else { return Vec::new() };
    let widget = doc.get(w.obj);
    let Some(d) = widget.as_dict() else { return Vec::new() };
    let aa = d.get(b"AA").map(|a| doc.resolve(a));
    let aa = aa.as_ref().and_then(|a| a.as_dict());
    let mut out = Vec::new();
    for (name, key) in
        [("MouseUp", b"U".as_slice()), ("MouseDown", b"D"), ("MouseEnter", b"E"), ("MouseExit", b"X"), ("OnFocus", b"Fo"), ("OnBlur", b"Bl")]
    {
        let action = if name == "MouseUp" { d.get(b"A").or_else(|| aa.and_then(|a| a.get(key))) } else { aa.and_then(|a| a.get(key)) };
        let Some(action) = action.map(|a| doc.resolve(a)) else { continue };
        let Some(action) = action.as_dict().filter(|a| a.name(b"S") == Some(b"JavaScript")) else { continue };
        let Some(source) = action.get(b"JS").map(|s| doc.resolve(s)) else { continue };
        let source = match &*source {
            Object::String(s) => s.to_text(),
            Object::Stream(s) => match s.decoded_within(1_048_576) {
                Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
                Err(_) => continue,
            },
            _ => continue,
        };
        out.push((name.into(), source));
    }
    out
}

/// The changes a script made, against the fields as they were.
fn changes(before: &[pdfcraft_js::FieldState], after: &[pdfcraft_js::FieldState]) -> Vec<FieldChange> {
    if after.is_empty() {
        return Vec::new();
    }
    // Outcomes contain only fields touched by the script. Index the original snapshot once,
    // keeping the first match as the old linear lookup did for malformed duplicate names.
    let mut by_name = std::collections::HashMap::with_capacity(before.len());
    for field in before {
        by_name.entry(field.name.as_str()).or_insert(field);
    }
    after
        .iter()
        .filter_map(|a| {
            let b = by_name.get(a.name.as_str())?;
            let c = FieldChange {
                name: a.name.clone(),
                value: (a.value != b.value).then(|| a.value.clone()),
                read_only: (a.readonly != b.readonly).then_some(a.readonly),
                required: (a.required != b.required).then_some(a.required),
                display: (a.display != b.display).then_some(a.display),
                options: (a.options != b.options).then(|| a.options.clone()),
                stroke_color: (a.stroke_color != b.stroke_color).then(|| a.stroke_color.clone()).flatten(),
                actions: a.actions.iter().filter(|action| !b.actions.contains(action)).cloned().collect(),
            };
            (c.value.is_some()
                || c.read_only.is_some()
                || c.required.is_some()
                || c.display.is_some()
                || c.options.is_some()
                || c.stroke_color.is_some()
                || !c.actions.is_empty())
            .then_some(c)
        })
        .collect()
}

/// Runs field scripts with the document's object model and keeps what they print or ask for.
pub struct JsRunner {
    pub doc: pdfcraft_js::DocInfo,
    pub doc_scripts: Vec<String>,
    pub output: JsOutput,
    /// Immutable document snapshot from the start of the edit (for widget `display`).
    cos: pdfcraft_cos::Document,
    // `cos` is the same immutable pre-edit snapshot for the runner's lifetime. Cache only
    // its scalar display readings, not COS objects: large forms exceed the object cache and
    // otherwise reparse every widget for every calculation. Values/flags still come from
    // the live `fields` snapshot on each call. A new edit gets a new runner and cache.
    displays: std::collections::HashMap<pdfcraft_cos::ObjRef, i32>,
    runtime: Option<pdfcraft_js::Runtime>,
    pub(crate) allow_structure: bool,
    metadata: std::collections::HashMap<pdfcraft_cos::ObjRef, pdfcraft_js::FieldState>,
}

impl JsRunner {
    pub fn with_runtime(cos: &pdfcraft_cos::Document, file_name: &str, runtime: pdfcraft_js::Runtime) -> Self {
        let mut runner = Self::new(cos, file_name);
        runner.runtime = Some(runtime);
        runner
    }

    /// A runner for `cos`: what scripts read about the document (pages, info, whether it is
    /// an XFA form) and its document-level scripts, read once.
    pub fn new(cos: &pdfcraft_cos::Document, file_name: &str) -> JsRunner {
        let xfa = pdfcraft_xfa::xfa_shape(cos).is_some();
        let doc =
            pdfcraft_js::DocInfo { file_name: file_name.to_string(), num_pages: pdfcraft_model::pages(cos).len(), page: 0, info: info(cos), xfa };
        // Once XFA owns the form, the PDF container's scripts belong to its fallback view.
        // Keep them in the file and in Document JavaScripts, but do not initialize an
        // AcroForm context with them: a viewer-compatibility alert would be misleading.
        // Packets alone are not enough: static XFA and an unlaid-out shell keep their scripts.
        let xfa_active = xfa && pdfcraft_xfa::existing_layout(cos).is_some();
        let doc_scripts = if xfa_active { Vec::new() } else { pdfcraft_forms::document_scripts(cos) };
        JsRunner {
            doc,
            doc_scripts,
            output: JsOutput::default(),
            cos: cos.clone(),
            displays: std::collections::HashMap::new(),
            runtime: None,
            allow_structure: true,
            metadata: std::collections::HashMap::new(),
        }
    }
}

/// The Info dictionary as `this.info` keys (lower-case).
fn info(cos: &pdfcraft_cos::Document) -> Vec<(String, String)> {
    let Some(d) = cos.trailer().get(b"Info").map(|i| cos.resolve(i)).and_then(|i| i.as_dict().cloned()) else { return Vec::new() };
    d.iter()
        .filter_map(|(k, v)| match &*cos.resolve(v) {
            pdfcraft_cos::Object::String(s) => Some((String::from_utf8_lossy(k).to_lowercase(), s.to_text())),
            _ => None,
        })
        .collect()
}

impl Scripts for JsRunner {
    fn run(&mut self, event: FieldEvent, script: &str, target: &Field, value: &str, fields: &[Field]) -> ScriptResult {
        let states: Vec<_> = fields
            .iter()
            .map(|f| {
                let display = match f.widgets.first() {
                    Some(w) => *self.displays.entry(w.obj).or_insert_with(|| display(&self.cos, f)),
                    None => pdfcraft_js::DISPLAY_VISIBLE,
                };
                let mut state = field_state_with_display(f, display);
                let metadata = self.metadata.entry(f.obj).or_insert_with(|| field_state(&self.cos, f));
                state.stroke_color.clone_from(&metadata.stroke_color);
                state.actions.clone_from(&metadata.actions);
                state
            })
            .collect();
        let states = std::sync::Arc::new(states);
        let ev = pdfcraft_js::Event::field(event.name(), &target.name, value);
        let o = match &self.runtime {
            Some(runtime) => runtime.run_shared(script, &ev, &self.doc, std::sync::Arc::clone(&states), &self.doc_scripts, Limits::default()),
            None => pdfcraft_js::run_shared(script, &ev, &self.doc, std::sync::Arc::clone(&states), &self.doc_scripts, Limits::default()),
        };
        self.output.absorb(&o);
        if o.error.is_some() {
            // As in Acrobat, a failing script leaves the value alone (the error goes to the console).
            return ScriptResult { rc: true, value: value.to_string(), changes: Vec::new(), message: None };
        }
        let changes = changes(&states, &o.changed);
        if !self.allow_structure && structural_changes(&changes) {
            self.output
                .errors
                .push("The form script tried to change field definitions in a certified or restricted PDF; those changes were refused.".into());
            return ScriptResult { rc: true, value: value.into(), changes: Vec::new(), message: None };
        }
        ScriptResult { rc: o.rc, value: o.value.clone(), changes, message: (!o.rc).then(|| o.alerts.last().cloned()).flatten() }
    }
}

/// A document-level JavaScript: its name and source.
pub fn document_scripts(cos: &pdfcraft_cos::Document) -> Vec<(String, String)> {
    pdfcraft_forms::document_scripts_named(cos)
}

impl crate::Document {
    /// Field Properties ▸ Actions: field `name`'s action per trigger.
    pub fn field_actions(&self, name: &str) -> Vec<(pdfcraft_forms::Trigger, pdfcraft_forms::FieldAction)> {
        self.editor.as_ref().and_then(|e| pdfcraft_forms::field_actions(&e.cos, name).ok()).unwrap_or_default()
    }

    /// Stored lifecycle scripts. Unsupported actions are preserved by the editor.
    pub fn document_actions(&self) -> Vec<(String, String)> {
        let Some(editor) = &self.editor else { return Vec::new() };
        DOCUMENT_EVENTS.iter().map(|event| ((*event).into(), lifecycle_scripts(&editor.cos, event).join("\n"))).collect()
    }

    /// Field Properties ▸ Actions: how many actions are chained after each trigger's first one.
    pub fn field_action_chains(&self, name: &str) -> Vec<(pdfcraft_forms::Trigger, usize)> {
        self.editor.as_ref().and_then(|e| pdfcraft_forms::field_action_chains(&e.cos, name).ok()).unwrap_or_default()
    }

    /// Document JavaScripts (name, source), in name order.
    pub fn document_scripts(&self) -> Vec<(String, String)> {
        self.editor.as_ref().map(|e| document_scripts(&e.cos)).unwrap_or_default()
    }
}

impl Session {
    /// Preferences ▸ JavaScript ▸ Enable Acrobat JavaScript (on by default).
    pub fn set_javascript(&mut self, on: bool) {
        self.js_off = !on;
    }

    pub fn javascript(&self) -> bool {
        !self.js_off
    }

    /// What scripts printed or asked for since the last call (alerts, console, print, …).
    pub fn take_js_output(&mut self, id: DocId) -> JsOutput {
        self.docs.iter_mut().find(|d| d.id == id).map(|d| std::mem::take(&mut d.js_output)).unwrap_or_default()
    }

    /// Run `script` in document `id`, for a button (`target`: the field, event "Mouse Up") or
    /// the console (no target). Field changes and `resetForm` are applied as one undoable step;
    /// everything else the script asked for is returned.
    pub fn run_javascript(&mut self, id: DocId, script: &str, target: Option<&str>) -> Result<Outcome, EditError> {
        if self.js_off {
            return Err(EditError::Invalid("JavaScript is turned off (Preferences ▸ JavaScript)".into()));
        }
        let doc = self.get(id).ok_or(EditError::NoDocument)?;
        let cos = doc.editor.as_ref().map(|e| e.cos.clone()).ok_or_else(|| EditError::ReadOnly(doc.read_only_reason.clone().unwrap_or_default()))?;
        // A button PdfCraft generated from an XFA template runs its XFA click script.
        if let Some(t) = target
            && let Some(tpl) = doc.xfa_template.clone()
            && let Some(field) = doc.form.iter().find(|f| f.name == t)
            && let Some(som) = crate::xfa::clickable_som(&cos, &tpl, field)
        {
            self.apply(id, Edit::XfaEvent { som, activity: "click".into() })?;
            let out = self.take_js_output(id);
            return Ok(Outcome {
                rc: true,
                alerts: out.alerts,
                console: out.console,
                requests: out.requests,
                error: out.errors.first().cloned(),
                ..Default::default()
            });
        }
        let event = match target {
            Some(t) => pdfcraft_js::Event { will_commit: false, ..pdfcraft_js::Event::field("Mouse Up", t, "") },
            None => pdfcraft_js::Event::doc("Console"),
        };
        self.run_js_event(id, script, event)
    }

    fn run_js_event(&mut self, id: DocId, script: &str, event: pdfcraft_js::Event) -> Result<Outcome, EditError> {
        let runtime = self.js_runtime(id)?;
        let doc = self.get(id).ok_or(EditError::NoDocument)?;
        let cos = doc.editor.as_ref().map(|e| e.cos.clone()).ok_or_else(|| EditError::ReadOnly(doc.read_only_reason.clone().unwrap_or_default()))?;
        let runner = JsRunner::new(&cos, &doc.name);
        let fields = pdfcraft_forms::fields(&cos);
        let states: Vec<_> = fields.iter().map(|f| field_state(&cos, f)).collect();
        let mut o = runtime.run(script, &event, &runner.doc, &states, &runner.doc_scripts, Limits::default());
        if o.error.is_some() {
            return Ok(o);
        }
        let changes = changes(&states, &o.changed);
        let resets: Vec<Vec<String>> = o
            .requests
            .iter()
            .filter_map(|r| match r {
                Request::Reset(n) => Some(n.clone()),
                _ => None,
            })
            .collect();
        o.requests.retain(|r| !matches!(r, Request::Reset(_)));
        let mut edits = Vec::new();
        if !changes.is_empty() {
            edits.push(Edit::ApplyScriptChanges { changes });
        }
        for names in resets {
            let all: Vec<String> = fields.iter().map(|f| f.name.clone()).collect();
            let names = (!names.is_empty())
                .then(|| all.iter().filter(|n| names.iter().any(|x| *n == x || n.starts_with(&format!("{x}.")))).cloned().collect());
            edits.push(Edit::ResetForm { names });
        }
        if !edits.is_empty() {
            self.apply(id, Edit::Batch { label: "Run JavaScript".into(), edits })?;
        }
        Ok(o)
    }

    pub(crate) fn js_runtime(&mut self, id: DocId) -> Result<pdfcraft_js::Runtime, EditError> {
        let doc = self.doc_mut(id)?;
        if doc.js_runtime.is_none() {
            doc.js_runtime = Some(pdfcraft_js::Runtime::new().map_err(EditError::Invalid)?);
        }
        doc.js_runtime.clone().ok_or_else(|| EditError::Invalid("the script engine could not start".into()))
    }

    /// Dispatch a stored widget JavaScript action, using the same realm as document scripts.
    pub fn run_field_event(&mut self, id: DocId, field: &str, trigger: pdfcraft_forms::Trigger) -> Result<JsOutput, EditError> {
        if self.js_off {
            return Ok(JsOutput::default());
        }
        let doc = self.get(id).ok_or(EditError::NoDocument)?;
        let cos = doc.editor.as_ref().map(|e| &e.cos).ok_or_else(|| EditError::ReadOnly(doc.read_only_reason.clone().unwrap_or_default()))?;
        let target =
            pdfcraft_forms::fields(cos).into_iter().find(|f| f.name == field).ok_or_else(|| EditError::Invalid(format!("no field named {field}")))?;
        let scripts = pdfcraft_forms::field_actions(cos, field)?;
        let mut output = JsOutput::default();
        for (t, action) in scripts {
            if t == trigger
                && let pdfcraft_forms::FieldAction::JavaScript(script) = action
            {
                let name = match trigger {
                    pdfcraft_forms::Trigger::OnFocus => "Focus",
                    pdfcraft_forms::Trigger::OnBlur => "Blur",
                    _ => trigger.label(),
                };
                let event = pdfcraft_js::Event { will_commit: false, ..pdfcraft_js::Event::field(name, field, &target.value.join(",")) };
                let out = self.run_js_event(id, &script, event)?;
                output.absorb(&out);
            }
        }
        Ok(output)
    }

    /// Document lifecycle actions share the same realm as field and console scripts.
    pub fn run_document_event(&mut self, id: DocId, name: &str) -> Result<(), EditError> {
        if self.js_off {
            return Ok(());
        }
        let doc = self.get(id).ok_or(EditError::NoDocument)?;
        let Some(cos) = doc.editor.as_ref().map(|e| &e.cos) else { return Ok(()) };
        let scripts = lifecycle_scripts(cos, name);
        if scripts.is_empty() && (name != "Open" || pdfcraft_forms::document_scripts(cos).is_empty()) {
            return Ok(());
        }
        // Even with no OpenAction, evaluate document scripts once on opening.
        let scripts = if scripts.is_empty() { vec![String::new()] } else { scripts };
        for script in scripts {
            let out = self.run_js_event(id, &script, pdfcraft_js::Event::doc(name))?;
            self.doc_mut(id)?.js_output.absorb(&out);
            if let Some(error) = out.error
                && name == "WillSave"
            {
                return Err(EditError::Invalid(format!("Will Save script failed: {error}")));
            }
        }
        Ok(())
    }
}

fn lifecycle_scripts(cos: &pdfcraft_cos::Document, name: &str) -> Vec<String> {
    use pdfcraft_cos::Object;
    let Some(root) = cos.trailer().get(b"Root").map(|o| cos.resolve(o)) else { return Vec::new() };
    let Some(catalog) = root.as_dict() else { return Vec::new() };
    let action = if name == "Open" {
        catalog.get(b"OpenAction").cloned()
    } else {
        let key: &[u8] = match name {
            "WillSave" => b"WS",
            "DidSave" => b"DS",
            "WillPrint" => b"WP",
            "DidPrint" => b"DP",
            _ => return Vec::new(),
        };
        catalog.get(b"AA").map(|a| cos.resolve(a)).and_then(|a| a.as_dict().and_then(|d| d.get(key)).cloned())
    };
    let mut pending: Vec<_> = action.into_iter().collect();
    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    let mut count = 0;
    while let Some(action) = pending.pop() {
        count += 1;
        if count > 256 {
            break;
        }
        if let Some(r) = action.as_ref()
            && !seen.insert(r)
        {
            continue;
        }
        let action = cos.resolve(&action);
        if let Some(array) = action.as_array() {
            pending.extend(array.iter().rev().take(256).cloned());
            continue;
        }
        let Some(d) = action.as_dict() else { continue };
        if d.name(b"S") == Some(b"JavaScript")
            && let Some(js) = d.get(b"JS")
        {
            match &*cos.resolve(js) {
                Object::String(s) => out.push(s.to_text()),
                Object::Stream(s) => {
                    if let Ok(bytes) = s.decoded_within(1_048_576) {
                        out.push(String::from_utf8_lossy(&bytes).into_owned());
                    }
                }
                _ => {}
            }
        }
        if let Some(next) = d.get(b"Next") {
            pending.push(next.clone());
        }
    }
    out
}

/// A page's words for form-field detection, in user space (underscore runs split from the
/// text around them).
fn detection_words(text: &pdfcraft_render::PageText, info: &pdfcraft_render::PageInfo) -> Vec<pdfcraft_forms::detect::Word> {
    page_words(text, info).into_iter().map(|(text, rect)| pdfcraft_forms::detect::Word { text, rect }).collect()
}

/// A page's words in reading order, in user space ([x0, y0, x1, y1]); runs of underscores are
/// words of their own.
pub(crate) fn page_words(text: &pdfcraft_render::PageText, info: &pdfcraft_render::PageInfo) -> Vec<(String, [f64; 4])> {
    words_with(text, |x, y| info.view_to_user(x, y))
        .into_iter()
        .map(|(text, r)| (text, [f64::from(r[0]), f64::from(r[1]), f64::from(r[2]), f64::from(r[3])]))
        .collect()
}

#[cfg(test)]
thread_local! {
    /// Characters read to tell underscore runs from other words.
    static UNDERSCORE_CHARS_READ: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// A page's words in reading order, their boxes mapped from view space by `map` (and ordered
/// again, so that [x0, y0, x1, y1] holds whatever way `map` turns them).
pub(crate) fn words_with(text: &pdfcraft_render::PageText, map: impl Fn(f32, f32) -> [f32; 2]) -> Vec<(String, [f32; 4])> {
    let mut words: Vec<(String, [f32; 4])> = Vec::new();
    let mut last_line = u32::MAX;
    // A space glyph ends the word before it.
    let mut gap = false;
    // Whether the last word is an underscore run (a form's blank), which other glyphs don't join.
    let mut last_under = false;
    for (i, g) in text.glyphs.iter().enumerate() {
        let line = text.line_of.get(i).copied().unwrap_or(0);
        if g.text.trim().is_empty() {
            gap = true;
            continue;
        }
        let under = g.text.chars().all(|c| c == '_');
        #[cfg(test)]
        UNDERSCORE_CHARS_READ.with(|n| n.set(n.get() + g.text.len()));
        let breaks = words.is_empty() || gap || line != last_line || text.space_before.get(i).copied().unwrap_or(false) || last_under != under;
        gap = false;
        last_line = line;
        match words.last_mut() {
            Some((w, r)) if !breaks => {
                w.push_str(&g.text);
                r[0] = r[0].min(g.rect[0]);
                r[1] = r[1].min(g.rect[1]);
                r[2] = r[2].max(g.rect[2]);
                r[3] = r[3].max(g.rect[3]);
            }
            _ => {
                words.push((g.text.clone(), g.rect));
                last_under = under;
            }
        }
    }
    words
        .into_iter()
        .map(|(text, r)| {
            let a = map(r[0], r[1]);
            let b = map(r[2], r[3]);
            (text, [a[0].min(b[0]), a[1].min(b[1]), a[0].max(b[0]), a[1].max(b[1])])
        })
        .collect()
}

impl Session {
    /// Prepare a form ▸ automatic field detection: the fields `pages` (0-based; empty = all)
    /// seem to ask for, named from their labels, as (page, candidate). Scanned pages (a picture
    /// and no drawn shapes) are read from their pixels, see [`crate::scan_fields`].
    pub fn detect_fields(&self, id: DocId, pages: &[usize]) -> Vec<(usize, pdfcraft_forms::detect::Candidate)> {
        self.detect_fields_turned(id, pages).into_iter().map(|(page, c, _)| (page, c)).collect()
    }

    /// [`Self::detect_fields`], with how far each field turns (`/MK /R`) to read upright.
    fn detect_fields_turned(&self, id: DocId, pages: &[usize]) -> Vec<(usize, pdfcraft_forms::detect::Candidate, i64)> {
        let Some(doc) = self.get(id) else { return Vec::new() };
        let Some(cos) = doc.editor.as_ref().map(|e| &e.cos) else { return Vec::new() };
        let config = pdfcraft_render::RenderConfig { password: doc.password.as_deref().map(std::sync::Arc::from), ..Default::default() };
        let mut r = pdfcraft_render::PageRenderer::new(doc.bytes.clone(), config);
        let mut taken: Vec<String> = doc.form.iter().map(|f| f.name.clone()).collect();
        let all = doc.info.pages.len();
        let list: Vec<usize> = if pages.is_empty() { (0..all).collect() } else { pages.iter().copied().filter(|p| *p < all).collect() };
        let mut out = Vec::new();
        for page in list {
            let Some(info) = doc.info.pages.get(page) else { continue };
            let res = r.render(pdfcraft_render::RenderRequest { page, kind: pdfcraft_render::RequestKind::Text, scale: 1.0, ..Default::default() });
            let shapes = pdfcraft_forms::detect::page_shapes(cos, page);
            let existing: Vec<[f64; 4]> = doc.form.iter().flat_map(|f| f.widgets.iter()).filter(|w| w.page == Some(page)).map(|w| w.rect).collect();
            let (found, turn) = if shapes.boxes.is_empty() && shapes.rules.is_empty() && crate::scan_fields::is_scan(doc, page, info) {
                self.detect_on_scan(id, &mut r, page, res.text.as_deref(), &existing, &taken)
            } else {
                let words = res.text.map(|t| detection_words(&t, info)).unwrap_or_default();
                (pdfcraft_forms::detect::detect(&words, &shapes, &existing, &taken), 0)
            };
            for c in found {
                taken.push(c.name.clone());
                out.push((page, c, turn));
            }
        }
        out
    }

    /// Detect fields and add them, as one undoable step. Returns the new fields' names.
    pub fn auto_detect_fields(&mut self, id: DocId, pages: &[usize]) -> Result<Vec<String>, EditError> {
        let found = self.detect_fields_turned(id, pages);
        if found.is_empty() {
            return Ok(Vec::new());
        }
        let names = found.iter().map(|(_, c, _)| c.name.clone()).collect();
        let mut edits = Vec::with_capacity(found.len());
        for (page, c, turn) in found {
            let tall = if turn % 180 == 0 { c.rect[3] - c.rect[1] } else { c.rect[2] - c.rect[0] };
            let (kind, comb) = match c.kind {
                pdfcraft_forms::detect::Kind::Text => (pdfcraft_forms::NewField::Text { multiline: tall > 30.0 }, None),
                pdfcraft_forms::detect::Kind::CheckBox => (pdfcraft_forms::NewField::CheckBox, None),
                pdfcraft_forms::detect::Kind::Comb { cells } => (pdfcraft_forms::NewField::Text { multiline: false }, Some(cells)),
            };
            // The page already draws the line or box the field sits on, so the field gets no
            // border or fill of its own: an opaque fill would hide the printed form (#294, #2831).
            let mut props = FieldProps {
                appearance: Some(pdfcraft_forms::LookPatch { border: Some(None), fill: Some(None), ..Default::default() }),
                ..Default::default()
            };
            if turn != 0 {
                // A turned field keeps its place: the rectangle is already the one on the page.
                props.rotation = Some((0, turn));
                props.rect = Some((0, c.rect));
            }
            if let Some(cells) = comb {
                props.max_len = Some(Some(cells));
                props.flags = vec![(pdfcraft_forms::flags::COMB, true)];
            }
            edits.push(Edit::AddField { page, rect: c.rect, kind, name: Some(c.name.clone()) });
            edits.push(Edit::SetFieldProps { name: c.name, props: Box::new(props) });
        }
        self.apply(id, Edit::Batch { label: "Detect form fields".into(), edits })?;
        Ok(names)
    }
}

#[cfg(test)]
mod review_tests {
    use super::*;

    /// A long run of underscores (a form's blank) is read in one pass: each glyph is compared with
    /// the word so far without reading the word again.
    #[test]
    fn a_long_underscore_run_is_read_in_one_pass() {
        let n = 10_000;
        let glyph = |i: usize, t: &str| pdfcraft_render::TextGlyph {
            text: t.into(),
            rect: [i as f32, 0.0, i as f32 + 1.0, 10.0],
            quad: [[0.0; 2]; 4],
            direction: [1.0, 0.0],
        };
        let mut glyphs: Vec<_> = (0..n).map(|i| glyph(i, "_")).collect();
        glyphs.push(glyph(n, "x"));
        let text = pdfcraft_render::PageText { line_of: vec![0; glyphs.len()], space_before: vec![false; glyphs.len()], glyphs };
        UNDERSCORE_CHARS_READ.with(|c| c.set(0));
        let words = words_with(&text, |x, y| [x, y]);
        assert_eq!(words.iter().map(|(w, _)| w.len()).collect::<Vec<_>>(), [n, 1]);
        let read = UNDERSCORE_CHARS_READ.with(std::cell::Cell::get);
        assert!(read <= 4 * n, "{read} characters read for {n} glyphs");
    }

    /// Review on #3985: merging two outputs reports a failing document script once, as one run
    /// does, while each field script error stays: told apart by where an error came from, so a
    /// field script that throws a document script's very words is still reported.
    #[test]
    fn merged_output_reports_a_document_script_error_once() {
        let doc = "document script 2: TypeError: not a callable function";
        let field = "ReferenceError: x is not defined";
        let run = |error: &str| {
            let mut out = JsOutput::default();
            out.absorb(&Outcome { error: Some(error.into()), doc_errors: vec![doc.into()], ..Outcome::default() });
            out
        };
        let mut out = run(field);
        out.append(run(field));
        assert_eq!(out.errors, [field, doc, field]);
        // CodeRabbit on #3985: a field script that throws the document script's text.
        out.append(run(doc));
        assert_eq!(out.errors, [field, doc, field, doc]);
        let alone = run(doc);
        assert_eq!(alone.errors, [doc, doc], "the field's error and the document script's");
        let count = |o: &JsOutput| o.errors.iter().filter(|e| *e == doc).count();
        let mut merged = run(field);
        merged.append(alone);
        assert_eq!(count(&merged), 2, "the field's copy stays; the document script's is listed once");
    }

    #[test]
    fn duplicate_field_names_diff_against_first_original() {
        let first = pdfcraft_js::FieldState::new("duplicate", pdfcraft_js::FieldType::Text, vec!["first".into()]);
        let mut second = first.clone();
        second.value = vec!["second".into()];
        second.display = pdfcraft_js::DISPLAY_HIDDEN;
        let before = vec![first.clone(), second.clone()];
        assert!(changes(&before, std::slice::from_ref(&first)).is_empty());
        let actual = changes(&before, std::slice::from_ref(&second));
        assert_eq!(
            actual,
            vec![FieldChange {
                name: "duplicate".into(),
                value: Some(second.value),
                display: Some(pdfcraft_js::DISPLAY_HIDDEN),
                ..Default::default()
            }]
        );
        let outcome = pdfcraft_js::run_shared(
            "event.value = getField('duplicate').value; getField('duplicate').value = 'updated';",
            &pdfcraft_js::Event::field("Calculate", "duplicate", ""),
            &pdfcraft_js::DocInfo::default(),
            std::sync::Arc::new(before.clone()),
            &[],
            Limits::default(),
        );
        assert_eq!(outcome.error, None);
        assert_eq!(outcome.value, "first");
        assert_eq!(outcome.changed.len(), 1);
        assert_eq!(
            changes(&before, &outcome.changed),
            vec![FieldChange { name: "duplicate".into(), value: Some(vec!["updated".into()]), ..Default::default() }]
        );
        assert!(changes(&before, &[]).is_empty());
        let mut missing = first;
        missing.name = "missing".into();
        assert!(changes(&before, &[missing]).is_empty());
    }
}

pub const DOCUMENT_EVENTS: [&str; 3] = ["Open", "WillSave", "WillPrint"];

pub(crate) fn set_document_action(cos: &mut pdfcraft_cos::Document, event: &str, script: Option<&str>) -> Result<(), EditError> {
    use pdfcraft_cos::{Dict, Object, PdfString};
    let key: &[u8] = match event {
        "Open" => b"OpenAction",
        "WillSave" => b"WS",
        "WillPrint" => b"WP",
        _ => return Err(EditError::Invalid("expected Open, WillSave or WillPrint".into())),
    };
    if script.is_some_and(|s| s.len() > 1_048_576) {
        return Err(EditError::Invalid("document action exceeds 1 MiB".into()));
    }
    let root = cos.root().ok_or_else(|| EditError::Invalid("missing document catalog".into()))?;
    let catalog = cos.get(root);
    let mut parent = if event == "Open" {
        catalog.as_dict().cloned().unwrap_or_default()
    } else {
        catalog.as_dict().and_then(|d| d.get(b"AA")).map(|o| cos.resolve(o)).and_then(|o| o.as_dict().cloned()).unwrap_or_default()
    };
    if let Some(old) = parent.get(key) {
        let old = cos.resolve(old);
        if old.as_dict().is_none_or(|d| d.name(b"S") != Some(b"JavaScript") || d.contains(b"Next")) {
            return Err(EditError::Invalid("this event already has a destination or an action chain; it was preserved".into()));
        }
    }
    match script {
        Some(source) => {
            let mut action = Dict::new();
            action.set(b"S".to_vec(), Object::name("JavaScript"));
            action.set(b"JS".to_vec(), PdfString::text(source));
            parent.set(key.to_vec(), Object::Dict(action));
        }
        None => {
            parent.remove(key);
        }
    }
    if event == "Open" {
        cos.update_dict(root, |d| *d = parent).map_err(|e| EditError::Invalid(e.to_string()))?;
    } else {
        cos.update_dict(root, |d| d.set(b"AA".to_vec(), Object::Dict(parent))).map_err(|e| EditError::Invalid(e.to_string()))?;
    }
    Ok(())
}

pub(crate) fn structural_changes(changes: &[FieldChange]) -> bool {
    changes.iter().any(|c| c.options.is_some() || c.stroke_color.is_some() || !c.actions.is_empty())
}

#[cfg(test)]
mod lifecycle_tests {
    use super::*;
    use std::sync::Arc;
    #[test]
    fn save_errors_disable_switch_and_print_hooks_preserve_document_rules() {
        let mut session = Session::new();
        let id = session.open("actions.pdf", None, Arc::new(crate::tests::fixture(1)), None).unwrap();
        session.apply(id, Edit::SetDocumentAction { event: "WillPrint".into(), script: Some("console.println('before print');".into()) }).unwrap();
        session.print_pdf(id, &crate::print::Settings { pages: vec![0], ..Default::default() }).unwrap();
        assert_eq!(session.take_js_output(id).console, ["before print"]);
        session.apply(id, Edit::SetDocumentAction { event: "WillSave".into(), script: Some("throw new Error('cannot save');".into()) }).unwrap();
        let generation = session.get(id).unwrap().edit_generation();
        assert!(session.save_bytes(id).unwrap_err().to_string().contains("cannot save"));
        assert_eq!(session.get(id).unwrap().edit_generation(), generation);
        session.set_javascript(false);
        assert!(session.save_bytes(id).is_ok());
        session.set_javascript(true);
        session.apply(id, Edit::SetDocumentAction { event: "WillSave".into(), script: None }).unwrap();
        session.apply(id, Edit::SetPageLocks { pages: vec![0], locked: true }).unwrap();
        assert!(session.run_javascript(id, "app.launchURL('https://example.com');", None).unwrap().requests.len() == 1);
        assert!(session.get(id).unwrap().protected_pages().contains(&0));
    }
    #[test]
    fn action_editor_refuses_to_destroy_destinations_or_action_chains() {
        use pdfcraft_cos::{Dict, Object};
        let mut cos = pdfcraft_cos::Document::open(Arc::new(crate::tests::fixture(1))).unwrap();
        let root = cos.root().unwrap();
        cos.update_dict(root, |d| d.set(b"OpenAction".to_vec(), Object::Array(vec![Object::Int(0), Object::name("Fit")]))).unwrap();
        assert!(set_document_action(&mut cos, "Open", Some("1")).is_err());
        assert!(set_document_action(&mut cos, "Open", None).is_err());
        let mut action = Dict::new();
        action.set(b"S".to_vec(), Object::name("JavaScript"));
        action.set(b"JS".to_vec(), pdfcraft_cos::PdfString::text("1"));
        action.set(b"Next".to_vec(), Object::Null);
        cos.update_dict(root, |d| d.set(b"OpenAction".to_vec(), Object::Dict(action))).unwrap();
        assert!(set_document_action(&mut cos, "Open", Some("2")).is_err());
    }
}
