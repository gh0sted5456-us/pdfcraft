//! Bounded source-colour inventory; no colour conversion or simulated press separations.
use crate::{Document as OpenDocument, EditError};
use pdfcraft_cos::{Dict, Document, Object};
use std::collections::BTreeSet;
use std::sync::Arc;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct OutputInspection {
    pub page: usize,
    pub spaces: BTreeSet<String>,
    pub spots: BTreeSet<String>,
    pub overprint_declared: bool,
    /// Largest explicit k/K source colour sum, not composited total area coverage.
    pub max_source_cmyk_percent: Option<f64>,
    pub complete: bool,
    pub warnings: BTreeSet<String>,
}

impl OutputInspection {
    pub fn text(&self) -> String {
        format!(
            "Source output inspection — page {}\nColour spaces: {}\nSpot names: {}\nOverprint declared: {}\nMaximum explicit DeviceCMYK source sum: {}\nComplete within source scan: {}\n{}\nNot a soft proof: ICC rendering, separations, composited total ink coverage and transparency flattening are not simulated.\n",
            self.page + 1,
            self.spaces.iter().cloned().collect::<Vec<_>>().join(", "),
            self.spots.iter().cloned().collect::<Vec<_>>().join(", "),
            self.overprint_declared,
            self.max_source_cmyk_percent.map(|n| format!("{n:.1}%")).unwrap_or_else(|| "not measured".into()),
            self.complete,
            self.warnings.iter().cloned().collect::<Vec<_>>().join("\n")
        )
    }

    fn incomplete(&mut self, reason: &str) {
        self.complete = false;
        self.warnings.insert(reason.into());
    }
}

impl OpenDocument {
    pub fn output_inspection(&self, page: usize) -> Result<OutputInspection, EditError> {
        let editor = self.editor.as_ref().ok_or_else(|| EditError::ReadOnly("no object graph to inspect".into()))?;
        inspect(&editor.cos, page)
    }
}

fn colour(doc: &Document, raw: &Object, report: &mut OutputInspection) {
    let value = doc.resolve(raw);
    if let Some(name) = value.as_name() {
        report.spaces.insert(String::from_utf8_lossy(name).into_owned());
    } else if let Some(values) = value.as_array() {
        if let Some(kind) = values.first().and_then(Object::as_name) {
            report.spaces.insert(String::from_utf8_lossy(kind).into_owned());
            if kind == b"Separation" {
                if let Some(name) = values.get(1).and_then(Object::as_name) {
                    report.spots.insert(String::from_utf8_lossy(name).into_owned());
                }
            } else if kind == b"DeviceN"
                && let Some(names) = values.get(1).map(|o| doc.resolve(o))
                && let Some(names) = names.as_array()
            {
                if names.len() > 64 {
                    report.incomplete("DeviceN colourant limit reached.");
                }
                for name in names.iter().filter_map(Object::as_name).take(64) {
                    report.spots.insert(String::from_utf8_lossy(name).into_owned());
                }
            }
        } else {
            report.incomplete("Unrecognized colour-space definition.");
        }
    } else {
        report.incomplete("Unrecognized colour-space definition.");
    }
}

fn inspect(doc: &Document, page: usize) -> Result<OutputInspection, EditError> {
    let pages = pdfcraft_model::pages(doc);
    let page_info = pages.get(page).ok_or_else(|| EditError::Invalid("page is outside the document".into()))?;
    let mut report = OutputInspection { page, complete: true, ..Default::default() };
    // Share resource owners: inherited dictionaries must not be copied once per Form edge.
    let resources = page_info.dict.get(b"Resources").map(|o| doc.resolve(o)).unwrap_or_else(|| Arc::new(Object::Dict(Dict::new())));
    if page_info.dict.contains(b"Annots") {
        report.incomplete("Annotation appearance colours are not included in the source scan.");
    }
    let mut pending = vec![(page_info.dict.get(b"Contents").cloned().unwrap_or(Object::Null), resources)];
    let mut seen = BTreeSet::new();
    // shortcut: source inventory only; a press soft-proof requires an ICC/separation renderer.
    let mut remaining = 8 * 1024 * 1024usize;
    let mut visits = 0usize;
    while let Some((raw, resources)) = pending.pop() {
        visits += 1;
        if visits > 2048 {
            report.incomplete("Resource traversal limit reached.");
            break;
        }
        if let Some(r) = raw.as_ref()
            && !seen.insert(r)
        {
            continue;
        }
        let object = doc.resolve(&raw);
        if let Some(array) = object.as_array() {
            // A page's content pieces are one program, including operands split across streams.
            let mut bytes = Vec::new();
            for piece in array.iter().take(2048) {
                let piece = doc.resolve(piece);
                if let Object::Stream(stream) = &*piece {
                    match stream.decoded_within(remaining) {
                        Ok(data) => {
                            remaining = remaining.saturating_sub(data.len());
                            bytes.extend_from_slice(&data);
                        }
                        Err(_) => report.incomplete("A content stream could not be decoded within the 8 MiB inspection budget."),
                    }
                } else {
                    report.incomplete("A content piece is not a stream.");
                }
            }
            if array.len() > 2048 {
                report.incomplete("Content-piece limit reached.");
            }
            operators(&bytes, &mut report);
        } else if let Object::Stream(stream) = &*object {
            match stream.decoded_within(remaining) {
                Ok(bytes) => {
                    remaining = remaining.saturating_sub(bytes.len());
                    operators(&bytes, &mut report);
                }
                Err(_) => report.incomplete("A content stream could not be decoded within the 8 MiB inspection budget."),
            }
        } else if !matches!(&*object, Object::Null) {
            report.incomplete("Content is not a stream.");
        }
        inspect_resources(doc, &resources, &mut report, &mut pending);
        if pending.len() > 2048 {
            report.incomplete("Resource traversal limit reached.");
            break;
        }
    }
    Ok(report)
}

fn inspect_resources(doc: &Document, owner: &Arc<Object>, report: &mut OutputInspection, pending: &mut Vec<(Object, Arc<Object>)>) {
    let Some(resources) = owner.as_dict() else {
        report.incomplete("Malformed page or Form resources.");
        return;
    };
    for group in [b"ColorSpace".as_slice(), b"ExtGState", b"XObject"] {
        let Some(values) = resources.get(group).map(|o| doc.resolve(o)) else { continue };
        let Some(values) = values.as_dict() else {
            report.incomplete("Malformed resource dictionary.");
            continue;
        };
        for (index, (_, raw)) in values.iter().enumerate() {
            if index >= 2048 || pending.len() >= 2048 {
                report.incomplete("Resource traversal limit reached.");
                break;
            }
            if group == b"ColorSpace" {
                colour(doc, raw, report);
                continue;
            }
            let value = doc.resolve(raw);
            if group == b"ExtGState" {
                if let Some(d) = value.as_dict() {
                    report.overprint_declared |=
                        [b"OP".as_slice(), b"op"].iter().any(|key| d.get(key).is_some_and(|v| matches!(&*doc.resolve(v), Object::Bool(true))));
                }
            } else if let Object::Stream(stream) = &*value {
                if stream.dict.name(b"Subtype") == Some(b"Form") {
                    let nested = stream.dict.get(b"Resources").map(|o| doc.resolve(o)).unwrap_or_else(|| Arc::clone(owner));
                    pending.push((raw.clone(), nested));
                } else if stream.dict.name(b"Subtype") == Some(b"Image")
                    && let Some(space) = stream.dict.get(b"ColorSpace")
                {
                    colour(doc, space, report);
                }
            }
        }
    }
    if resources.contains(b"Pattern") || resources.contains(b"Shading") {
        report.incomplete("Pattern and shading paint colours are not expanded.");
    }
}

fn operators(bytes: &[u8], report: &mut OutputInspection) {
    let mut reader = pdfcraft_content::ops(bytes);
    for (index, op) in reader.by_ref().enumerate() {
        if index >= 100_000 {
            report.incomplete("Content-operator limit reached.");
            break;
        }
        let space = match op.op.as_slice() {
            b"k" | b"K" => Some("DeviceCMYK"),
            b"rg" | b"RG" => Some("DeviceRGB"),
            b"g" | b"G" => Some("DeviceGray"),
            _ => None,
        };
        if let Some(space) = space {
            report.spaces.insert(space.into());
        }
        if matches!(op.op.as_slice(), b"k" | b"K") {
            if let Some(values) = op.nums::<4>().filter(|a| a.iter().all(|n| n.is_finite() && (0.0..=1.0).contains(n))) {
                let sum = values.into_iter().sum::<f64>() * 100.0;
                report.max_source_cmyk_percent = Some(report.max_source_cmyk_percent.unwrap_or(0.0).max(sum));
            } else {
                report.incomplete("Invalid DeviceCMYK components.");
            }
        }
        if matches!(op.op.as_slice(), b"cs" | b"CS")
            && let Some(name) = op.name(0)
        {
            report.spaces.insert(String::from_utf8_lossy(name).into_owned());
        }
        if let Some((dict, _)) = &op.inline
            && let Some(space) = dict.get(b"CS").or_else(|| dict.get(b"ColorSpace"))
            && let Some(name) = space.as_name()
        {
            report.spaces.insert(String::from_utf8_lossy(name).into_owned());
        }
    }
    if reader.skipped() > 0 || reader.ambiguous_images() > 0 {
        report.incomplete("Some content could not be parsed unambiguously.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_colours_are_not_misreported_as_composited_ink_coverage() {
        let mut report = OutputInspection { complete: true, ..Default::default() };
        operators(b"0.1 0.2 0.3 0.4 k 1 0 0 rg 1 1 1 1 K 3 0 0 0 k", &mut report);
        assert_eq!(report.max_source_cmyk_percent, Some(400.0));
        assert!(report.spaces.contains("DeviceRGB"));
        assert!(!report.complete);
        assert!(report.text().contains("not simulated"));
    }
}

#[cfg(test)]
mod graph_tests {
    use super::*;
    use pdfcraft_cos::Stream;
    use std::sync::Arc;
    #[test]
    fn nested_resources_spots_overprint_and_cycles_are_bounded_and_read_only() {
        let mut doc = Document::open(Arc::new(crate::tests::fixture(1))).unwrap();
        let page = pdfcraft_model::pages(&doc)[0].obj;
        let form = doc.add(Object::Null);
        let mut spaces = Dict::new();
        spaces.set(
            b"Spot".to_vec(),
            Object::Array(vec![Object::name("Separation"), Object::name("CustomOrange"), Object::name("DeviceCMYK"), Object::Null]),
        );
        let mut xobjects = Dict::new();
        xobjects.set(b"Loop".to_vec(), Object::Ref(form));
        let mut gs = Dict::new();
        gs.set(b"OP".to_vec(), Object::Bool(true));
        let mut states = Dict::new();
        states.set(b"Over".to_vec(), Object::Dict(gs));
        let mut resources = Dict::new();
        resources.set(b"ColorSpace".to_vec(), Object::Dict(spaces));
        resources.set(b"XObject".to_vec(), Object::Dict(xobjects));
        resources.set(b"ExtGState".to_vec(), Object::Dict(states));
        let mut form_dict = Dict::new();
        form_dict.set(b"Subtype".to_vec(), Object::name("Form"));
        form_dict.set(b"Resources".to_vec(), Object::Dict(resources.clone()));
        doc.set(form, Object::Stream(Stream::from_raw(form_dict, b".5 .5 .5 .5 k /Loop Do".to_vec())));
        let content = doc.add(Object::Stream(Stream::from_raw(Dict::new(), b"1 0 0 rg /Loop Do".to_vec())));
        doc.update_dict(page, |d| {
            d.set(b"Resources".to_vec(), Object::Dict(resources));
            d.set(b"Contents".to_vec(), Object::Ref(content));
        })
        .unwrap();
        let before = pdfcraft_cos::write_full(&doc, &Default::default()).unwrap();
        let report = inspect(&doc, 0).unwrap();
        assert!(report.spots.contains("CustomOrange"));
        assert!(report.spaces.contains("DeviceRGB"));
        assert_eq!(report.max_source_cmyk_percent, Some(200.0));
        assert!(report.overprint_declared);
        assert_eq!(pdfcraft_cos::write_full(&doc, &Default::default()).unwrap(), before);
        assert!(inspect(&doc, 1).is_err());
    }
    #[test]
    fn oversized_content_reports_incomplete_instead_of_passing() {
        let mut doc = Document::open(Arc::new(crate::tests::fixture(1))).unwrap();
        let page = pdfcraft_model::pages(&doc)[0].obj;
        let content = doc.add(Object::Stream(Stream::from_raw(Dict::new(), vec![b' '; 8 * 1024 * 1024 + 1])));
        doc.update_dict(page, |d| d.set(b"Contents".to_vec(), Object::Ref(content))).unwrap();
        let report = inspect(&doc, 0).unwrap();
        assert!(!report.complete);
        assert!(!report.warnings.is_empty());
    }
}
