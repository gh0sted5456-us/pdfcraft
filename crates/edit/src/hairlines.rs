//! Fix hairlines (Acrobat's Print production ▸ Fix Hairlines; Fix hairlines): strokes thinner
//! than a printer holds - zero-width lines, and lines whose width on the page, after every
//! transformation, is below a threshold - are given a chosen width. Page content and the Form
//! XObjects it draws are rewritten; Type 3 glyphs and tiling patterns on request.
//!
//! Changed forms, glyphs and patterns are copied into the selected pages' resources. Their
//! original objects stay intact even when an unknown object graph also references them.

use pdfcraft_content::{Op, parse, serialize_ops};
use pdfcraft_cos::{Dict, Document, Name, ObjRef, Object, Stream};
use std::collections::{HashMap, HashSet};

use crate::page_streams::PageStreams;
use crate::{EditError, check, page_list};

/// The stream `r` holds, if it is one.
fn stream_of(doc: &Document, r: ObjRef) -> Option<Stream> {
    match &*doc.get(r) {
        Object::Stream(s) => Some(s.clone()),
        _ => None,
    }
}

/// What counts as a hairline and what it becomes, in points on the page.
#[derive(Clone, Debug, PartialEq)]
pub struct Hairlines {
    /// Lines narrower than this (points, after the page's transformations) are replaced;
    /// zero-width lines always are. Acrobat's default is 0.003 in.
    pub narrower_than: f64,
    /// The width they are given (points on the page). Acrobat's default is 0.01 in.
    pub width: f64,
    /// Also the strokes inside Type 3 font glyphs.
    pub type3: bool,
    /// Also the strokes inside tiling patterns.
    pub patterns: bool,
}

impl Default for Hairlines {
    fn default() -> Self {
        Self { narrower_than: 0.003 * 72.0, width: 0.01 * 72.0, type3: false, patterns: false }
    }
}

/// What a run changed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Fixed {
    /// Strokes given a new width.
    pub lines: usize,
    /// Pages on which something changed.
    pub pages: usize,
}

/// The uniform scale a matrix applies to lengths (the geometric mean of its two axes).
fn scale_of(m: [f64; 6]) -> f64 {
    // Normalize first so tiny and large finite transforms keep their scale.
    let largest = m.into_iter().take(4).map(f64::abs).fold(0.0, f64::max);
    if largest == 0.0 {
        return 0.0;
    }
    ((m[0] / largest) * (m[3] / largest) - (m[1] / largest) * (m[2] / largest)).abs().sqrt() * largest
}

fn mul(a: [f64; 6], b: [f64; 6]) -> [f64; 6] {
    // a then b.
    [
        a[0] * b[0] + a[1] * b[2],
        a[0] * b[1] + a[1] * b[3],
        a[2] * b[0] + a[3] * b[2],
        a[2] * b[1] + a[3] * b[3],
        a[4] * b[0] + a[5] * b[2] + b[4],
        a[4] * b[1] + a[5] * b[3] + b[5],
    ]
}

/// The graphics state a stream leaves for the next one on the same page: CTM and line width,
/// with the q/Q stack, and whether a path is still being built.
#[derive(Clone, Debug)]
struct State {
    ctm: [f64; 6],
    width: f64,
    stack: Vec<([f64; 6], f64)>,
    in_path: bool,
}

impl State {
    /// A stream drawn under `base` with line width `width` in effect.
    fn new(base: [f64; 6], width: f64) -> Self {
        Self { ctm: base, width, stack: Vec::new(), in_path: false }
    }
}

/// All a form's rewrite depends on: the scale it is drawn under and the line width in effect
/// at its `Do` (as bits, so it can be a map key).
type Key = (u64, u64);

fn key_of(ctm: [f64; 6], width: f64) -> Key {
    (scale_of(ctm).to_bits(), width.to_bits())
}

/// A Form XObject a stream draws: the resource name it is drawn by, the object, and the CTM
/// and line width at its `Do`.
struct Drawn {
    name: Name,
    r: ObjRef,
    ctm: [f64; 6],
    width: f64,
}

/// The names a stream's `Do`s are given instead, by the name they had and the key they are
/// drawn under.
type Renames = HashMap<(Name, Key), Name>;

/// How deep forms drawing forms are followed, and how many form rewrites one run makes; past
/// either, the whole edit is refused.
const MAX_DEPTH: usize = 32;
const MAX_FORMS: usize = 4096;
const MAX_OPS: usize = 1_000_000;
const MAX_RESOURCE_READS: usize = 100_000;

/// The width `/name gs` sets, when its ExtGState has an `/LW`.
fn line_width_of(doc: &Document, gstates: Option<&Dict>, name: &[u8]) -> Option<f64> {
    let gs = doc.resolve(gstates?.get(name)?);
    let lw = gs.as_dict()?.get(b"LW")?;
    doc.resolve(lw).as_f64().filter(|w| w.is_finite())
}

/// Rewrite one content stream drawn under `state` (the CTM when it starts, scale only
/// matters), leaving in it the state at the stream's end. A thin stroke gets a `w` of its own
/// before its path begins (graphics-state operators are not allowed inside a path) and the
/// old width back after it is painted, so nothing else drawn changes width.
/// `forms` collects the Form XObjects it draws, with the CTM and width each is drawn under,
/// for the caller to rewrite in turn; a `Do` listed in `renames` draws the name given there.
/// Returns the new bytes when anything changed, and the count.
fn rewrite(
    data: &[u8],
    state: &mut State,
    h: &Hairlines,
    resources: Option<&Dict>,
    doc: &Document,
    forms: &mut Vec<Drawn>,
    renames: &Renames,
) -> Result<(Option<Vec<u8>>, usize), EditError> {
    let parsed = parse(data);
    if parsed.skipped != 0 || parsed.ambiguous_images != 0 || parsed.ops.len() > MAX_OPS {
        return Err(EditError::Invalid("hairline repair requires complete, bounded content streams".into()));
    }
    if !state.ctm.iter().all(|v| v.is_finite()) || !state.width.is_finite() || state.width < 0.0 {
        return Err(EditError::Invalid("invalid graphics state while repairing hairlines".into()));
    }
    let mut ops: Vec<Op> = parsed.ops;
    let mut fixed = 0usize;
    let mut changed = false;
    // Where in `out` the path being built began (None when it began in an earlier stream).
    let mut path_start: Option<usize> = None;
    let mut out: Vec<Op> = Vec::with_capacity(ops.len().saturating_add(8));
    let dict_of = |key: &[u8]| resources.and_then(|r| r.get(key)).map(|x| doc.resolve(x)).and_then(|x| x.as_dict().cloned());
    let xobjects = dict_of(&b"XObject"[..]);
    let gstates = dict_of(&b"ExtGState"[..]);
    for mut op in ops.drain(..) {
        match op.op.as_slice() {
            b"q" => {
                state.stack.push((state.ctm, state.width));
                if state.stack.len() > 512 {
                    return Err(EditError::Invalid("hairline repair supports at most 512 nested graphics states".into()));
                }
            }
            b"Q" => {
                if let Some((c, w)) = state.stack.pop() {
                    state.ctm = c;
                    state.width = w;
                }
            }
            b"cm" => {
                let m =
                    op.nums::<6>().ok_or_else(|| EditError::Invalid("a transform needs six numeric operands; hairlines were not changed".into()))?;
                state.ctm = mul(m, state.ctm);
            }
            b"w" => {
                let [w] =
                    op.nums::<1>().ok_or_else(|| EditError::Invalid("a line width needs one numeric operand; hairlines were not changed".into()))?;
                state.width = w;
            }
            b"gs" => {
                // An ExtGState's /LW sets the width as `w` does (so the width put back after
                // a fixed stroke is the right one).
                if let Some(Object::Name(name)) = op.operands.first()
                    && let Some(w) = line_width_of(doc, gstates.as_ref(), name)
                {
                    state.width = w;
                }
            }
            b"m" | b"re" => {
                if !state.in_path {
                    state.in_path = true;
                    path_start = Some(out.len());
                }
            }
            b"Do" => {
                if let Some(Object::Name(name)) = op.operands.first()
                    && let Some(x) = xobjects.as_ref().and_then(|x| x.get(name))
                    && let Some(r) = x.as_ref()
                    && matches!(&*doc.get(r), Object::Stream(s) if s.dict.name(b"Subtype") == Some(b"Form"))
                {
                    let name = name.clone();
                    if !renames.is_empty()
                        && let Some(to) = renames.get(&(name.clone(), key_of(state.ctm, state.width)))
                        && let Some(first) = op.operands.first_mut()
                    {
                        *first = Object::Name(to.clone());
                        changed = true;
                    }
                    forms.push(Drawn { name, r, ctm: state.ctm, width: state.width });
                }
            }
            b"S" | b"s" | b"B" | b"B*" | b"b" | b"b*" => {
                let scale = scale_of(state.ctm);
                if !scale.is_finite() || scale <= 0.0 {
                    return Err(EditError::Invalid("a stroke has an unrepresentable transform; hairlines were not changed".into()));
                }
                let on_page = state.width * scale;
                let thin = state.width <= 0.0 || (scale > 0.0 && on_page < h.narrower_than);
                // A path begun in an earlier stream cannot get its `w` before it here; it is
                // left as it is rather than given an operator inside the path.
                let at = match path_start {
                    Some(i) => Some(i),
                    None if !state.in_path => Some(out.len()),
                    None => None,
                };
                state.in_path = false;
                path_start = None;
                if thin
                    && scale > 0.0
                    && let Some(at) = at
                {
                    let new_w = h.width / scale;
                    if !new_w.is_finite() || new_w <= 0.0 {
                        return Err(EditError::Invalid("the requested stroke width cannot be represented under this transform".into()));
                    }
                    if (new_w - state.width).abs() > 1e-9 * new_w.abs().max(state.width.abs()) {
                        // `at` was this output's length when the path began; output only grows.
                        out.insert(at, Op::new("w", vec![Object::Real(new_w)]));
                        out.push(op);
                        out.push(Op::new("w", vec![Object::Real(state.width)]));
                        fixed += 1;
                        changed = true;
                        continue;
                    }
                }
            }
            b"f" | b"F" | b"f*" | b"n" => {
                state.in_path = false;
                path_start = None;
            }
            _ => {}
        }
        if !state.ctm.iter().all(|v| v.is_finite()) || !state.width.is_finite() || state.width < 0.0 {
            return Err(EditError::Invalid("invalid graphics state while repairing hairlines".into()));
        }
        out.push(op);
    }
    Ok((changed.then(|| serialize_ops(&out)), fixed))
}

/// Memoized repairs and bounded work for one atomic edit.
struct Run {
    done: HashMap<(ObjRef, Key), (ObjRef, usize)>,
    active: HashSet<ObjRef>,
    budget: usize,
    operators_left: usize,
    resource_copies: HashMap<(ObjRef, Key), (ObjRef, usize)>,
    resource_budget: usize,
    resource_reads: usize,
}

fn charge_ops(data: &[u8], remaining: &mut usize) -> Result<(), EditError> {
    let count = pdfcraft_content::ops(data).take(remaining.saturating_add(1)).count();
    *remaining =
        remaining.checked_sub(count).ok_or_else(|| EditError::Invalid("too much content for one hairline repair; select fewer pages".into()))?;
    Ok(())
}

/// A name for another copy drawn by `name` that `xobjects` does not hold yet.
fn fresh_name(xobjects: &Dict, name: &[u8]) -> Result<Name, EditError> {
    if name.len() > 4096 {
        return Err(EditError::Invalid("a form resource name is too long to repair safely".into()));
    }
    for i in 1..=xobjects.len().saturating_add(1) {
        let mut candidate = name.to_vec();
        candidate.extend_from_slice(format!("_hl{i}").as_bytes());
        if !xobjects.contains(&candidate) {
            return Ok(candidate);
        }
    }
    Err(EditError::Invalid("could not allocate a hairline-repair resource name".into()))
}

type ContextRepair = (Vec<Option<Vec<u8>>>, usize, Option<Dict>);

/// Rewrite one drawing context - a page's streams, or a form's - that starts in `start`, and
/// the forms it draws. Returns each stream's new bytes (`None` where it did not change), the
/// strokes fixed (in the forms too), and the `/XObject` dictionary for the context's resources
/// when its names must now point at other objects.
fn fix_context(
    doc: &mut Document,
    run: &mut Run,
    h: &Hairlines,
    data: &[Vec<u8>],
    start: &State,
    resources: Option<&Dict>,
    depth: usize,
) -> Result<ContextRepair, EditError> {
    let none = Renames::new();
    let mut drawn: Vec<Drawn> = Vec::new();
    let mut state = start.clone();
    let mut new: Vec<Option<Vec<u8>>> = Vec::with_capacity(data.len());
    let mut fixed = 0usize;
    for d in data {
        charge_ops(d, &mut run.operators_left)?;
        let (bytes, count) = rewrite(d, &mut state, h, resources, doc, &mut drawn, &none)?;
        fixed = fixed.checked_add(count).ok_or_else(|| EditError::Invalid("hairline count overflow".into()))?;
        new.push(bytes);
    }
    if drawn.is_empty() {
        return Ok((new, fixed, None));
    }
    let Some(xo) = resources.and_then(|r| r.get(b"XObject")).map(|x| doc.resolve(x)).and_then(|x| x.as_dict().cloned()) else {
        return Ok((new, fixed, None));
    };
    // The object each (name, key) is drawn as, and each name's objects in order of first use.
    let mut chosen: HashMap<(Name, Key), ObjRef> = HashMap::new();
    let mut targets: Vec<(Name, Vec<ObjRef>)> = Vec::new();
    for d in drawn {
        let k = key_of(d.ctm, d.width);
        if chosen.contains_key(&(d.name.clone(), k)) {
            continue;
        }
        let (to, count) = fix_form(doc, run, h, d.r, d.ctm, d.width, depth + 1)?;
        fixed = fixed.checked_add(count).ok_or_else(|| EditError::Invalid("hairline count overflow".into()))?;
        chosen.insert((d.name.clone(), k), to);
        match targets.iter_mut().find(|(name, _)| *name == d.name) {
            Some((_, list)) => {
                if !list.contains(&to) {
                    list.push(to);
                }
            }
            None => targets.push((d.name, vec![to])),
        }
    }
    // The first object keeps the name; each other one gets a name of its own, and the `Do`s
    // drawing it are renamed.
    let mut repointed = xo;
    let mut changed = false;
    let mut renames = Renames::new();
    for (name, list) in &targets {
        for (i, to) in list.iter().enumerate() {
            let at = if i == 0 { name.clone() } else { fresh_name(&repointed, name)? };
            if repointed.get(&at).and_then(Object::as_ref) != Some(*to) {
                repointed.set(at.clone(), Object::Ref(*to));
                changed = true;
            }
            if i > 0 {
                for ((was, k), t) in &chosen {
                    if was == name && t == to {
                        renames.insert((was.clone(), *k), at.clone());
                    }
                }
            }
        }
    }
    if !renames.is_empty() {
        // The same rewrite again, with the renamed `Do`s.
        let mut state = start.clone();
        let mut again: Vec<Drawn> = Vec::new();
        new = data.iter().map(|d| rewrite(d, &mut state, h, resources, &*doc, &mut again, &renames).map(|r| r.0)).collect::<Result<_, _>>()?;
    }
    Ok((new, fixed, changed.then_some(repointed)))
}

/// Repair a form for this use-context without changing any original resource object.
fn fix_form(
    doc: &mut Document,
    run: &mut Run,
    h: &Hairlines,
    r: ObjRef,
    ctm: [f64; 6],
    width: f64,
    depth: usize,
) -> Result<(ObjRef, usize), EditError> {
    let key = key_of(ctm, width);
    if let Some(&done) = run.done.get(&(r, key)) {
        return Ok(done);
    }
    if depth > MAX_DEPTH || run.budget == 0 || !run.active.insert(r) {
        return Err(EditError::Invalid("hairline repair encountered recursive forms or exceeded its form budget".into()));
    }
    run.budget -= 1;
    let stream = stream_of(doc, r).ok_or_else(|| EditError::Invalid("a form is not a readable stream".into()))?;
    let data = stream.decoded().map_err(|e| EditError::Invalid(format!("could not decode a form for hairline repair: {e}")))?;
    let base = mul(matrix_of(doc, &stream.dict, b"Matrix", [1.0, 0.0, 0.0, 1.0, 0.0, 0.0])?, ctm);
    let resources = stream.dict.get(b"Resources").map(|x| doc.resolve(x)).and_then(|x| x.as_dict().cloned());
    let (new, count, xobjects) = fix_context(doc, run, h, std::slice::from_ref(&data), &State::new(base, width), resources.as_ref(), depth)?;
    let bytes = new.into_iter().next().flatten();
    let to = if bytes.is_none() && xobjects.is_none() {
        r
    } else {
        let mut result = stream;
        if let Some(xobjects) = xobjects {
            let mut resources = resources.unwrap_or_default();
            resources.set(b"XObject".to_vec(), Object::Dict(xobjects));
            result.dict.set(b"Resources".to_vec(), Object::Dict(resources));
        }
        if let Some(bytes) = bytes {
            let mut dict = result.dict;
            for key in [&b"Length"[..], b"Filter", b"DecodeParms", b"DL"] {
                dict.remove(key);
            }
            result = Stream::flate(dict, &bytes);
        }
        doc.add(Object::Stream(result))
    };
    run.active.remove(&r);
    run.done.insert((r, key), (to, count));
    Ok((to, count))
}

/// Fix hairlines on 0-based pages, atomically. Changed forms and optional glyph/pattern
/// procedures are copied, with only the selected pages repointed. Glyphs use a nominal
/// 10-point scale; their nested Forms are not followed. Transformed width uses the geometric
/// mean of the matrix axes, matching the basic device-space repair model.
/// shortcut: optional glyphs use 10 pt and nested glyph/pattern Forms are not followed;
/// use-context traversal is needed before promising exact repair of those strokes.
pub fn fix_hairlines(doc: &mut Document, pages: &[usize], h: &Hairlines) -> Result<Fixed, EditError> {
    let mut next = doc.clone();
    let result = fix_hairlines_inner(&mut next, pages, h)?;
    *doc = next;
    Ok(result)
}

fn fix_hairlines_inner(doc: &mut Document, pages: &[usize], h: &Hairlines) -> Result<Fixed, EditError> {
    let all = page_list(doc);
    check(pages, all.len())?;
    if !(h.narrower_than.is_finite() && h.narrower_than >= 0.0 && h.width.is_finite() && h.width > 0.0) {
        return Err(EditError::Invalid("the hairline widths must be positive numbers of points".into()));
    }
    if pages.iter().copied().collect::<HashSet<_>>().len() != pages.len() {
        return Err(EditError::Invalid("choose each page only once for hairline repair".into()));
    }
    let mut result = Fixed::default();
    let mut run = Run {
        done: HashMap::new(),
        active: HashSet::new(),
        budget: MAX_FORMS,
        operators_left: MAX_OPS,
        resource_copies: HashMap::new(),
        resource_budget: MAX_FORMS,
        resource_reads: MAX_RESOURCE_READS,
    };
    for &i in pages {
        let page = all.get(i).cloned().ok_or(EditError::NoSuchPage(i))?;
        let resources = page.dict.get(b"Resources").map(|r| doc.resolve(r)).and_then(|r| r.as_dict().cloned());
        let mut page_fixed = 0usize;
        // The page's streams, each rewritten on its own (state carries across them), and the
        // forms they draw. Unreadable content refuses the whole atomic edit.
        {
            let streams = PageStreams::read(doc, &page.dict)
                .ok_or_else(|| EditError::Invalid(format!("page {} has unreadable content; hairlines were not changed", i.saturating_add(1))))?;
            let unit = page.user_unit(doc);
            let start = State::new([unit, 0.0, 0.0, unit, 0.0, 0.0], 1.0);
            let (new, fixed, xo) = fix_context(doc, &mut run, h, &streams.data, &start, resources.as_ref(), 0)?;
            page_fixed = page_fixed.checked_add(fixed).ok_or_else(|| EditError::Invalid("hairline count overflow".into()))?;
            streams.write(doc, page.obj, new)?;
            if let Some(xo) = xo {
                // This page's own resources point at the copies; a resource dictionary other
                // pages share is not changed.
                let mut res = resources.clone().unwrap_or_default();
                res.set(b"XObject".to_vec(), Object::Dict(xo));
                doc.update_dict(page.obj, |d| d.set(b"Resources".to_vec(), Object::Dict(res)))?;
            }
        }
        if h.type3 || h.patterns {
            let current = doc.get(page.obj);
            let resources = current.as_dict().and_then(|d| d.get(b"Resources")).or_else(|| page.dict.get(b"Resources")).map(|r| doc.resolve(r));
            let resources = resources.as_ref().and_then(|r| r.as_dict());
            let (next, count) = fix_in_resources(doc, resources, h, page.user_unit(doc), &mut run)?;
            page_fixed = page_fixed.checked_add(count).ok_or_else(|| EditError::Invalid("hairline count overflow".into()))?;
            if let Some(next) = next {
                doc.update_dict(page.obj, |d| d.set(b"Resources".to_vec(), Object::Dict(next)))?;
            }
        }
        if page_fixed > 0 {
            result.lines = result.lines.checked_add(page_fixed).ok_or_else(|| EditError::Invalid("hairline count overflow".into()))?;
            result.pages += 1;
        }
    }
    Ok(result)
}

/// Read a bounded finite matrix; missing matrices use their PDF-defined defaults.
fn matrix_of(doc: &Document, dict: &Dict, key: &[u8], default: [f64; 6]) -> Result<[f64; 6], EditError> {
    let Some(value) = dict.get(key) else {
        return Ok(default);
    };
    let value = doc.resolve(value);
    let array = value.as_array().ok_or_else(|| EditError::Invalid("a repair resource has an invalid matrix".into()))?;
    let [a, b, c, d, e, f] = array.as_slice() else {
        return Err(EditError::Invalid("a repair matrix must contain six finite numbers".into()));
    };
    let mut out = [0.0; 6];
    for (slot, value) in out.iter_mut().zip([a, b, c, d, e, f]) {
        *slot = doc
            .resolve(value)
            .as_f64()
            .filter(|v| v.is_finite())
            .ok_or_else(|| EditError::Invalid("a repair matrix must contain six finite numbers".into()))?;
    }
    Ok(out)
}

fn repaired_resource(doc: &mut Document, reference: ObjRef, base: [f64; 6], h: &Hairlines, run: &mut Run) -> Result<(ObjRef, usize), EditError> {
    run.resource_reads = run.resource_reads.checked_sub(1).ok_or_else(|| EditError::Invalid("too many glyph/pattern uses for one repair".into()))?;
    let key = (reference, key_of(base, 1.0));
    if let Some(&done) = run.resource_copies.get(&key) {
        return Ok(done);
    }
    if run.resource_budget == 0 {
        return Err(EditError::Invalid("too many glyphs or patterns for hairline repair".into()));
    }
    run.resource_budget -= 1;
    let stream = stream_of(doc, reference).ok_or_else(|| EditError::Invalid("a glyph or pattern is not a stream".into()))?;
    let bytes = stream.decoded().map_err(|e| EditError::Invalid(format!("could not decode a repair resource: {e}")))?;
    let resources = stream.dict.get(b"Resources").map(|r| doc.resolve(r)).and_then(|r| r.as_dict().cloned());
    charge_ops(&bytes, &mut run.operators_left)?;
    let (new, count) = rewrite(&bytes, &mut State::new(base, 1.0), h, resources.as_ref(), doc, &mut Vec::new(), &Renames::new())?;
    let to = if let Some(new) = new {
        let mut dict = stream.dict;
        for key in [&b"Length"[..], b"Filter", b"DecodeParms", b"DL"] {
            dict.remove(key);
        }
        doc.add(Object::Stream(Stream::flate(dict, &new)))
    } else {
        reference
    };
    run.resource_copies.insert(key, (to, count));
    Ok((to, count))
}

/// Type 3 glyphs use a nominal 10-point size; tiling patterns use their matrix. The Form
/// XObjects these procedures call are not followed. Copies keep unselected pages untouched.
fn fix_in_resources(
    doc: &mut Document,
    resources: Option<&Dict>,
    h: &Hairlines,
    unit: f64,
    run: &mut Run,
) -> Result<(Option<Dict>, usize), EditError> {
    let Some(resources) = resources else {
        return Ok((None, 0));
    };
    let mut next = resources.clone();
    let mut fixed = 0usize;
    let mut changed = false;
    if h.type3
        && let Some(fonts) = resources.get(b"Font").map(|f| doc.resolve(f)).and_then(|f| f.as_dict().cloned())
    {
        let mut next_fonts = fonts.clone();
        let mut fonts_changed = false;
        for (name, font) in fonts.iter() {
            run.resource_reads =
                run.resource_reads.checked_sub(1).ok_or_else(|| EditError::Invalid("too many font resources for one repair".into()))?;
            let resolved = doc.resolve(font);
            let Some(font) = resolved.as_dict().filter(|f| f.name(b"Subtype") == Some(b"Type3")) else {
                continue;
            };
            let base = mul(matrix_of(doc, font, b"FontMatrix", [0.001, 0.0, 0.0, 0.001, 0.0, 0.0])?, [10.0 * unit, 0.0, 0.0, 10.0 * unit, 0.0, 0.0]);
            let Some(procs) = font.get(b"CharProcs").map(|p| doc.resolve(p)).and_then(|p| p.as_dict().cloned()) else {
                continue;
            };
            if procs.len() > MAX_FORMS {
                return Err(EditError::Invalid("too many Type 3 procedures for hairline repair".into()));
            }
            let mut next_procs = procs.clone();
            let mut font_changed = false;
            for (name, procedure) in procs.iter() {
                let reference = procedure.as_ref().ok_or_else(|| EditError::Invalid("a Type 3 procedure must be an indirect stream".into()))?;
                let (to, count) = repaired_resource(doc, reference, base, h, run)?;
                fixed = fixed.checked_add(count).ok_or_else(|| EditError::Invalid("hairline count overflow".into()))?;
                if to != reference {
                    next_procs.set(name.clone(), Object::Ref(to));
                    font_changed = true;
                }
            }
            if font_changed {
                let mut font = font.clone();
                font.set(b"CharProcs".to_vec(), Object::Dict(next_procs));
                next_fonts.set(name.clone(), Object::Ref(doc.add(Object::Dict(font))));
                fonts_changed = true;
            }
        }
        if fonts_changed {
            next.set(b"Font".to_vec(), Object::Dict(next_fonts));
            changed = true;
        }
    }
    if h.patterns
        && let Some(patterns) = resources.get(b"Pattern").map(|p| doc.resolve(p)).and_then(|p| p.as_dict().cloned())
    {
        if patterns.len() > MAX_FORMS {
            return Err(EditError::Invalid("too many patterns for hairline repair".into()));
        }
        let mut next_patterns = patterns.clone();
        let mut patterns_changed = false;
        for (name, pattern) in patterns.iter() {
            run.resource_reads =
                run.resource_reads.checked_sub(1).ok_or_else(|| EditError::Invalid("too many pattern resources for one repair".into()))?;
            let Some(reference) = pattern.as_ref() else {
                continue;
            };
            let Some(stream) = stream_of(doc, reference).filter(|s| s.dict.int(b"PatternType") == Some(1)) else {
                continue;
            };
            let base = mul(matrix_of(doc, &stream.dict, b"Matrix", [1.0, 0.0, 0.0, 1.0, 0.0, 0.0])?, [unit, 0.0, 0.0, unit, 0.0, 0.0]);
            let (to, count) = repaired_resource(doc, reference, base, h, run)?;
            fixed = fixed.checked_add(count).ok_or_else(|| EditError::Invalid("hairline count overflow".into()))?;
            if to != reference {
                next_patterns.set(name.clone(), Object::Ref(to));
                patterns_changed = true;
            }
        }
        if patterns_changed {
            next.set(b"Pattern".to_vec(), Object::Dict(next_patterns));
            changed = true;
        }
    }
    Ok((changed.then_some(next), fixed))
}
