//! Fedra's side of the shared Customize Keyboard Shortcuts dialog.
//!
//! The dialog itself lives in `wx_utils::shortcuts`; all that is needed here is to describe
//! Fedra's keymap to it. Quick keys and normal mode are separate keymaps with their own
//! defaults, so the same key can mean different things in each and a conflict in one is not a
//! conflict in the other. The global shortcuts are system-wide hotkeys, which fire even while
//! Fedra's window has focus, so they conflict with both.

use wx_utils::shortcuts::{ShortcutModel, TabScope};
use wxdragon::prelude::*;

use crate::config::{ActionId, GlobalAction, KeyChord, ShortcutsConfig};

/// Tab order. Quick keys comes first because it is the mode most users customize.
const QUICK_KEYS_TAB: usize = 0;
const NORMAL_TAB: usize = 1;
const GLOBAL_TAB: usize = 2;

/// A row in the dialog: an in-window action or a global one.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Action {
	Local(ActionId),
	Global(GlobalAction),
}

/// A [`ShortcutsConfig`] presented as one tab per input mode, plus one for global shortcuts.
///
/// A newtype because both the trait and `ShortcutsConfig` are foreign to this module's owner.
#[derive(Clone)]
struct ModeShortcutsModel(ShortcutsConfig);

impl ModeShortcutsModel {
	const fn is_quick(tab: usize) -> bool {
		tab == QUICK_KEYS_TAB
	}
}

impl ShortcutModel for ModeShortcutsModel {
	type Action = Action;

	fn tabs(&self) -> Vec<String> {
		vec!["Quick Keys Mode".to_string(), "Normal Mode".to_string(), "Global".to_string()]
	}

	fn tab_scope(&self, tab: usize) -> TabScope {
		match tab {
			QUICK_KEYS_TAB => TabScope::Mode(0),
			NORMAL_TAB => TabScope::Mode(1),
			_ => TabScope::Global,
		}
	}

	fn actions(&self, tab: usize) -> Vec<Action> {
		if tab == GLOBAL_TAB {
			GlobalAction::all().into_iter().map(Action::Global).collect()
		} else {
			ActionId::all().iter().map(|&action| Action::Local(action)).collect()
		}
	}

	fn action_name(&self, action: Action) -> String {
		match action {
			Action::Local(action) => action.display_name(),
			Action::Global(action) => action.display_name(),
		}
		.to_string()
	}

	fn chord(&self, tab: usize, action: Action) -> Option<KeyChord> {
		match action {
			Action::Local(action) => self.0.get_chord(Self::is_quick(tab), action),
			Action::Global(action) => self.0.global.get_chord(action),
		}
	}

	fn set_chord(&mut self, tab: usize, action: Action, chord: Option<KeyChord>) {
		match action {
			Action::Local(action) => self.0.active_mode_mut(Self::is_quick(tab)).set_chord(action, chord),
			Action::Global(action) => self.0.global.set_chord(action, chord),
		}
	}

	fn reset_action(&mut self, tab: usize, action: Action) {
		match action {
			Action::Local(action) => self.0.active_mode_mut(Self::is_quick(tab)).reset_action(action),
			Action::Global(action) => self.0.global.reset_action(action),
		}
	}

	fn reset_all(&mut self, tab: usize) {
		if tab == GLOBAL_TAB {
			self.0.global.reset_all();
		} else {
			self.0.active_mode_mut(Self::is_quick(tab)).reset_all();
		}
	}
}

pub fn prompt_for_shortcuts(parent: &dyn WxWidget, initial: &ShortcutsConfig) -> Option<ShortcutsConfig> {
	let model = ModeShortcutsModel(initial.clone());
	wx_utils::shortcuts::prompt_for_shortcuts(parent, &model).map(|updated| updated.0)
}
