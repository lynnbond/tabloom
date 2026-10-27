//! A workspace is displayed as one document; regions are ordered syntax spans.
use super::{EditorRegion, Workspace};
use std::ops::Range;
use uuid::Uuid;

/// Accept whole lines, including the usual Home..End selection without its newline.
/// Never expand a partial line or split a CRLF pair / UTF-8 character.
pub fn whole_lines(text: &str, mut range: Range<usize>) -> Option<Range<usize>> {
    if range.is_empty() || text.get(range.clone()).is_none() {
        return None;
    }
    let start_ok = range.start == 0
        || text.as_bytes()[range.start - 1] == b'\n'
        || (text.as_bytes()[range.start - 1] == b'\r'
            && text.as_bytes().get(range.start) != Some(&b'\n'));
    if !start_ok {
        return None;
    }
    if text.as_bytes().get(range.end) == Some(&b'\r') {
        range.end += 1;
        if text.as_bytes().get(range.end) == Some(&b'\n') {
            range.end += 1;
        }
    } else if text.as_bytes().get(range.end) == Some(&b'\n') {
        if range.end > 0 && text.as_bytes()[range.end - 1] == b'\r' {
            return None;
        }
        range.end += 1;
    } else if range.end < text.len()
        && text.as_bytes()[range.end - 1] != b'\n'
        && text.as_bytes()[range.end - 1] != b'\r'
    {
        return None;
    }
    Some(range)
}

impl Workspace {
    pub fn text(&self) -> String {
        self.regions.iter().map(|r| r.text.as_str()).collect()
    }
    pub fn region_offset(&self, id: Uuid) -> Option<usize> {
        let mut offset = 0;
        for r in &self.regions {
            if r.id == id {
                return Some(offset);
            }
            offset += r.text.len();
        }
        None
    }
    pub fn region_at(&self, byte: usize) -> Uuid {
        let mut offset = 0;
        for r in &self.regions {
            offset += r.text.len();
            if byte < offset {
                return r.id;
            }
        }
        self.regions.last().expect("workspace has a region").id
    }
    /// Map edits onto syntax spans. A cursor hint disambiguates repeated characters.
    pub fn apply_text(&mut self, new: &str) {
        self.apply_text_at(new, None);
    }
    pub fn apply_text_at(&mut self, new: &str, hint: Option<Range<usize>>) {
        let old = self.text();
        if old == new {
            return;
        }
        if let Some(range) = hint
            && old.get(range.clone()).is_some()
        {
            let added = new
                .len()
                .checked_add(range.len())
                .and_then(|n| n.checked_sub(old.len()));
            if let Some(added) = added
                && let Some(end) = range.start.checked_add(added)
                && new.get(..range.start) == old.get(..range.start)
                && new.get(end..) == old.get(range.end..)
            {
                self.apply_edits(&[(range.clone(), new[range.start..end].into())], true);
                return;
            }
        }
        let start = old
            .chars()
            .zip(new.chars())
            .take_while(|(a, b)| a == b)
            .map(|(c, _)| c.len_utf8())
            .sum::<usize>();
        let suffix = old[start..]
            .chars()
            .rev()
            .zip(new[start..].chars().rev())
            .take_while(|(a, b)| a == b)
            .map(|(c, _)| c.len_utf8())
            .sum::<usize>();
        let diff = similar::TextDiff::from_chars(
            &old[start..old.len() - suffix],
            &new[start..new.len() - suffix],
        );
        let mut edits = Vec::new();
        let mut pending: Option<(Range<usize>, String)> = None;
        let mut offset = start;
        for change in diff.iter_all_changes() {
            match change.tag() {
                similar::ChangeTag::Equal => {
                    if let Some(edit) = pending.take() {
                        edits.push(edit);
                    }
                    offset += change.value().len();
                }
                similar::ChangeTag::Delete => {
                    let edit = pending.get_or_insert_with(|| (offset..offset, String::new()));
                    offset += change.value().len();
                    edit.0.end = offset;
                }
                similar::ChangeTag::Insert => {
                    pending
                        .get_or_insert_with(|| (offset..offset, String::new()))
                        .1
                        .push_str(change.value());
                }
            }
        }
        if let Some(edit) = pending {
            edits.push(edit);
        }
        self.apply_edits(&edits, false);
        debug_assert_eq!(self.text(), new);
    }
    fn apply_edits(&mut self, edits: &[(Range<usize>, String)], prefer_active: bool) {
        let mut offsets = Vec::new();
        let mut offset = 0;
        for r in &self.regions {
            offsets.push(offset);
            offset += r.text.len();
        }
        let targets: Vec<_> = edits
            .iter()
            .map(|(range, _)| {
                if prefer_active
                    && let Some(index) = self.regions.iter().enumerate().position(|(i, r)| {
                        r.id == self.active_region
                            && range.start >= offsets[i]
                            && range.start <= offsets[i] + r.text.len()
                    })
                {
                    return index;
                }
                self.regions
                    .iter()
                    .enumerate()
                    .position(|(i, r)| range.start < offsets[i] + r.text.len())
                    .unwrap_or(self.regions.len() - 1)
            })
            .collect();
        for (index, r) in self.regions.iter_mut().enumerate() {
            let mut output = String::new();
            let mut cursor = 0;
            let offset = offsets[index];
            let len = r.text.len();
            for ((range, inserted), target) in edits.iter().zip(&targets) {
                let start = range.start.saturating_sub(offset).min(len);
                let end = range.end.saturating_sub(offset).min(len);
                if start < end || *target == index {
                    output.push_str(&r.text[cursor..start]);
                    if *target == index {
                        output.push_str(inserted);
                    }
                    cursor = end;
                }
            }
            output.push_str(&r.text[cursor..]);
            if output != r.text {
                r.text = output;
                r.touch();
            }
        }
        self.touch();
    }

    /// Convert a complete-line selection into one syntax span without changing any bytes.
    pub fn extract_lines(&mut self, range: Range<usize>, prefix: &str) -> Result<Uuid, String> {
        let text = self.text();
        let range = whole_lines(&text, range)
            .ok_or_else(|| "请至少完整选中一行；无需选中行尾换行符。".to_owned())?;
        let mut offset = 0;
        let sources: Vec<_> = self
            .regions
            .iter()
            .filter(|r| {
                let start = offset;
                offset += r.text.len();
                start < range.end && offset > range.start
            })
            .collect();
        let first = sources.first().ok_or("选区为空")?;
        if sources.iter().any(|r| r.file_path != first.file_path) {
            return Err("选区跨越不同文件来源，请分别设置。".into());
        }
        if sources.len() == 1 && first.text.len() == range.len() {
            self.active_region = first.id;
            return Ok(first.id);
        }
        let mut selected =
            EditorRegion::new(self.id, format!("{prefix} {}", self.next_region_number));
        self.next_region_number += 1;
        selected.text = text[range.clone()].into();
        selected.syntax_id = first.syntax_id.clone();
        selected.file_path = first.file_path.clone();
        selected.file_state = first.file_state.clone();
        selected.touch();
        let selected_id = selected.id;
        let mut result = Vec::new();
        let mut inserted = false;
        offset = 0;
        for original in &self.regions {
            let start = offset;
            let end = start + original.text.len();
            offset = end;
            if end <= range.start || start >= range.end {
                result.push(original.clone());
                continue;
            }
            let left = range.start.saturating_sub(start).min(original.text.len());
            let right = range.end.saturating_sub(start).min(original.text.len());
            if left > 0 {
                let mut r = original.clone();
                r.text = original.text[..left].into();
                r.touch();
                result.push(r);
            }
            if !inserted {
                result.push(selected.clone());
                inserted = true;
            }
            if right < original.text.len() {
                let mut r = original.clone();
                if left > 0 {
                    r.id = Uuid::new_v4();
                    r.title = format!("{prefix} {}", self.next_region_number);
                    self.next_region_number += 1;
                }
                r.text = original.text[right..].into();
                r.touch();
                result.push(r);
            }
        }
        self.regions = result;
        self.active_region = selected_id;
        self.touch();
        debug_assert_eq!(self.text(), text);
        Ok(selected_id)
    }
}
