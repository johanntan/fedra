//! System-wide shortcuts: using Fedra without its window, from anywhere.
//!
//! Most global actions are ordinary commands under another key. Moving through posts is the
//! exception: it drives the timeline list the same way its arrow keys do, and speaks where it
//! lands whenever a screen reader wouldn't already be reading the list.

use super::{UiCommandContext, handle_ui_command};
use crate::{
	UiCommand,
	config::GlobalAction,
	ui::{app_shell, keys},
};

pub(super) fn run(ctx: &mut UiCommandContext<'_>, action: GlobalAction) {
	let command = match action {
		GlobalAction::ToggleWindow => {
			app_shell::toggle_window_visibility(ctx.frame, ctx.tray_hidden);
			return;
		}
		GlobalAction::PreviousPost => return step(ctx, keys::UP),
		GlobalAction::NextPost => return step(ctx, keys::DOWN),
		GlobalAction::FirstPost => return step(ctx, keys::HOME),
		GlobalAction::LastPost => return step(ctx, keys::END),
		GlobalAction::ReadPost => {
			let text = ctx.timeline_list.selected_text().unwrap_or_else(|| "No post selected".to_string());
			ctx.live_region.announce(&text);
			return;
		}
		GlobalAction::PreviousTimeline => UiCommand::SwitchPrevTimeline,
		GlobalAction::NextTimeline => UiCommand::SwitchNextTimeline,
		GlobalAction::LoadMore => UiCommand::LoadMore,
		GlobalAction::NewPost => UiCommand::NewPost,
		GlobalAction::Reply => UiCommand::Reply { reply_all: true },
		GlobalAction::Favorite => UiCommand::Favorite,
		GlobalAction::Boost => UiCommand::Boost,
		GlobalAction::ViewPost => UiCommand::ViewPost,
		GlobalAction::OpenLinks => UiCommand::OpenLinks,
		GlobalAction::ViewInBrowser => UiCommand::ViewInBrowser,
		GlobalAction::PlayMedia => UiCommand::PlayMedia,
	};
	handle_ui_command(command, ctx);
}

/// Moves through the active timeline as `key` would in the list, and reads the post it lands on.
///
/// At either end the selection stays put and that post is read again, so the key never goes
/// silent.
fn step(ctx: &UiCommandContext<'_>, key: i32) {
	let list = &ctx.timeline_list;
	if list.get_count() == 0 {
		ctx.live_region.announce("No posts");
		return;
	}
	list.navigate(key);
	// With the list focused in a foreground window, the screen reader reads the new selection
	// itself, and saying it again would read it twice.
	if list.has_focus() && app_shell::is_window_active(ctx.frame) {
		return;
	}
	if let Some(text) = list.selected_text() {
		ctx.live_region.announce(&text);
	}
}
