use tabloom::model::{Workspace, document::whole_lines};
#[test]
fn complete_line_validation_handles_crlf_unicode_eof_and_partial_lines() {
    let s = "first\r\n中文😀\nlast";
    assert_eq!(whole_lines(s, 0..5), Some(0..7));
    assert_eq!(whole_lines(s, 7..17), Some(7..18));
    assert_eq!(whole_lines(s, 18..22), Some(18..22));
    for r in [1..5, 0..2, 7..8, 0..6, 3..3] {
        assert!(whole_lines(s, r).is_none());
    }
    assert_eq!(whole_lines("\n", 0..1), Some(0..1));
    assert_eq!(whole_lines("one\rtwo", 0..3), Some(0..4));
}
#[test]
fn extracting_a_middle_line_retains_source_ids_order_languages_and_every_byte() {
    let mut w = Workspace::new("doc", "region");
    let id = w.regions[0].id;
    w.regions[0].text = "before\r\n中文😀\t\nlast".into();
    w.regions[0].syntax_id = "rust".into();
    let old = w.text();
    let selected = w.extract_lines(8..19, "region").unwrap();
    assert_eq!(w.text(), old);
    assert_eq!(w.regions.len(), 3);
    assert_eq!(w.regions[0].id, id);
    assert_eq!(w.regions[1].id, selected);
    assert_ne!(w.regions[2].id, id);
    assert!(w.regions.iter().all(|r| r.syntax_id == "rust"));
    assert!(w.extract_lines(1..3, "region").is_err());
    assert_eq!(w.text(), old);
}
#[test]
fn selection_can_cross_existing_spans_without_rewriting_text() {
    let mut w = Workspace::new("doc", "region");
    w.regions[0].text = "a\nb\nc\nd".into();
    w.extract_lines(2..3, "region").unwrap();
    assert_eq!(w.regions.len(), 3);
    w.extract_lines(0..6, "region").unwrap();
    assert_eq!(w.text(), "a\nb\nc\nd");
    assert_eq!(w.regions.len(), 2);
    assert_eq!(w.regions[0].text, "a\nb\nc\n");
}
#[test]
fn ordinary_edit_and_multiline_replace_preserve_other_language_spans() {
    let mut w = Workspace::new("doc", "region");
    w.regions[0].text = "cat\ncat\ncat".into();
    w.extract_lines(4..7, "region").unwrap();
    w.regions[1].syntax_id = "sql".into();
    w.apply_text("dog\ndog\ndog");
    assert_eq!(w.text(), "dog\ndog\ndog");
    assert_eq!(w.regions[1].syntax_id, "sql");
    assert_eq!(w.regions[1].text, "dog\n");
    w.apply_text("\tdog\n\tdog\n\tdog");
    assert_eq!(w.regions[1].text, "\tdog\n");
}
#[test]
fn cursor_hint_assigns_repeated_characters_to_the_actual_span() {
    let mut w = Workspace::new("doc", "region");
    w.regions[0].text = "aaa".into();
    w.add_region("region");
    w.regions[1].text = "aaa".into();
    w.active_region = w.regions[0].id;
    w.apply_text_at("aaaaaaa", Some(1..1));
    assert_eq!(w.regions[0].text, "aaaa");
    assert_eq!(w.regions[1].text, "aaa");
}
