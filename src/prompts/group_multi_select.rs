use std::io;

use console::{Key, Term};

use crate::{
    theme::{render::TermThemeRenderer, GroupState, SimpleTheme, Theme},
    Result,
};

/// Represents the state of an item in GroupMultiSelect.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ItemState {
    /// Normal item - can be focused and selected
    #[default]
    Normal,
    /// Disabled item - cursor skips, cannot be selected
    Disabled {
        /// Reason why the item is disabled
        reason: String,
    },
    /// Warning item - can be focused and selected, shows warning message
    Warning {
        /// Warning message to display
        message: String,
    },
}

pub struct Group<T> {
    pub label: String,
    pub items: Vec<T>,
    pub states: Vec<ItemState>,
}

impl<T> Group<T> {
    pub fn new(label: impl Into<String>, items: Vec<T>) -> Self {
        let len = items.len();
        Self {
            label: label.into(),
            items,
            states: vec![ItemState::Normal; len],
        }
    }

    pub fn with_states(label: impl Into<String>, items: Vec<T>, states: Vec<ItemState>) -> Self {
        Self {
            label: label.into(),
            items,
            states,
        }
    }
}

#[derive(Clone, Copy, Default)]
struct Cursor {
    group_idx: usize,
    item_idx: Option<usize>,
}

pub struct GroupMultiSelect<'a, T> {
    groups: Vec<Group<T>>,
    defaults: Vec<Vec<bool>>,
    prompt: String,
    report: bool,
    clear: bool,
    max_length: Option<usize>,
    theme: &'a dyn Theme,
}

impl<T> Default for GroupMultiSelect<'_, T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a, T> GroupMultiSelect<'a, T> {
    pub fn new() -> GroupMultiSelect<'static, T> {
        GroupMultiSelect {
            groups: Vec::new(),
            defaults: Vec::new(),
            prompt: String::new(),
            report: true,
            clear: true,
            max_length: None,
            theme: &SimpleTheme,
        }
    }

    pub fn with_theme(self, theme: &'a dyn Theme) -> GroupMultiSelect<'a, T> {
        GroupMultiSelect {
            groups: self.groups,
            defaults: self.defaults,
            prompt: self.prompt,
            report: self.report,
            clear: self.clear,
            max_length: self.max_length,
            theme,
        }
    }

    pub fn group(mut self, label: impl Into<String>, items: Vec<T>) -> Self {
        self.groups.push(Group::new(label, items));
        self
    }

    pub fn group_with_states(
        mut self,
        label: impl Into<String>,
        items: Vec<(T, ItemState)>,
    ) -> Self {
        let (items, states): (Vec<T>, Vec<ItemState>) = items.into_iter().unzip();
        self.groups.push(Group::with_states(label, items, states));
        self
    }

    pub fn defaults(mut self, defaults: Vec<Vec<bool>>) -> Self {
        self.defaults = defaults;
        self
    }

    pub fn with_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.prompt = prompt.into();
        self
    }

    pub fn report(mut self, val: bool) -> Self {
        self.report = val;
        self
    }

    pub fn clear(mut self, val: bool) -> Self {
        self.clear = val;
        self
    }

    pub fn max_length(mut self, val: usize) -> Self {
        self.max_length = Some(val);
        self
    }
}

impl<T: ToString> GroupMultiSelect<'_, T> {
    pub fn interact(self) -> Result<Vec<Vec<usize>>> {
        self.interact_on(&Term::stderr())
    }

    pub fn interact_on(self, term: &Term) -> Result<Vec<Vec<usize>>> {
        self._interact_on(term, false)?
            .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "Cancelled").into())
    }

    pub fn interact_opt(self) -> Result<Option<Vec<Vec<usize>>>> {
        self.interact_on_opt(&Term::stderr())
    }

    pub fn interact_on_opt(self, term: &Term) -> Result<Option<Vec<Vec<usize>>>> {
        self._interact_on(term, true)
    }

    fn _interact_on(self, term: &Term, allow_quit: bool) -> Result<Option<Vec<Vec<usize>>>> {
        if self.groups.is_empty() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "No groups added").into());
        }

        let mut checked: Vec<Vec<bool>> = self
            .groups
            .iter()
            .enumerate()
            .map(|(g_idx, group)| {
                (0..group.items.len())
                    .map(|i_idx| {
                        self.defaults
                            .get(g_idx)
                            .and_then(|g| g.get(i_idx))
                            .copied()
                            .unwrap_or(false)
                    })
                    .collect()
            })
            .collect();

        let mut cursor = Cursor::default();
        let total_rows = self.total_rows();

        if total_rows == 0 {
            return Ok(Some(vec![vec![]; self.groups.len()]));
        }

        let mut render = TermThemeRenderer::new(term, self.theme);
        let mut page_offset = 0usize;
        let capacity = self
            .max_length
            .unwrap_or(usize::MAX)
            .min(term.size().0 as usize);

        term.hide_cursor()?;

        loop {
            self.render(&mut render, &checked, cursor, page_offset, capacity)?;

            match term.read_key()? {
                Key::ArrowDown | Key::Char('j') => {
                    cursor = self.move_cursor_down(cursor);
                    page_offset = self.adjust_page_offset(cursor, page_offset, capacity);
                }
                Key::ArrowUp | Key::Char('k') => {
                    cursor = self.move_cursor_up(cursor);
                    page_offset = self.adjust_page_offset(cursor, page_offset, capacity);
                }
                Key::Char(' ') => {
                    self.toggle(&mut checked, cursor);
                }
                Key::Char('a') => {
                    let all_selectable_selected = self
                        .groups
                        .iter()
                        .zip(checked.iter())
                        .flat_map(|(group, group_checked)| {
                            group.states.iter().zip(group_checked.iter())
                        })
                        .filter(|(state, _)| !matches!(state, ItemState::Disabled { .. }))
                        .all(|(_, &is_checked)| is_checked);
                    let new_state = !all_selectable_selected;
                    for (group, group_checked) in self.groups.iter().zip(checked.iter_mut()) {
                        for (idx, state) in group.states.iter().enumerate() {
                            if !matches!(state, ItemState::Disabled { .. }) {
                                group_checked[idx] = new_state;
                            }
                        }
                    }
                }
                Key::Enter => {
                    if self.clear {
                        render.clear()?;
                    }

                    if self.report {
                        self.render_report(&mut render, &checked)?;
                    }

                    term.show_cursor()?;
                    term.flush()?;

                    return Ok(Some(self.build_result(&checked)));
                }
                Key::Escape | Key::Char('q') if allow_quit => {
                    if self.clear {
                        render.clear()?;
                    }
                    term.show_cursor()?;
                    term.flush()?;
                    return Ok(None);
                }
                _ => {}
            }

            render.clear()?;
        }
    }

    fn total_rows(&self) -> usize {
        self.groups.iter().map(|g| 1 + g.items.len()).sum()
    }

    fn cursor_to_flat(&self, cursor: Cursor) -> usize {
        let mut flat = 0;
        for g_idx in 0..cursor.group_idx {
            flat += 1 + self.groups[g_idx].items.len();
        }
        flat += match cursor.item_idx {
            None => 0,
            Some(i) => 1 + i,
        };
        flat
    }

    fn flat_to_cursor(&self, flat_idx: usize) -> Cursor {
        let mut remaining = flat_idx;
        for (g_idx, group) in self.groups.iter().enumerate() {
            if remaining == 0 {
                return Cursor {
                    group_idx: g_idx,
                    item_idx: None,
                };
            }
            remaining -= 1;
            if remaining < group.items.len() {
                return Cursor {
                    group_idx: g_idx,
                    item_idx: Some(remaining),
                };
            }
            remaining -= group.items.len();
        }
        Cursor::default()
    }

    fn is_item_disabled(&self, cursor: Cursor) -> bool {
        match cursor.item_idx {
            None => false,
            Some(item_idx) => {
                matches!(
                    self.groups[cursor.group_idx].states.get(item_idx),
                    Some(ItemState::Disabled { .. })
                )
            }
        }
    }

    fn move_cursor_down(&self, cursor: Cursor) -> Cursor {
        let total = self.total_rows();
        let mut flat = self.cursor_to_flat(cursor);

        loop {
            if flat + 1 >= total {
                return cursor;
            }
            flat += 1;
            let new_cursor = self.flat_to_cursor(flat);
            if !self.is_item_disabled(new_cursor) {
                return new_cursor;
            }
        }
    }

    fn move_cursor_up(&self, cursor: Cursor) -> Cursor {
        let mut flat = self.cursor_to_flat(cursor);

        loop {
            if flat == 0 {
                return cursor;
            }
            flat -= 1;
            let new_cursor = self.flat_to_cursor(flat);
            if !self.is_item_disabled(new_cursor) {
                return new_cursor;
            }
        }
    }

    fn toggle(&self, checked: &mut [Vec<bool>], cursor: Cursor) {
        match cursor.item_idx {
            None => {
                let group = &self.groups[cursor.group_idx];
                if group.items.is_empty() {
                    return;
                }
                let selectable_all_selected = group
                    .states
                    .iter()
                    .zip(checked[cursor.group_idx].iter())
                    .filter(|(state, _)| !matches!(state, ItemState::Disabled { .. }))
                    .all(|(_, &is_checked)| is_checked);
                let new_state = !selectable_all_selected;
                for (idx, state) in group.states.iter().enumerate() {
                    if !matches!(state, ItemState::Disabled { .. }) {
                        checked[cursor.group_idx][idx] = new_state;
                    }
                }
            }
            Some(item_idx) => {
                if !matches!(
                    self.groups[cursor.group_idx].states.get(item_idx),
                    Some(ItemState::Disabled { .. })
                ) {
                    checked[cursor.group_idx][item_idx] = !checked[cursor.group_idx][item_idx];
                }
            }
        }
    }

    fn group_state(checked: &[bool]) -> GroupState {
        let selected_count = checked.iter().filter(|&&b| b).count();
        let total = checked.len();
        if total == 0 || selected_count == 0 {
            GroupState::None
        } else if selected_count == total {
            GroupState::All
        } else {
            GroupState::Partial
        }
    }

    fn adjust_page_offset(&self, cursor: Cursor, current_offset: usize, capacity: usize) -> usize {
        let flat = self.cursor_to_flat(cursor);
        let total = self.total_rows();

        if capacity >= total {
            return 0;
        }

        if flat < current_offset {
            flat
        } else if flat >= current_offset + capacity {
            flat - capacity + 1
        } else {
            current_offset
        }
    }

    fn render(
        &self,
        render: &mut TermThemeRenderer,
        checked: &[Vec<bool>],
        cursor: Cursor,
        page_offset: usize,
        capacity: usize,
    ) -> Result<()> {
        let total = self.total_rows();
        let paging_info = if capacity < total {
            let total_pages = (total + capacity - 1) / capacity;
            let current_page = page_offset / capacity + 1;
            Some((current_page, total_pages))
        } else {
            None
        };
        render.group_multi_select_prompt(&self.prompt, paging_info)?;
        let visible_end = (page_offset + capacity).min(total);

        for flat_idx in page_offset..visible_end {
            let pos = self.flat_to_cursor(flat_idx);
            let is_active = pos.group_idx == cursor.group_idx && pos.item_idx == cursor.item_idx;

            match pos.item_idx {
                None => {
                    let state = Self::group_state(&checked[pos.group_idx]);
                    render.group_multi_select_header(
                        &self.groups[pos.group_idx].label,
                        state,
                        is_active,
                    )?;
                }
                Some(item_idx) => {
                    let item_text = self.groups[pos.group_idx].items[item_idx].to_string();
                    let is_checked = checked[pos.group_idx][item_idx];
                    let state = &self.groups[pos.group_idx].states[item_idx];

                    match state {
                        ItemState::Normal => {
                            render.group_multi_select_item(&item_text, is_checked, is_active)?;
                        }
                        ItemState::Disabled { reason } => {
                            render
                                .group_multi_select_item_disabled(&item_text, reason, is_active)?;
                        }
                        ItemState::Warning { message } => {
                            render.group_multi_select_item_warning(
                                &item_text, message, is_checked, is_active,
                            )?;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn render_report(&self, render: &mut TermThemeRenderer, checked: &[Vec<bool>]) -> Result<()> {
        let selected: Vec<String> = self
            .groups
            .iter()
            .zip(checked.iter())
            .flat_map(|(group, group_checked)| {
                group
                    .items
                    .iter()
                    .zip(group_checked.iter())
                    .filter(|(_, &is_checked)| is_checked)
                    .map(|(item, _)| item.to_string())
            })
            .collect();

        let selected_refs: Vec<&str> = selected.iter().map(|s| s.as_str()).collect();
        render.group_multi_select_prompt_selection(&self.prompt, &selected_refs)?;
        Ok(())
    }

    fn build_result(&self, checked: &[Vec<bool>]) -> Vec<Vec<usize>> {
        checked
            .iter()
            .map(|group_checked| {
                group_checked
                    .iter()
                    .enumerate()
                    .filter(|(_, &is_checked)| is_checked)
                    .map(|(idx, _)| idx)
                    .collect()
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cursor_conversion_roundtrip() {
        let gs: GroupMultiSelect<'_, &str> = GroupMultiSelect::new()
            .group("A", vec!["a1", "a2"])
            .group("B", vec!["b1"]);

        for flat in 0..gs.total_rows() {
            let cursor = gs.flat_to_cursor(flat);
            assert_eq!(gs.cursor_to_flat(cursor), flat);
        }
    }

    #[test]
    fn test_group_state() {
        assert!(matches!(
            GroupMultiSelect::<&str>::group_state(&[]),
            GroupState::None
        ));
        assert!(matches!(
            GroupMultiSelect::<&str>::group_state(&[false, false]),
            GroupState::None
        ));
        assert!(matches!(
            GroupMultiSelect::<&str>::group_state(&[true, false]),
            GroupState::Partial
        ));
        assert!(matches!(
            GroupMultiSelect::<&str>::group_state(&[true, true]),
            GroupState::All
        ));
    }

    #[test]
    fn test_toggle_group() {
        let gs: GroupMultiSelect<'_, &str> = GroupMultiSelect::new()
            .group("A", vec!["a1", "a2"])
            .group("B", vec!["b1"]);

        let mut checked = vec![vec![false, false], vec![false]];
        let cursor = Cursor {
            group_idx: 0,
            item_idx: None,
        };

        gs.toggle(&mut checked, cursor);
        assert_eq!(checked[0], vec![true, true]);

        gs.toggle(&mut checked, cursor);
        assert_eq!(checked[0], vec![false, false]);
    }

    #[test]
    fn test_toggle_item() {
        let gs: GroupMultiSelect<'_, &str> = GroupMultiSelect::new().group("A", vec!["a1", "a2"]);

        let mut checked = vec![vec![false, false]];
        let cursor = Cursor {
            group_idx: 0,
            item_idx: Some(1),
        };

        gs.toggle(&mut checked, cursor);
        assert_eq!(checked[0], vec![false, true]);
    }

    #[test]
    fn test_disabled_item_cannot_toggle() {
        let gs: GroupMultiSelect<'_, &str> = GroupMultiSelect::new().group_with_states(
            "A",
            vec![
                ("a1", ItemState::Normal),
                (
                    "a2",
                    ItemState::Disabled {
                        reason: "test".into(),
                    },
                ),
            ],
        );

        let mut checked = vec![vec![false, false]];
        let cursor = Cursor {
            group_idx: 0,
            item_idx: Some(1),
        };

        gs.toggle(&mut checked, cursor);
        assert_eq!(checked[0], vec![false, false]);
    }

    #[test]
    fn test_group_toggle_skips_disabled() {
        let gs: GroupMultiSelect<'_, &str> = GroupMultiSelect::new().group_with_states(
            "A",
            vec![
                ("a1", ItemState::Normal),
                (
                    "a2",
                    ItemState::Disabled {
                        reason: "test".into(),
                    },
                ),
                ("a3", ItemState::Normal),
            ],
        );

        let mut checked = vec![vec![false, false, false]];
        let cursor = Cursor {
            group_idx: 0,
            item_idx: None,
        };

        gs.toggle(&mut checked, cursor);
        assert_eq!(checked[0], vec![true, false, true]);
    }

    #[test]
    fn test_cursor_skips_disabled() {
        let gs: GroupMultiSelect<'_, &str> = GroupMultiSelect::new().group_with_states(
            "A",
            vec![
                ("a1", ItemState::Normal),
                (
                    "a2",
                    ItemState::Disabled {
                        reason: "test".into(),
                    },
                ),
                ("a3", ItemState::Normal),
            ],
        );

        let cursor = Cursor {
            group_idx: 0,
            item_idx: Some(0),
        };
        let new_cursor = gs.move_cursor_down(cursor);
        assert_eq!(new_cursor.item_idx, Some(2));

        let back_cursor = gs.move_cursor_up(new_cursor);
        assert_eq!(back_cursor.item_idx, Some(0));
    }
}
