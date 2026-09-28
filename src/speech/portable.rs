//! Speech through the screen reader's own announcements. Without global shortcuts there's no
//! speaking from the background, so this only ever talks to the screen reader Fedra is in front of.

use std::cell::Cell;

use live_region::Priority;
use wxdragon::prelude::*;

thread_local! {
	static LABEL: Cell<Option<StaticText>> = const { Cell::new(None) };
}

/// Makes the hidden label announcements go through.
pub fn init(frame: &Frame) {
	let label = StaticText::builder(frame).with_label("").build();
	label.show(false);
	LABEL.set(Some(label));
}

pub fn speak(text: &str) {
	announce(text, Priority::High);
}

pub fn speak_queued(text: &str) {
	announce(text, Priority::Medium);
}

fn announce(text: &str, priority: Priority) {
	if let Some(label) = LABEL.get() {
		live_region::announce_with_priority(label, text, priority);
	}
}
