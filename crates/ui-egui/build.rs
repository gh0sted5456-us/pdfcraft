//! Bake the contributor credits into the binary as static Rust tables, and embed the craft-i18n
//! interface catalogs. Nothing is read at run time.
//!
//! - contributors/contributors.json: commit stats per GitHub username, with a mark for whose people.toml
//!   entry is verified.
//! - contributors/people.toml: the names people chose for themselves. A name is used only when the
//!   JSON says the entry was verified (`people_entry`).
//!
//! A missing or malformed file gives empty tables (or no names) and a warning.
//!
//! - `CRAFT_I18N_DIR`, a craft-i18n checkout (storytold/craft-i18n, at the SHA in
//!   `craft-i18n.rev`): `craft_i18n.rs` holds `CRAFT_I18N`, one (locale, shared catalog, PdfCraft
//!   catalog) entry per locale that has a `pdfcraft.tsv`. Unset, or not a checkout (a warning),
//!   it is empty and the interface is English only; with `CRAFT_I18N_REQUIRED=1` (CI and releases)
//!   that is an error instead. Recipe: craft-i18n's `docs/integration.md`.
//! - Lion PDF overlay, `i18n-lion/<locale>.tsv`: rows Lion PDF adds or changes. Each locale's
//!   embedded app catalog is the overlay first, then craft-i18n's rows whose context and source
//!   the overlay doesn't define (written to `OUT_DIR/i18n/<locale>.tsv`). A locale only the overlay
//!   has (`en-gb`) is embedded from the overlay alone, with or without `CRAFT_I18N_DIR`.

use std::fmt::Write as _;

use serde_json::Value;

fn main() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../contributors");
    let json = read(&dir.join("contributors.json"), "the About window shows no contributors", |t| {
        serde_json::from_str::<Value>(t).map_err(|e| e.to_string())
    })
    .unwrap_or(Value::Null);
    let people = read(&dir.join("people.toml"), "contributors are shown by username only", |t| t.parse::<toml::Table>().map_err(|e| e.to_string()))
        .unwrap_or_default();
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap_or_default()).join("credits.rs");
    if let Err(e) = std::fs::write(&out, generate(&json, &people)) {
        println!("cargo::warning=writing {}: {e}", out.display());
    }
    let out = out.with_file_name("craft_i18n.rs");
    if let Err(e) = std::fs::write(&out, craft_i18n()) {
        println!("cargo::error=writing {}: {e}", out.display());
    }
}

/// The catalog this app reads in each locale: `locales/<locale>/pdfcraft.tsv`.
const APP_CATALOG: &str = "pdfcraft.tsv";

/// `CRAFT_I18N` from `CRAFT_I18N_DIR`: the `Embedded` list the craft-i18n crate reads.
fn craft_i18n() -> String {
    println!("cargo::rerun-if-env-changed=CRAFT_I18N_DIR");
    println!("cargo::rerun-if-env-changed=CRAFT_I18N_REQUIRED");
    let required = std::env::var("CRAFT_I18N_REQUIRED").is_ok_and(|v| v.trim() == "1");
    let entries = match std::env::var_os("CRAFT_I18N_DIR").filter(|d| !d.is_empty()) {
        // Not canonicalized: on Windows that gives `\\?\C:\…`, which `include_str!` can't read.
        Some(dir) => match locales(&std::path::PathBuf::from(dir).join("locales")) {
            Ok(entries) => entries,
            Err(e) => {
                let level = if required { "error" } else { "warning" };
                println!("cargo::{level}=CRAFT_I18N_DIR: {e}; the interface is English only");
                Vec::new()
            }
        },
        None => {
            if required {
                println!("cargo::error=CRAFT_I18N_REQUIRED=1 but CRAFT_I18N_DIR is unset: a release must embed the translations");
            }
            Vec::new()
        }
    };
    let entries = with_lion_overlay(entries);
    let mut o = String::from("/// (locale, shared catalog, PdfCraft's catalog), one entry per locale; `\"\"` for a missing file.\n");
    o.push_str("pub static CRAFT_I18N: &craft_i18n::Embedded = &[\n");
    for (code, shared, app) in entries {
        // `{:?}` escapes the path (Windows backslashes) as a Rust string literal.
        let shared = shared.map_or_else(|| "\"\"".to_string(), |p| format!("include_str!({p:?})"));
        let _ = writeln!(o, "    ({code:?}, {shared}, include_str!({app:?})),");
    }
    o.push_str("];\n");
    o
}

/// One embedded locale: its code, its `common.tsv` if it has one, and its PdfCraft catalog.
type Locale = (String, Option<std::path::PathBuf>, std::path::PathBuf);

/// Every locale under `dir` with PdfCraft's catalog, by code. An error when `dir` isn't a
/// craft-i18n `locales/` directory.
fn locales(dir: &std::path::Path) -> Result<Vec<Locale>, String> {
    // Build scripts run in the crate's directory and `include_str!` resolves against the
    // generated file, so a relative path would mean two different places.
    if !dir.is_absolute() {
        return Err(format!("{} is not an absolute path", dir.display()));
    }
    // Before read_dir, so a checkout that appears later reruns the script.
    println!("cargo::rerun-if-changed={}", dir.display());
    let list = std::fs::read_dir(dir).map_err(|e| format!("{} is not a craft-i18n checkout ({e})", dir.display()))?;
    let mut out = Vec::new();
    for entry in list.flatten() {
        let path = entry.path();
        let Some(code) = path.file_name().and_then(|n| n.to_str()).map(str::to_string) else { continue };
        let app = path.join(APP_CATALOG);
        if !app.is_file() {
            continue;
        }
        println!("cargo::rerun-if-changed={}", app.display());
        let shared = Some(path.join("common.tsv")).filter(|p| p.is_file());
        if let Some(p) = &shared {
            println!("cargo::rerun-if-changed={}", p.display());
        }
        out.push((code, shared, app));
    }
    if out.is_empty() {
        return Err(format!("{} has no locale with a {APP_CATALOG}", dir.display()));
    }
    out.sort();
    Ok(out)
}

/// Lion PDF's overlay catalogs (`i18n-lion/<locale>.tsv`) merged into `entries`: see the module
/// documentation. A merged catalog is written to `OUT_DIR/i18n/`; an unreadable overlay is a
/// warning and leaves the craft-i18n catalog as it is.
fn with_lion_overlay(entries: Vec<Locale>) -> Vec<Locale> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("i18n-lion");
    println!("cargo::rerun-if-changed={}", dir.display());
    let out_dir = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap_or_default()).join("i18n");
    let _ = std::fs::create_dir_all(&out_dir);
    let overlay = |code: &str| -> Option<(std::path::PathBuf, String)> {
        let p = dir.join(format!("{code}.tsv"));
        println!("cargo::rerun-if-changed={}", p.display());
        let text = std::fs::read_to_string(&p).ok()?;
        Some((p, text))
    };
    // A row's key, as craft-i18n's parser takes it: (context, source).
    fn keys(text: &str) -> std::collections::HashSet<(String, String)> {
        text.lines()
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .filter_map(|l| {
                let mut f = l.trim_end_matches('\r').splitn(3, '\t');
                Some((f.next()?.to_string(), f.next()?.to_string()))
            })
            .collect()
    }
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (code, shared, app) in entries {
        seen.insert(code.clone());
        let Some((_, lion)) = overlay(&code) else {
            out.push((code, shared, app));
            continue;
        };
        let upstream = match std::fs::read_to_string(&app) {
            Ok(t) => t,
            Err(e) => {
                println!("cargo::warning={}: {e}; the Lion PDF overlay for {code} is not applied", app.display());
                out.push((code, shared, app));
                continue;
            }
        };
        let own = keys(&lion);
        let mut merged = lion.clone();
        if !merged.ends_with('\n') {
            merged.push('\n');
        }
        for line in upstream.lines() {
            let kept = line.is_empty() || line.starts_with('#') || {
                let mut f = line.trim_end_matches('\r').splitn(3, '\t');
                match (f.next(), f.next()) {
                    (Some(c), Some(s)) => !own.contains(&(c.to_string(), s.to_string())),
                    _ => true,
                }
            };
            if kept {
                merged.push_str(line);
                merged.push('\n');
            }
        }
        let path = out_dir.join(format!("{code}.tsv"));
        if let Err(e) = std::fs::write(&path, merged) {
            println!("cargo::warning={}: {e}; the Lion PDF overlay for {code} is not applied", path.display());
            out.push((code, shared, app));
            continue;
        }
        out.push((code, shared, path));
    }
    // Locales only the overlay has.
    if let Ok(list) = std::fs::read_dir(&dir) {
        let mut extra: Vec<_> = list
            .flatten()
            .filter_map(|e| e.path().file_stem().and_then(|s| s.to_str()).map(str::to_string))
            .filter(|code| !seen.contains(code) && STANDALONE_OVERLAYS.contains(&code.as_str()))
            .collect();
        extra.sort();
        for code in extra {
            if let Some((p, _)) = overlay(&code) {
                out.push((code, None, p));
            }
        }
    }
    out.sort();
    out
}

/// Overlay locales complete on their own (embedded even without craft-i18n). The others only add
/// to craft-i18n's catalogs.
const STANDALONE_OVERLAYS: &[&str] = &["en-gb"];

fn read<T>(path: &std::path::Path, fallback: &str, parse: impl Fn(&str) -> Result<T, String>) -> Option<T> {
    println!("cargo::rerun-if-changed={}", path.display());
    let r = std::fs::read_to_string(path).map_err(|e| e.to_string()).and_then(|t| parse(&t));
    if let Err(e) = &r {
        println!("cargo::warning={}: {e}; {fallback}", path.display());
    }
    r.ok()
}

/// A non-empty string field of someone's people.toml entry, as the builder call `.<method>("..")`; nothing when empty.
fn name(method: &str, v: Option<&str>) -> String {
    match v.map(str::trim).filter(|s| !s.is_empty()) {
        Some(x) => format!(".{method}({x:?})"),
        None => String::new(),
    }
}

fn s(v: &Value, k: &str) -> String {
    format!("{:?}", v.get(k).and_then(Value::as_str).unwrap_or(""))
}

fn n(v: &Value, k: &str) -> u64 {
    v.get(k).and_then(Value::as_u64).unwrap_or(0)
}

fn rows(json: &Value, k: &str) -> Vec<Value> {
    json.get(k).and_then(Value::as_array).cloned().unwrap_or_default()
}

fn generate(json: &Value, people: &toml::Table) -> String {
    let mut o = String::new();
    let _ = writeln!(o, "/// The credits tables baked from contributors/contributors.json.");
    let _ = writeln!(o, "pub(crate) fn credits() -> Credits {{");
    let _ = writeln!(o, "    let contributors: Vec<Contributor> = vec![");
    for c in rows(json, "contributors") {
        let login = c.get("login").and_then(Value::as_str).unwrap_or("");
        if login.is_empty() {
            continue;
        }
        // Names only from a verified, self-chosen people.toml entry.
        let entry = people.get("people").and_then(|p| p.get(login)).filter(|_| c.get("people_entry").and_then(Value::as_bool) == Some(true));
        let field = |k: &str| entry.and_then(|e| e.get(k)).and_then(toml::Value::as_str);
        let synced = entry.and_then(|e| e.get("sync_github_name")).and_then(toml::Value::as_bool) == Some(true);
        let display = field("display_name")
            .filter(|s| !s.trim().is_empty())
            .or_else(|| if synced { c.get("github_name").and_then(Value::as_str) } else { None });
        let _ = writeln!(
            o,
            "        Contributor::new({login:?}){}{}.profile({:?}).counts({}, {}).lines({}, {}).binary({}, {}).active({}, {}),",
            name("display_name", display),
            name("real_name", field("real_name")),
            format!("https://github.com/{login}"),
            n(&c, "prs"),
            n(&c, "commits"),
            n(&c, "lines_added"),
            n(&c, "lines_deleted"),
            n(&c, "binary_added"),
            n(&c, "binary_deleted"),
            s(&c, "first_commit"),
            s(&c, "last_commit"),
        );
    }
    let _ = writeln!(o, "    ];");
    let _ = writeln!(o, "    let models: Vec<ModelCredit> = vec![");
    for m in rows(json, "models") {
        let _ = writeln!(
            o,
            "        ModelCredit::new({}, {}, {}).work({}, {}, {}),",
            s(&m, "company"),
            s(&m, "model"),
            s(&m, "version"),
            n(&m, "commits"),
            n(&m, "lines_added"),
            n(&m, "lines_deleted"),
        );
    }
    let _ = writeln!(o, "    ];");
    let _ = writeln!(o, "    Credits::new(contributors, models, {})", n(json, "commits"));
    let _ = writeln!(o, "}}");
    o
}
