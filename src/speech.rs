//! Speech through the user's screen reader or a system voice, for when Fedra's window isn't
//! in the foreground and so a live region in it would go unheard.

use prismer::{Backend, Prism};

thread_local! {
	// Prism is only ever used from the UI thread, and a backend isn't Send.
	static BACKEND: Option<Backend<'static>> = Prism::new().ok().and_then(|prism| {
		// A backend borrows its context. Leaking the context once is what lets the backend be
		// kept rather than rebuilt for every line.
		let prism: &'static Prism = Box::leak(Box::new(prism));
		prism.create_best().ok()
	});
}

/// Starts prism up front, so the first thing spoken from the background isn't delayed by it.
pub fn init() {
	BACKEND.with(|_| {});
}

/// Speaks `text`, interrupting whatever is being said. Does nothing if no speech backend is
/// available.
pub fn speak(text: &str) {
	BACKEND.with(|backend| {
		if let Some(backend) = backend {
			let _ = backend.speak(text, true);
		}
	});
}

/// Whether the foreground window belongs to Fedra: the main window or one of its dialogs.
pub fn fedra_in_foreground() -> bool {
	use windows::Win32::{
		System::Threading::GetCurrentProcessId,
		UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId},
	};
	let mut process_id = 0;
	unsafe {
		let foreground = GetForegroundWindow();
		if foreground.is_invalid() {
			return false;
		}
		GetWindowThreadProcessId(foreground, Some(&raw mut process_id));
		process_id == GetCurrentProcessId()
	}
}
