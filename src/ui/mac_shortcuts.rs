//! Run timeline shortcuts before Cocoa turns their menu key equivalents into menu selections.

use std::{
	cell::{Cell, RefCell},
	ptr,
	rc::Rc,
};

use block::ConcreteBlock;
use objc::{class, msg_send, runtime::Object, sel, sel_impl};
use wxdragon::prelude::*;

use crate::{
	UiCommand,
	config::{ActionId, ShortcutsConfig},
	ui::{commands::command_for, dialogs, window::WindowParts},
	ui_wake::UiCommandSender,
};

// NSEventModifierFlags and NSEventMaskKeyDown, from AppKit.
const SHIFT: usize = 1 << 17;
const CONTROL: usize = 1 << 18;
const OPTION: usize = 1 << 19;
const COMMAND: usize = 1 << 20;
const KEY_DOWN: usize = 1 << 10;

fn wx_key_code(character: u16) -> Option<i32> {
	match character {
		0x7f => Some(8),                                              // Backspace
		0xf700 => Some(315),                                          // Up
		0xf701 => Some(317),                                          // Down
		0xf702 => Some(314),                                          // Left
		0xf703 => Some(316),                                          // Right
		0xf704..=0xf71b => Some(i32::from(character - 0xf704) + 340), // F1 through F24
		0xf728 => Some(127),                                          // Forward Delete
		0xf729 => Some(313),                                          // Home
		0xf72b => Some(312),                                          // End
		0xf72c => Some(366),                                          // Page Up
		0xf72d => Some(367),                                          // Page Down
		_ => {
			let character = char::from_u32(u32::from(character))?;
			if character.is_ascii() { Some(character.to_ascii_uppercase() as i32) } else { None }
		}
	}
}

pub(super) fn install(
	parts: &WindowParts,
	ui_tx: UiCommandSender,
	is_shutting_down: Rc<Cell<bool>>,
	quick_action_keys_enabled: Rc<Cell<bool>>,
	shortcuts_cell: Rc<RefCell<ShortcutsConfig>>,
) {
	let frame = parts.frame;
	let timelines_selector = parts.timelines_selector;
	let timeline_list = parts.timeline_list.clone();
	let monitor = ConcreteBlock::new(move |event: *mut Object| -> *mut Object {
		if is_shutting_down.get() || !(timelines_selector.has_focus() || timeline_list.has_focus()) {
			return event;
		}
		// A local NSEvent monitor runs before NSMenu.performKeyEquivalent. Returning null
		// stops only a key that Fedra handled, leaving the visible menu shortcuts intact.
		let (modifiers, character) = unsafe {
			let modifiers: usize = msg_send![event, modifierFlags];
			let characters: *mut Object = msg_send![event, charactersIgnoringModifiers];
			let length: usize = msg_send![characters, length];
			if length == 0 {
				return event;
			}
			let character: u16 = msg_send![characters, characterAtIndex: 0_usize];
			(modifiers, character)
		};
		let Some(key_code) = wx_key_code(character) else { return event };
		let quick_mode = quick_action_keys_enabled.get();
		if (49..=57).contains(&key_code)
			&& (modifiers & COMMAND != 0 || (quick_mode && modifiers & (COMMAND | OPTION | SHIFT | CONTROL) == 0))
		{
			let _ = ui_tx.send(UiCommand::SwitchTimelineByIndex((key_code - 49) as usize));
			return ptr::null_mut();
		}
		let action = shortcuts_cell.borrow().find_action(
			quick_mode,
			key_code,
			modifiers & COMMAND != 0,
			modifiers & OPTION != 0,
			modifiers & SHIFT != 0,
		);
		let Some(action) = action else { return event };
		// The in-window keymap has no physical-Control modifier. Do not consume a
		// Control key chord as though it were an unmodified quick key.
		if modifiers & CONTROL != 0 {
			return event;
		}
		match action {
			ActionId::Find => {
				if let Some(query) = dialogs::show_find_dialog(&frame) {
					let _ = ui_tx.send(UiCommand::Find(query));
				}
			}
			ActionId::ToggleQuickActionKeys => {
				let enabled = !quick_mode;
				quick_action_keys_enabled.set(enabled);
				let _ = ui_tx.send(UiCommand::SetQuickActionKeysEnabled(enabled));
			}
			_ => {
				if let Some(command) = command_for(action) {
					let _ = ui_tx.send(command);
				}
			}
		}
		ptr::null_mut()
	})
	.copy();
	unsafe {
		let _: *mut Object =
			msg_send![class!(NSEvent), addLocalMonitorForEventsMatchingMask: KEY_DOWN handler: &*monitor];
	}
}
