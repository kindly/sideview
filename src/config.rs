//! `.sideview.toml` — per-repo configuration, committed, hand- or
//! agent-authored (V3.sv). Placement follows the project's own principle:
//! `.sideview/` self-ignores and holds disposable machinery, so versioned
//! intent lives beside it at the repo root, not inside it.
//!
//! It supplies only what a file cannot say about itself. An `.sv` page
//! declares its own label, order and category in canon; markdown and HTML
//! have nowhere to put attributes, so for them this is the only voice — and
//! it is what survives a deleted db, since `.sv` files announce themselves
//! in the startup scan and a repo's markdown never should.
//!
//! No CLI writer, deliberately (author, 2026-08-10): agents edit files
//! trivially, and the page file's one-author rule extends here.

use std::path::Path;

use serde::Deserialize;

pub const FILE: &str = ".sideview.toml";

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Config {
    /// The port to bind, durable across a deleted db — the wrinkle V2's own
    /// sign-off recorded (the page resurrects; its address did not).
    pub port: Option<u16>,
    /// Pages to bind at startup. `.sv` files need no entry unless they want
    /// a category; imported formats need one to exist at all.
    pub pages: Vec<PageEntry>,
    /// Installed extensions. Listing one here is the trust act
    /// (EXTENSIONS.md): everything an extension can say about itself lives
    /// in its own manifest; config says only that it is installed, and
    /// whether it is currently enabled.
    pub extensions: Vec<ExtensionEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExtensionEntry {
    /// Project-relative directory holding plugin.toml and the entry files.
    pub path: String,
    pub enabled: Option<bool>,
}

/// An extension's own manifest — extensions/<name>/plugin.toml. Required in
/// full: a stray index.html never becomes an extension by accident.
#[derive(Debug, Clone, Deserialize)]
pub struct Manifest {
    /// Claims the block tag `sv-<name>`.
    pub name: String,
    /// The version of the EXTENSIONS.md contract the extension expects.
    pub api: u32,
    /// "inline" | "frame" | "sandbox" — only "frame" is built (api 1).
    pub render: String,
    /// Entry file, relative to the extension directory.
    pub entry: String,
    /// The one binary call_cli may exec; bare names resolve on PATH,
    /// "./…" relative to the extension directory. Optional: a pure-UI
    /// extension has no binary at all.
    pub bin: Option<String>,
}

/// The highest EXTENSIONS.md contract this build implements.
pub const EXTENSION_API: u32 = 1;

/// A loaded, validated extension: manifest plus its directory.
#[derive(Debug, Clone)]
pub struct Extension {
    pub manifest: Manifest,
    /// Project-relative directory (the config entry's path).
    pub dir: String,
}

impl Extension {
    pub fn tag(&self) -> String {
        format!("sv-{}", self.manifest.name)
    }
}

/// Load every enabled extension's manifest. Failures are facts to render,
/// not reasons to refuse service: each problem comes back as a
/// (path, message) the caller logs, and a degraded manifest is simply not
/// in the map — its blocks fall through to the honest unknown-tag block.
pub fn load_extensions(root: &Path, cfg: &Config) -> (Vec<Extension>, Vec<(String, String)>) {
    let mut out = Vec::new();
    let mut problems = Vec::new();
    for e in &cfg.extensions {
        if e.enabled == Some(false) {
            continue;
        }
        let manifest_path = root.join(&e.path).join("plugin.toml");
        let src = match std::fs::read_to_string(&manifest_path) {
            Ok(s) => s,
            Err(err) => {
                problems.push((e.path.clone(), format!("no plugin.toml ({err})")));
                continue;
            }
        };
        let m: Manifest = match toml::from_str(&src) {
            Ok(m) => m,
            Err(err) => {
                problems.push((e.path.clone(), format!("plugin.toml: {err}")));
                continue;
            }
        };
        if !m.name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
            || m.name.is_empty()
        {
            problems.push((e.path.clone(), format!("name {:?} is not [a-z0-9-]", m.name)));
            continue;
        }
        if m.api > EXTENSION_API {
            problems.push((
                e.path.clone(),
                format!("wants api {} but this sideview knows {}", m.api, EXTENSION_API),
            ));
            continue;
        }
        if m.render != "frame" {
            // inline and sandbox are specified, not built; anything else is
            // a typo. Either way the honest outcome is the same: not loaded.
            problems.push((
                e.path.clone(),
                format!("render {:?} is not built in this sideview (only \"frame\")", m.render),
            ));
            continue;
        }
        if out.iter().any(|x: &Extension| x.manifest.name == m.name) {
            problems.push((e.path.clone(), format!("duplicate extension name {:?}", m.name)));
            continue;
        }
        out.push(Extension { manifest: m, dir: e.path.clone() });
    }
    (out, problems)
}

#[derive(Debug, Clone, Deserialize)]
pub struct PageEntry {
    /// Project-relative path to a `.sv`, `.md` or `.html` file.
    pub path: String,
    /// Page id (the chip's identity and URL). Defaults to the file stem.
    pub id: Option<String>,
    /// Human label for the chip. A `.sv` file's own `label` wins over this.
    pub label: Option<String>,
    /// Grouping for the switcher and the homepage.
    pub category: Option<String>,
    /// Sort key within a category; unset sorts after those that set one.
    pub order: Option<f64>,
    /// HTML only: `inline` (styled by the page, commentable) or `iframe`
    /// (its own world, no anchors inside). Ignored for other formats.
    pub render: Option<String>,
}

impl PageEntry {
    /// The page id: explicit, else the file stem.
    pub fn page_id(&self) -> String {
        self.id.clone().unwrap_or_else(|| {
            Path::new(&self.path)
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| self.path.clone())
        })
    }
}

/// Read the config beside `root`. A malformed file is reported, never fatal:
/// a typo must not cost you the page (V3.sv), so the caller logs the string
/// and serves with defaults.
pub fn load(root: &Path) -> (Config, Option<String>) {
    let path = root.join(FILE);
    let src = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(_) => return (Config::default(), None), // absent is the normal case
    };
    match toml::from_str::<Config>(&src) {
        Ok(c) => (c, None),
        Err(e) => (Config::default(), Some(format!("{}: {e}", path.display()))),
    }
}

/// How a bound file is rendered — decided by extension, with HTML's mode
/// coming from config because a foreign file cannot declare it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// The composed format: typed blocks, parsed by format.rs.
    Sv,
    /// Imported markdown: one prose block, fully commentable.
    Markdown,
    /// Imported HTML, rendered into the page: styled by it, commentable,
    /// and its scripts run here — the markup-block trust story.
    HtmlInline,
    /// Imported HTML in a sandboxed iframe: its own world, and the page
    /// cannot reach inside, so comments stop at the block.
    HtmlFrame,
}

pub fn format_of(rel: &str, render: Option<&str>) -> Format {
    let ext = Path::new(rel)
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "md" | "markdown" => Format::Markdown,
        "html" | "htm" => {
            if render == Some("iframe") { Format::HtmlFrame } else { Format::HtmlInline }
        }
        _ => Format::Sv,
    }
}

/// Imported formats are foreign files sideview does not own — `page rm`
/// deletes files, and the ✕ must never be the thing that removes DESIGN.md.
pub fn is_imported(rel: &str) -> bool {
    format_of(rel, None) != Format::Sv
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extensions_load_validated_and_degrade_by_omission() {
        let dir = std::env::temp_dir().join(format!("sv-ext-{}", uuid::Uuid::new_v4()));
        for (name, manifest) in [
            ("good", "name = \"wordcount\"\napi = 1\nrender = \"frame\"\nentry = \"index.html\"\nbin = \"wc\"\n"),
            ("newer", "name = \"future\"\napi = 99\nrender = \"frame\"\nentry = \"index.html\"\n"),
            ("inline", "name = \"prose-ext\"\napi = 1\nrender = \"inline\"\nentry = \"index.html\"\n"),
            ("broken", "name = = nope"),
        ] {
            let d = dir.join("extensions").join(name);
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join("plugin.toml"), manifest).unwrap();
        }
        let cfg: Config = toml::from_str(
            r#"
            [[extensions]]
            path = "extensions/good"
            [[extensions]]
            path = "extensions/newer"
            [[extensions]]
            path = "extensions/inline"
            [[extensions]]
            path = "extensions/broken"
            [[extensions]]
            path = "extensions/missing"
            [[extensions]]
            path = "extensions/good"      # duplicate name
            [[extensions]]
            path = "extensions/off"
            enabled = false
            "#,
        )
        .unwrap();
        let (exts, problems) = load_extensions(&dir, &cfg);
        assert_eq!(exts.len(), 1, "only the valid one loads");
        assert_eq!(exts[0].tag(), "sv-wordcount");
        assert_eq!(exts[0].manifest.bin.as_deref(), Some("wc"));
        // Every failure is a named fact; the disabled one is not a failure.
        assert_eq!(problems.len(), 5, "newer api, unbuilt render, broken toml, missing, dup: {problems:?}");
        assert!(problems.iter().any(|(_, m)| m.contains("api 99")));
        assert!(problems.iter().any(|(_, m)| m.contains("only \"frame\"")));
    }

    #[test]
    fn config_parses_pages_and_port_and_survives_nonsense() {
        let dir = std::env::temp_dir().join(format!("sv-cfg-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(FILE),
            r#"
            port = 46423
            [[pages]]
            path = "DESIGN.md"
            category = "design"
            [[pages]]
            path = "report.html"
            id = "report"
            render = "iframe"
            order = 3
            "#,
        )
        .unwrap();
        let (c, err) = load(&dir);
        assert!(err.is_none());
        assert_eq!(c.port, Some(46423));
        assert_eq!(c.pages.len(), 2);
        assert_eq!(c.pages[0].page_id(), "DESIGN", "id defaults to the stem");
        assert_eq!(c.pages[0].category.as_deref(), Some("design"));
        assert_eq!(c.pages[1].page_id(), "report");

        // A typo reports and degrades; it never denies service.
        std::fs::write(dir.join(FILE), "port = = 3").unwrap();
        let (c, err) = load(&dir);
        assert!(err.is_some(), "malformed config is reported");
        assert!(c.pages.is_empty(), "…and yields defaults, not a refusal");

        // Absent is the normal case, and silent.
        std::fs::remove_file(dir.join(FILE)).unwrap();
        assert!(load(&dir).1.is_none());
    }

    #[test]
    fn formats_come_from_the_extension_and_html_mode_from_config() {
        assert_eq!(format_of("V3.sv", None), Format::Sv);
        assert_eq!(format_of("DESIGN.md", None), Format::Markdown);
        assert_eq!(format_of("docs/report.HTML", None), Format::HtmlInline);
        assert_eq!(format_of("docs/report.html", Some("iframe")), Format::HtmlFrame);
        assert!(is_imported("DESIGN.md") && !is_imported("V3.sv"));
    }
}
