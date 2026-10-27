use eframe::egui::{self, Color32, FontId, TextFormat, text::LayoutJob};
use std::ops::Range;
use std::{
    collections::HashMap,
    sync::{Arc, OnceLock},
};
use syntect::{
    easy::HighlightLines, highlighting::ThemeSet, parsing::SyntaxSet, util::LinesWithEndings,
};
use uuid::Uuid;

/// Bound synchronous syntax work; these limits do not bound text editing or file size.
pub const MAX_HIGHLIGHT_BYTES: usize = 256 * 1024;
pub const MAX_HIGHLIGHT_LINE_BYTES: usize = 16 * 1024;

pub fn highlighting_limited(text: &str, syntax: &str) -> bool {
    syntax != "plain"
        && (text.len() > MAX_HIGHLIGHT_BYTES
            || text
                .split('\n')
                .any(|line| line.len() > MAX_HIGHLIGHT_LINE_BYTES))
}

pub struct HighlightOptions<'a> {
    pub syntax: &'a str,
    pub size: f32,
    pub matched: Option<Range<usize>>,
}

#[derive(Debug, Clone)]
pub struct Language {
    pub id: String,
    pub label: String,
    extension: Option<&'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyntaxPalette {
    pub foreground: Color32,
    pub keyword: Color32,
    pub string: Color32,
    pub number: Color32,
    pub comment: Color32,
}
impl SyntaxPalette {
    fn recolor(self, color: Color32, base: Color32) -> Color32 {
        if color == base {
            return self.foreground;
        }
        let hsv = egui::ecolor::Hsva::from(color);
        if hsv.s < 0.13 {
            self.comment
        } else if hsv.h >= 0.18 && hsv.h < 0.48 {
            self.string
        } else if hsv.h >= 0.48 && hsv.h < 0.82 {
            self.keyword
        } else {
            self.number
        }
    }
}

/// Registry and highlighting are read-only with respect to the source text.
pub struct Highlighter {
    syntaxes: OnceLock<SyntaxSet>,
    themes: OnceLock<ThemeSet>,
    languages: Vec<Language>,
}

impl Default for Highlighter {
    fn default() -> Self {
        let mut languages = Vec::new();
        let candidates = [
            ("plain", "纯文本", None),
            ("json", "JSON", Some("json")),
            ("yaml", "YAML", Some("yaml")),
            ("toml", "TOML", Some("toml")),
            ("markdown", "Markdown", Some("md")),
            ("rust", "Rust", Some("rs")),
            ("go", "Go", Some("go")),
            ("python", "Python", Some("py")),
            ("javascript", "JavaScript", Some("js")),
            ("typescript", "TypeScript", Some("ts")),
            ("html", "HTML", Some("html")),
            ("css", "CSS", Some("css")),
            ("sql", "SQL", Some("sql")),
            ("shell", "Bash / Shell", Some("sh")),
            ("powershell", "PowerShell", Some("ps1")),
            ("c", "C", Some("c")),
            ("cpp", "C++", Some("cpp")),
            ("java", "Java", Some("java")),
            ("xml", "XML", Some("xml")),
        ];
        // This catalog is checked against the pinned grammar bundle in tests.
        // Listing languages must not decompress all grammars on a blank document.
        for (id, label, extension) in candidates {
            languages.push(Language {
                id: id.into(),
                label: label.into(),
                extension,
            });
        }
        Self {
            syntaxes: OnceLock::new(),
            themes: OnceLock::new(),
            languages,
        }
    }
}

impl Highlighter {
    pub fn languages(&self) -> &[Language] {
        &self.languages
    }

    pub fn language_label<'a>(&'a self, id: &'a str) -> &'a str {
        self.languages
            .iter()
            .find(|l| l.id == id)
            .map_or(id, |l| l.label.as_str())
    }

    pub fn layout_job_styled(
        &self,
        text: &str,
        language: &str,
        size: f32,
        dark: bool,
        palette: SyntaxPalette,
    ) -> Result<LayoutJob, String> {
        let mut job = self.layout_job(text, language, size, dark)?;
        let base = if dark {
            Color32::from_rgb(192, 197, 206)
        } else {
            Color32::from_rgb(50, 50, 50)
        };
        for section in &mut job.sections {
            section.format.color = palette.recolor(section.format.color, base);
        }
        Ok(job)
    }

    pub fn layout_job(
        &self,
        text: &str,
        language: &str,
        size: f32,
        dark: bool,
    ) -> Result<LayoutJob, String> {
        let language = self
            .languages
            .iter()
            .find(|l| l.id == language)
            .ok_or_else(|| format!("Unknown syntax: {language}"))?;
        if text.is_empty() || language.id == "plain" || highlighting_limited(text, &language.id) {
            // Match the foreground of the two bundled display themes without
            // loading grammars/themes for plain text or the large-text fallback.
            return Ok(LayoutJob::simple(
                text.into(),
                FontId::monospace(size),
                if dark {
                    Color32::from_rgb(192, 197, 206)
                } else {
                    Color32::from_rgb(50, 50, 50)
                },
                f32::INFINITY,
            ));
        }
        let syntaxes = self.syntaxes.get_or_init(two_face::syntax::extra_newlines);
        let syntax = language
            .extension
            .and_then(|ext| syntaxes.find_syntax_by_extension(ext))
            .ok_or_else(|| format!("Missing grammar: {}", language.id))?;
        let themes = self.themes.get_or_init(ThemeSet::load_defaults);
        let theme = &themes.themes[if dark {
            "base16-ocean.dark"
        } else {
            "InspiredGitHub"
        }];
        let mut highlighter = HighlightLines::new(syntax, theme);
        let mut job = LayoutJob::default();
        for line in LinesWithEndings::from(text) {
            for (style, token) in highlighter
                .highlight_line(line, syntaxes)
                .map_err(|e| e.to_string())?
            {
                job.append(
                    token,
                    0.0,
                    TextFormat {
                        font_id: FontId::monospace(size),
                        color: Color32::from_rgb(
                            style.foreground.r,
                            style.foreground.g,
                            style.foreground.b,
                        ),
                        ..Default::default()
                    },
                );
            }
        }
        // HighlightLines is never allowed to normalize whitespace or line endings.
        debug_assert_eq!(job.text, text);
        Ok(job)
    }
}

struct CachedLayout {
    syntax: String,
    size: f32,
    dark: bool,
    scale: f32,
    galley: Arc<egui::Galley>,
    error: Option<String>,
    matched: Option<Range<usize>>,
}

#[derive(Default)]
pub struct HighlightCache {
    regions: HashMap<Uuid, CachedLayout>,
}

impl HighlightCache {
    pub fn remove(&mut self, region: Uuid) {
        self.regions.remove(&region);
    }

    pub fn error(&self, region: Uuid) -> Option<&str> {
        self.regions.get(&region).and_then(|c| c.error.as_deref())
    }

    pub fn layout(
        &mut self,
        ui: &egui::Ui,
        highlighter: &Highlighter,
        region: Uuid,
        text: &str,
        options: HighlightOptions<'_>,
    ) -> Arc<egui::Galley> {
        let HighlightOptions {
            syntax,
            size,
            matched,
        } = options;
        let dark = ui.visuals().dark_mode;
        let scale = ui.ctx().pixels_per_point();
        if let Some(cached) = self.regions.get(&region)
            && cached.galley.job.text == text
            && cached.syntax == syntax
            && cached.size == size
            && cached.dark == dark
            && cached.scale == scale
            && cached.matched == matched
        {
            return cached.galley.clone();
        }
        let (mut job, error) = match highlighter.layout_job(text, syntax, size, dark) {
            Ok(job) => (job, None),
            Err(error) => (
                LayoutJob::simple(
                    text.into(),
                    FontId::monospace(size),
                    ui.visuals().text_color(),
                    f32::INFINITY,
                ),
                Some(error),
            ),
        };
        if let Some(range) = &matched {
            let color = if dark {
                Color32::from_rgb(65, 78, 110)
            } else {
                Color32::from_rgb(250, 222, 145)
            };
            let mut sections = Vec::new();
            for section in job.sections {
                let overlap = section.byte_range.start.max(range.start.into())
                    ..section.byte_range.end.min(range.end.into());
                if overlap.start >= overlap.end {
                    sections.push(section);
                    continue;
                }
                if section.byte_range.start < overlap.start {
                    sections.push(egui::text::LayoutSection {
                        byte_range: section.byte_range.start..overlap.start,
                        ..section.clone()
                    });
                }
                let mut highlighted = section.clone();
                highlighted.byte_range = overlap.clone();
                highlighted.format.background = color;
                sections.push(highlighted);
                if overlap.end < section.byte_range.end {
                    sections.push(egui::text::LayoutSection {
                        byte_range: overlap.end..section.byte_range.end,
                        ..section
                    });
                }
            }
            job.sections = sections;
        }
        let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
        self.regions.insert(
            region,
            CachedLayout {
                syntax: syntax.into(),
                size,
                dark,
                scale,
                galley: galley.clone(),
                error,
                matched,
            },
        );
        galley
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn plain_empty_and_large_fallback_do_not_load_syntax_resources() {
        let h = Highlighter::default();
        assert_eq!(h.languages().len(), 19);
        h.layout_job("笔记😀\r\n", "plain", 15., true).unwrap();
        h.layout_job("", "rust", 15., true).unwrap();
        h.layout_job(&"x".repeat(MAX_HIGHLIGHT_BYTES + 1), "json", 15., false)
            .unwrap();
        assert!(h.syntaxes.get().is_none());
        assert!(h.themes.get().is_none());
        h.layout_job("fn main() {}", "rust", 15., true).unwrap();
        assert!(h.syntaxes.get().is_some());
        assert!(h.themes.get().is_some());
        // The lightweight plain-text path uses exactly the existing theme colors.
        for (dark, name) in [(true, "base16-ocean.dark"), (false, "InspiredGitHub")] {
            let job = h.layout_job("text", "plain", 15., dark).unwrap();
            let c = h.themes.get().unwrap().themes[name]
                .settings
                .foreground
                .unwrap();
            assert_eq!(
                job.sections[0].format.color,
                Color32::from_rgb(c.r, c.g, c.b)
            );
        }
    }
}
