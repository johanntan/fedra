//! The timeline list on platforms whose native list box doesn't make a screen reader reread items
//! as they change, so it can be one.

use std::{cell::RefCell, collections::HashSet, rc::Rc, time::Instant};

use accesskit::NodeId;
use wxdragon::prelude::*;

use crate::ui::keys;

const PAGE_JUMP: usize = 20;

type KeyDownCallback = Rc<dyn Fn(&WindowEventData)>;

#[derive(Default)]
struct State {
	ids: Vec<NodeId>,
	search_buffer: String,
	last_search_time: Option<Instant>,
	on_selection_changed: Option<Rc<dyn Fn()>>,
	on_key_down: Option<KeyDownCallback>,
}

#[derive(Clone)]
pub struct TimelineList {
	list: ListBox,
	state: Rc<RefCell<State>>,
}

impl WxWidget for TimelineList {
	fn handle_ptr(&self) -> *mut wxdragon::ffi::wxd_Window_t {
		self.list.handle_ptr()
	}
}

impl TimelineList {
	pub fn new(parent: &impl WxWidget) -> Self {
		let list = ListBox::builder(parent).build();
		let tl = Self { list, state: Rc::new(RefCell::new(State::default())) };
		let on_select = tl.clone();
		list.on_selection_changed(move |_| on_select.fire_selection_changed());
		let on_key = tl.clone();
		list.on_key_down(move |event| {
			if let WindowEventData::Keyboard(key_event) = &event {
				key_event.event.skip(true);
			}
			let callback = on_key.state.borrow().on_key_down.clone();
			if let Some(callback) = callback {
				callback(&event);
			}
		});
		tl
	}

	fn fire_selection_changed(&self) {
		let callback = self.state.borrow().on_selection_changed.clone();
		if let Some(callback) = callback {
			callback();
		}
	}

	fn select(&self, index: usize) {
		let index = u32::try_from(index).unwrap_or(u32::MAX);
		self.list.set_selection(index, true);
		self.list.ensure_visible(i32::try_from(index).unwrap_or(i32::MAX));
	}

	/// Moves the selection as the arrow, Home, End, Page Up or Page Down key `key` would, firing
	/// `on_selection_changed` if it moved.
	///
	/// Returns whether `key` is one of those keys, not whether the selection moved.
	pub fn navigate(&self, key: i32) -> bool {
		let count = self.state.borrow().ids.len();
		if count == 0 {
			return false;
		}
		let current = self.list.get_selection().map_or(0, |i| i as usize);
		let new_index = match key {
			keys::UP => current.saturating_sub(1),
			keys::DOWN => (current + 1).min(count - 1),
			keys::HOME => 0,
			keys::END => count - 1,
			keys::PAGE_UP => current.saturating_sub(PAGE_JUMP),
			keys::PAGE_DOWN => (current + PAGE_JUMP).min(count - 1),
			_ => return false,
		};
		if self.list.get_selection().is_none() || new_index != current {
			self.select(new_index);
			self.fire_selection_changed();
		}
		true
	}

	pub fn selected_text(&self) -> Option<String> {
		self.list.get_string_selection()
	}

	pub fn has_focus(&self) -> bool {
		self.list.has_focus()
	}

	pub fn get_selection(&self) -> Option<i32> {
		self.list.get_selection().map(|i| i32::try_from(i).unwrap_or(i32::MAX))
	}

	pub fn get_count(&self) -> i32 {
		i32::try_from(self.list.get_count()).unwrap_or(i32::MAX)
	}

	pub fn clear(&self) {
		self.list.clear();
		self.state.borrow_mut().ids.clear();
	}

	pub fn bind_internal<F>(&self, event_type: EventType, callback: F)
	where
		F: FnMut(Event) + 'static,
	{
		self.list.bind_internal(event_type, callback);
	}

	pub fn popup_menu(&self, menu: &mut Menu, pos: Option<Point>) {
		self.list.popup_menu(menu, pos);
	}

	pub fn on_selection_changed<F>(&self, callback: F)
	where
		F: Fn() + 'static,
	{
		self.state.borrow_mut().on_selection_changed = Some(Rc::new(callback));
	}

	pub fn on_key_down<F>(&self, callback: F)
	where
		F: Fn(&WindowEventData) + 'static,
	{
		self.state.borrow_mut().on_key_down = Some(Rc::new(callback));
	}

	/// Replaces the rows in place, rewriting only the ones whose text changed, so the list isn't
	/// rebuilt under the screen reader on every refresh.
	pub fn update_entries(&self, entries: &[(NodeId, String)], selected_id: Option<NodeId>) {
		let mut seen = HashSet::new();
		let entries: Vec<_> = entries.iter().filter(|(id, _)| seen.insert(*id)).collect();
		let old_count = self.list.get_count() as usize;
		for (index, (_, text)) in entries.iter().enumerate() {
			if index < old_count {
				let row = u32::try_from(index).unwrap_or(u32::MAX);
				if self.list.get_string(row).as_deref() != Some(text.as_str()) {
					self.list.set_string(row, text);
				}
			} else {
				self.list.append(text);
			}
		}
		for index in (entries.len()..old_count).rev() {
			self.list.delete(u32::try_from(index).unwrap_or(u32::MAX));
		}
		let ids: Vec<NodeId> = entries.iter().map(|(id, _)| *id).collect();
		let selected = selected_id.and_then(|id| ids.iter().position(|&candidate| candidate == id));
		self.state.borrow_mut().ids = ids;
		match selected.or_else(|| (!entries.is_empty()).then_some(0)) {
			Some(index) if self.list.get_selection() != u32::try_from(index).ok() => self.select(index),
			_ => {}
		}
	}

	/// Moves the selection without firing `on_selection_changed`.
	pub fn set_selection(&self, selected_id: Option<NodeId>) {
		let index = selected_id.and_then(|id| self.state.borrow().ids.iter().position(|&candidate| candidate == id));
		match index {
			Some(index) => self.select(index),
			None => {
				if let Some(current) = self.list.get_selection() {
					self.list.set_selection(current, false);
				}
			}
		}
	}

	pub fn type_ahead(&self, ch: char) {
		let count = self.list.get_count() as usize;
		if count == 0 {
			return;
		}
		let now = Instant::now();
		let lower_ch = ch.to_lowercase().next().unwrap_or(ch);
		let prefix = {
			let mut state = self.state.borrow_mut();
			if state.last_search_time.is_none_or(|t| now.duration_since(t).as_millis() > 1000) {
				state.search_buffer.clear();
			}
			state.last_search_time = Some(now);
			let is_repeat = state.search_buffer.len() == 1 && state.search_buffer.starts_with(lower_ch);
			if !is_repeat {
				state.search_buffer.push(lower_ch);
			}
			state.search_buffer.clone()
		};
		let selected = self.list.get_selection().map(|i| i as usize);
		let start = if prefix.chars().count() == 1 { selected.map_or(0, |i| i + 1) } else { selected.unwrap_or(0) };
		let found = (0..count).map(|offset| (start + offset) % count).find(|&index| {
			self.list
				.get_string(u32::try_from(index).unwrap_or(u32::MAX))
				.is_some_and(|text| text.to_lowercase().starts_with(&prefix))
		});
		if let Some(index) = found {
			self.select(index);
			self.fire_selection_changed();
		}
	}

	#[allow(clippy::unused_self, reason = "a method, like the Windows version")]
	pub fn announce(&self, text: &str) {
		crate::speech::speak(text);
	}
}
