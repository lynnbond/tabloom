use std::sync::LazyLock;
use tabloom::editor::highlighting::Highlighter;

static HIGHLIGHTER: LazyLock<Highlighter> = LazyLock::new(Highlighter::default);

#[test]
fn requested_code_languages_have_real_grammars() {
    for id in [
        "plain",
        "json",
        "yaml",
        "toml",
        "markdown",
        "rust",
        "go",
        "python",
        "javascript",
        "typescript",
        "html",
        "css",
        "sql",
        "shell",
        "powershell",
        "c",
        "cpp",
        "java",
        "xml",
    ] {
        assert!(
            HIGHLIGHTER.languages().iter().any(|l| l.id == id),
            "missing {id}"
        );
    }
    assert!(!HIGHLIGHTER.languages().iter().any(|l| l.id == "log"));
}

#[test]
fn highlighting_preserves_unicode_whitespace_and_line_endings() {
    let text = "{\r\n\t\"中文😀\": \" x \"\n}\r";
    for language in HIGHLIGHTER.languages() {
        for dark in [false, true] {
            let job = HIGHLIGHTER
                .layout_job(text, &language.id, 15.0, dark)
                .unwrap();
            assert_eq!(job.text, text, "language {}", language.id);
        }
    }
    assert_eq!(
        HIGHLIGHTER
            .layout_job("", "plain", 15.0, true)
            .unwrap()
            .text,
        ""
    );
}

#[test]
fn real_json_highlighting_produces_multiple_colors() {
    let job = HIGHLIGHTER
        .layout_job("{\"key\": 123, \"ok\": true}", "json", 15.0, true)
        .unwrap();
    let colors: std::collections::HashSet<_> =
        job.sections.iter().map(|s| s.format.color).collect();
    assert!(colors.len() > 1);
    assert!(
        HIGHLIGHTER
            .layout_job("hello", "missing-language", 15.0, true)
            .is_err()
    );
}

#[test]
fn large_code_and_long_lines_use_plain_layout_without_changing_source_or_language() {
    use tabloom::editor::highlighting::highlighting_limited;
    let source = "fn main() { println!(\"中文😀\"); }\r\n".repeat(9000);
    assert!(highlighting_limited(&source, "rust"));
    for dark in [false, true] {
        let job = HIGHLIGHTER.layout_job(&source, "rust", 15., dark).unwrap();
        assert_eq!(job.text, source);
        assert_eq!(job.sections.len(), 1);
    }
    let long_line = "x".repeat(20000);
    assert!(highlighting_limited(&long_line, "json"));
    assert!(!highlighting_limited(&source, "plain"));
    assert!(!highlighting_limited("fn main() {}", "rust"));
    assert!(
        HIGHLIGHTER
            .layout_job(&source, "invalid", 15., true)
            .is_err()
    );
    assert!(HIGHLIGHTER.languages().iter().any(|l| l.id == "rust"));
}
