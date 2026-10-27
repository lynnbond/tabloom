use tabloom::editor::{
    clipboard::selected_text,
    search::{SearchOptions, SearchState, find_matches, replace_matches},
};
use uuid::Uuid;

#[test]
fn literal_matching_never_interprets_regex_metacharacters() {
    let text = "a.b $1 [x] aXb a.b";
    assert_eq!(
        find_matches(text, "a.b", SearchOptions::default()).unwrap(),
        vec![0..3, 15..18]
    );
    assert_eq!(
        find_matches(text, "[x]", SearchOptions::default()).unwrap(),
        vec![7..10]
    );
    assert!(
        find_matches(text, "", SearchOptions::default())
            .unwrap()
            .is_empty()
    );
}

#[test]
fn unicode_case_folding_uses_original_byte_offsets() {
    let text = "K k K 中文😀";
    let matches = find_matches(text, "k", SearchOptions::default()).unwrap();
    assert_eq!(
        matches.iter().map(|r| &text[r.clone()]).collect::<Vec<_>>(),
        ["K", "k", "K"]
    );
    let options = SearchOptions {
        case_sensitive: true,
        whole_word: false,
    };
    assert_eq!(
        find_matches(text, "k", options)
            .unwrap()
            .iter()
            .map(|r| &text[r.clone()])
            .collect::<Vec<_>>(),
        ["k"]
    );
    let matches = find_matches(text, "中文😀", options).unwrap();
    assert_eq!(&text[matches[0].clone()], "中文😀");
}

#[test]
fn whole_word_respects_unicode_letters_underscores_and_combining_marks() {
    let text = "cat cats _cat cat_ cat. 猫 cat\u{301} cat";
    let options = SearchOptions {
        case_sensitive: false,
        whole_word: true,
    };
    let matches = find_matches(text, "cat", options).unwrap();
    assert_eq!(
        matches.iter().map(|r| &text[r.clone()]).collect::<Vec<_>>(),
        ["cat", "cat", "cat"]
    );
    assert!(
        find_matches("中文笔记", "中文", options)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        find_matches("中文 笔记", "中文", options).unwrap(),
        vec![0..6]
    );
}

#[test]
fn replacement_preserves_whitespace_and_treats_dollar_and_backslash_literally() {
    let text = "\t中文😀\r\nA A\n\r";
    let matches = find_matches(text, "A", SearchOptions::default()).unwrap();
    assert_eq!(
        replace_matches(text, &matches, "$1\\n").unwrap(),
        "\t中文😀\r\n$1\\n $1\\n\n\r"
    );
    assert_eq!(
        replace_matches(text, &matches, "").unwrap(),
        "\t中文😀\r\n \n\r"
    );
    assert!(replace_matches("中文", std::slice::from_ref(&(1..3)), "x").is_none());
    assert!(replace_matches("hello", &[0..3, 2..4], "x").is_none());
}

#[test]
fn navigation_wraps_and_cache_changes_region_and_query_without_stale_ranges() {
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();
    let mut search = SearchState::default();
    search.query = "one".into();
    search.refresh(a, "one two one");
    assert_eq!(search.current_index(), Some(0));
    assert_eq!(search.navigate(false), Some(8..11));
    assert_eq!(search.navigate(false), Some(0..3));
    assert_eq!(search.navigate(true), Some(8..11));
    search.refresh(b, "no match");
    assert!(search.current.is_none());
    search.query = "猫".into();
    search.refresh(a, "猫 猫");
    assert_eq!(search.matches, vec![0..3, 4..7]);
    assert_eq!(search.select_from(4), Some(4..7));
    search.refresh(a, "猫");
    assert_eq!(search.current, Some(0..3));
}

#[test]
fn selection_extraction_is_raw_text_and_uses_character_indices() {
    let source = "标题\n\t{\"中文😀\": true}\r\n";
    assert_eq!(
        selected_text(source, 3..source.chars().count()),
        Some("\t{\"中文😀\": true}\r\n")
    );
    assert_eq!(selected_text(source, 0..0), Some(""));
    assert!(selected_text(source, 0..10000).is_none());
    assert!(selected_text(source, std::ops::Range { start: 3, end: 1 }).is_none());
}
