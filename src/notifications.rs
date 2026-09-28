use crate::{
	AppState,
	config::NotificationPreference,
	mastodon::{Notification, Status},
	ui::app_shell::AppShell,
};

#[cfg(windows)]
fn show(app_shell: Option<&AppShell>, title: &str, body: &str) {
	if let Some(app_shell) = app_shell {
		// wxICON_INFORMATION = 0x00000002
		app_shell.taskbar.show_balloon(title, body, 5000, 0x0000_0002, None);
	}
}

#[cfg(not(windows))]
fn show(_app_shell: Option<&AppShell>, title: &str, body: &str) {
	use std::cell::RefCell;

	use wxdragon::prelude::{NotificationMessage, TIMEOUT_AUTO};
	// Kept until the next one, since dropping a notification takes it off the screen.
	thread_local! {
		static LAST: RefCell<Option<NotificationMessage>> = const { RefCell::new(None) };
	}
	if let Ok(message) = NotificationMessage::builder().with_title(title).with_message(body).build() {
		message.show(TIMEOUT_AUTO);
		LAST.with_borrow_mut(|last| *last = Some(message));
	}
}

pub fn show_notification(app_shell: Option<&AppShell>, notification: &Notification) {
	show(app_shell, notification.account.display_name_or_username(), &notification.simple_display());
}

/// Alerts once for a batch of new posts in timelines the user asked to be notified about, each
/// paired with its timeline's name.
pub fn notify_new_posts(state: &AppState, posts: &[(String, Status)]) {
	let Some((timeline_name, status)) = posts.first() else { return };
	match state.config.notification_preference {
		NotificationPreference::Classic => {
			let app_shell = state.app_shell.as_deref();
			if posts.len() == 1 {
				let title = format!("{} in {timeline_name}", status.account.display_name_or_username());
				show(app_shell, &title, &status.simple_display());
			} else {
				show(app_shell, &format!("{} new posts", posts.len()), &format!("In {timeline_name}"));
			}
		}
		NotificationPreference::SoundOnly => {
			if let Some((output, sound_path)) = &state.notification_sound {
				crate::audio::play_once(output, sound_path);
			}
		}
		NotificationPreference::Disabled => {}
	}
}
