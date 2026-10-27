use tabloom::model::{Document, MAX_REGION_HEIGHT, MIN_REGION_HEIGHT, Workspace, WorkspaceStatus};
use uuid::Uuid;

#[test]
fn regions_have_stable_ids_and_ownership() {
    let mut workspace = Workspace::new("A", "Region");
    let first = workspace.regions[0].id;
    let second = workspace.add_region("Region");
    assert_ne!(first, second);
    assert!(
        workspace
            .regions
            .iter()
            .all(|r| r.workspace_id == workspace.id)
    );
    assert_eq!(workspace.active_region, second);
    assert!(workspace.move_region(second, -1));
    assert_eq!(workspace.regions[0].id, second);
    assert_eq!(workspace.active_region, second);
    assert!(!workspace.move_region(second, -1));
    assert!(!workspace.move_region(first, 1));
    assert!(!workspace.move_region(Uuid::new_v4(), 1));
}

#[test]
fn deletion_keeps_a_valid_active_editable_region() {
    let mut workspace = Workspace::new("A", "Region");
    let first = workspace.regions[0].id;
    let second = workspace.add_region("Region");
    assert!(workspace.remove_region(second, "Region"));
    assert_eq!(workspace.active_region, first);
    assert!(workspace.remove_region(first, "Region"));
    assert_eq!(workspace.regions.len(), 1);
    assert_eq!(workspace.active_region, workspace.regions[0].id);
    assert_eq!(workspace.regions[0].workspace_id, workspace.id);
    assert!(workspace.regions[0].text.is_empty());
    assert_ne!(workspace.regions[0].id, first);
    assert!(!workspace.remove_region(first, "Region"));
}

#[test]
fn text_and_language_remain_independent_when_moving() {
    let mut workspace = Workspace::new("A", "Region");
    let original = "中文😀\t  x\r\nnext\nlast\r";
    workspace.regions[0].text = original.into();
    workspace.regions[0].syntax_id = "json".into();
    let second = workspace.add_region("Region");
    workspace.regions[1].text = "SELECT 1;".into();
    workspace.regions[1].syntax_id = "sql".into();
    workspace.move_region(second, -1);
    assert_eq!(workspace.regions[0].text, "SELECT 1;");
    assert_eq!(workspace.regions[0].syntax_id, "sql");
    assert_eq!(workspace.regions[1].text, original);
    assert_eq!(workspace.regions[1].syntax_id, "json");
}

#[test]
fn close_and_reopen_preserve_content_and_other_tabs() {
    let mut doc = Document::default();
    let a = doc.create_workspace("Workspace", "Region");
    doc.active_mut().unwrap().regions[0].text = "A".into();
    let b = doc.create_workspace("Workspace", "Region");
    doc.active_mut().unwrap().regions[0].text = "B".into();
    assert!(doc.close(b));
    assert_eq!(doc.active().unwrap().id, a);
    assert!(!doc.select(b));
    assert_eq!(doc.active().unwrap().regions[0].text, "A");
    assert!(doc.reopen(b));
    assert_eq!(doc.active().unwrap().regions[0].text, "B");
    assert_eq!(doc.workspaces[1].status, WorkspaceStatus::Open);
    doc.close(b);
    doc.close(a);
    assert!(doc.active().is_none());
    assert!(doc.reopen(b));
    assert_eq!(doc.active_workspace, Some(b));
}

#[test]
fn cycling_skips_closed_tabs_and_wraps() {
    let mut doc = Document::default();
    let a = doc.create_workspace("Workspace", "Region");
    let b = doc.create_workspace("Workspace", "Region");
    let c = doc.create_workspace("Workspace", "Region");
    doc.close(b);
    doc.cycle(1);
    assert_eq!(doc.active_workspace, Some(a));
    doc.cycle(-1);
    assert_eq!(doc.active_workspace, Some(c));
    assert_ne!(doc.workspaces[0].title, doc.workspaces[2].title);
}

#[test]
fn heights_are_finite_and_bounded() {
    let mut workspace = Workspace::new("A", "Region");
    let region = &mut workspace.regions[0];
    region.resize(356.0);
    assert_eq!(region.height, 356.0);
    region.resize(f32::NAN);
    region.resize(f32::INFINITY);
    assert_eq!(region.height, 356.0);
    region.resize(-10.0);
    assert_eq!(region.height, MIN_REGION_HEIGHT);
    region.resize(1.0e8);
    assert_eq!(region.height, MAX_REGION_HEIGHT);
}

#[test]
fn document_serialization_preserves_every_field() {
    let mut doc = Document::default();
    let a = doc.create_workspace("Workspace", "Region");
    let workspace = doc.active_mut().unwrap();
    workspace.regions[0].text = "中文\r\n😀\t x\n\r".into();
    workspace.regions[0].syntax_id = "json".into();
    let second = workspace.add_region("Region");
    workspace.regions[1].resize(456.5);
    workspace.regions[1].syntax_id = "rust".into();
    workspace.move_region(second, -1);
    doc.create_workspace("Workspace", "Region");
    doc.close(a);
    let json = serde_json::to_string(&doc).unwrap();
    let restored: Document = serde_json::from_str(&json).unwrap();
    assert_eq!(restored, doc);
}
