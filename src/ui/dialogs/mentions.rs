//! @mention autocomplete in the compose dialog, shaped like an editor's completion list: typing
//! "@" offers the people in the current timeline, typing on narrows it and asks the instance
//! for more, Up/Down pick, Enter or Tab insert "@user@domain ", Escape dismisses and Ctrl+Space
//! brings the list back.
//!
//! Focus never leaves the post text - a list that took focus would break typing - so the
//! highlighted suggestion is spoken, and the list under the text is only there for sighted
//! users, who can also click a suggestion.

use std::{
	cell::RefCell,
	collections::{HashMap, HashSet},
	rc::Rc,
	sync::mpsc::{self, Receiver, Sender},
	thread,
	time::{Duration, Instant},
};

use wxdragon::prelude::*;

use crate::{
	AppState,
	mastodon::{Account, MastodonClient, Status},
	speech,
	timeline::TimelineEntry,
	ui::{commands::get_selected_entry, keys},
};

/// A handle can be long ("someone@some.very.long.instance.example"), but a run of handle
/// characters longer than this after an "@" isn't one.
const MAX_QUERY_CHARS: usize = 80;
/// How many suggestions the list holds at most.
const MAX_SUGGESTIONS: usize = 50;
/// How long typing has to pause before the instance is asked for matches.
const SEARCH_DELAY: Duration = Duration::from_millis(300);
const SEARCH_LIMIT: u32 = 10;
/// How often search results are collected and a due search is started.
const POLL_INTERVAL_MS: i32 = 100;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MentionCandidate {
	/// The full handle, "user@domain", without the leading "@".
	pub acct: String,
	/// The display name, empty when only the handle is known (people mentioned in a post).
	pub name: String,
}

impl MentionCandidate {
	fn from_account(account: &Account) -> Self {
		Self { acct: account.full_acct(), name: account.display_name.trim().to_string() }
	}

	fn label(&self) -> String {
		if self.name.is_empty() { format!("@{}", self.acct) } else { format!("{} (@{})", self.name, self.acct) }
	}

	fn spoken(&self) -> String {
		if self.name.is_empty() { format!("@{}", self.acct) } else { format!("{}, @{}", self.name, self.acct) }
	}
}

/// Where a compose dialog's suggestions come from, gathered before the dialog opens.
#[derive(Clone, Default)]
pub struct MentionSource {
	local: Vec<MentionCandidate>,
	/// Our own handle, lowercased, never suggested.
	self_acct: Option<String>,
	search: Option<(MastodonClient, String)>,
}

impl MentionSource {
	/// The people in the timeline, and the instance to search for anyone else.
	pub(crate) fn from_state(state: &AppState) -> Self {
		let search = state.client.clone().zip(state.access_token.clone());
		Self { local: timeline_people(state), self_acct: self_acct(state), search }
	}
}

/// Our own handle, lowercased and with the domain, to leave out of suggestions.
fn self_acct(state: &AppState) -> Option<String> {
	let account = state.active_account()?;
	let acct = account.acct.as_deref()?.trim().trim_start_matches('@');
	if acct.is_empty() {
		return None;
	}
	if acct.contains('@') {
		return Some(acct.to_lowercase());
	}
	let host = reqwest::Url::parse(&account.instance).ok()?.host_str()?.to_string();
	Some(format!("{acct}@{host}").to_lowercase())
}

/// Everyone in the active timeline but us - the selected entry's people first, then authors,
/// boosters and people mentioned, newest first. Also the user lookup dialog's suggestions.
pub fn timeline_people(state: &AppState) -> Vec<MentionCandidate> {
	let mut pool = Pool::default();
	if let Some(entry) = get_selected_entry(state) {
		pool.add_entry(entry);
	}
	for entry in state.timeline_manager.active().map(|t| t.entries.as_slice()).unwrap_or_default() {
		pool.add_entry(entry);
	}
	let me = self_acct(state);
	pool.list.retain(|c| me.as_deref().is_none_or(|me| !c.acct.eq_ignore_ascii_case(me)));
	pool.list
}

/// Candidates without repeats, in the order first seen.
#[derive(Default)]
struct Pool {
	list: Vec<MentionCandidate>,
	index: HashMap<String, usize>,
}

impl Pool {
	fn add(&mut self, candidate: MentionCandidate) {
		let key = candidate.acct.to_lowercase();
		if let Some(&i) = self.index.get(&key) {
			// A mention only gave the handle; an author seen later gives the name too.
			if self.list[i].name.is_empty() {
				self.list[i].name = candidate.name;
			}
			return;
		}
		self.index.insert(key, self.list.len());
		self.list.push(candidate);
	}

	fn add_entry(&mut self, entry: &TimelineEntry) {
		match entry {
			TimelineEntry::Status(status) => self.add_status(status),
			TimelineEntry::Notification(notification) => {
				self.add(MentionCandidate::from_account(&notification.account));
				if let Some(status) = &notification.status {
					self.add_status(status);
				}
			}
			TimelineEntry::Account(account) => self.add(MentionCandidate::from_account(account)),
			TimelineEntry::Hashtag(_) => {}
		}
	}

	fn add_status(&mut self, status: &Status) {
		self.add(MentionCandidate::from_account(&status.account));
		if let Some(reblog) = &status.reblog {
			self.add_status(reblog);
		}
		for mention in &status.mentions {
			self.add(MentionCandidate { acct: mention.full_acct(), name: String::new() });
		}
	}
}

struct Popup {
	/// Char index of the "@" being completed.
	at: usize,
	matches: Vec<MentionCandidate>,
	selected: usize,
}

struct Inner {
	/// The timeline's people plus everyone a search has turned up.
	pool: Pool,
	self_acct: Option<String>,
	search: Option<(MastodonClient, String)>,
	/// The instance's answers, by lowercased query.
	results: HashMap<String, Vec<MentionCandidate>>,
	requested: HashSet<String>,
	/// A search to start once typing pauses.
	due: Option<(String, Instant)>,
	popup: Option<Popup>,
	/// The "@" whose suggestions were dismissed with Escape or just accepted, so they stay
	/// closed while the caret is still on it.
	dismissed_at: Option<usize>,
	results_tx: Sender<(String, Vec<MentionCandidate>)>,
	results_rx: Receiver<(String, Vec<MentionCandidate>)>,
}

impl Inner {
	fn is_self(&self, candidate: &MentionCandidate) -> bool {
		self.self_acct.as_deref().is_some_and(|me| candidate.acct.eq_ignore_ascii_case(me))
	}

	/// Timeline people matching `query` first, then whatever the instance found for it.
	fn matches(&self, query: &str) -> Vec<MentionCandidate> {
		let mut seen = HashSet::new();
		let exact = self.results.get(query).into_iter().flatten();
		self.pool
			.list
			.iter()
			.filter(|c| matches_query(c, query))
			.chain(exact)
			.filter(|c| !self.is_self(c) && seen.insert(c.acct.to_lowercase()))
			.take(MAX_SUGGESTIONS)
			.cloned()
			.collect()
	}
}

/// The "@" the caret is completing, as (char index of the "@", text typed after it). The "@"
/// has to start a word, so an email address doesn't count, and a handle holds no spaces.
fn find_mention_query(text: &str, caret: usize) -> Option<(usize, String)> {
	let chars: Vec<char> = text.chars().collect();
	let caret = caret.min(chars.len());
	let mut i = caret;
	while i > 0 {
		i -= 1;
		match chars[i] {
			// The "@" in "user@domain" follows a handle character; keep looking for the first.
			'@' if i > 0 && is_handle_char(chars[i - 1]) => {}
			'@' => return Some((i, chars[i + 1..caret].iter().collect())),
			c if !is_handle_char(c) => return None,
			_ if caret - i > MAX_QUERY_CHARS => return None,
			_ => {}
		}
	}
	None
}

fn is_handle_char(c: char) -> bool {
	c.is_alphanumeric() || matches!(c, '_' | '.' | '-')
}

fn matches_query(candidate: &MentionCandidate, query: &str) -> bool {
	if query.is_empty() {
		return true;
	}
	if candidate.acct.to_lowercase().starts_with(query) {
		return true;
	}
	if query.contains('@') {
		return false;
	}
	let name = candidate.name.to_lowercase();
	name.starts_with(query) || name.split_whitespace().any(|word| word.starts_with(query))
}

/// How many positions a char takes in the native Windows edit control: UTF-16 units, with a
/// line break counting as two ("\r\n") though `get_value` gives "\n".
fn wx_width(c: char) -> i64 {
	if c == '\n' && cfg!(windows) { 2 } else { i64::from(c.len_utf16() == 2) + 1 }
}

fn wx_pos_to_char(text: &str, pos: i64) -> usize {
	let mut at = 0;
	for (i, c) in text.chars().enumerate() {
		if at >= pos {
			return i;
		}
		at += wx_width(c);
	}
	text.chars().count()
}

fn char_to_wx_pos(text: &str, index: usize) -> i64 {
	text.chars().take(index).map(wx_width).sum()
}

fn describe(popup: &Popup) -> String {
	let entry = &popup.matches[popup.selected];
	format!("{}, {} of {}", entry.spoken(), popup.selected + 1, popup.matches.len())
}

/// The suggestions for one compose dialog's post text.
#[derive(Clone)]
pub struct MentionCompleter {
	inner: Rc<RefCell<Inner>>,
	field: TextCtrl,
	list: ListBox,
	panel: Panel,
}

impl MentionCompleter {
	/// Wires the suggestions to `field` and `list`. The caller forwards text changes to
	/// [`Self::refresh`] and the dialog's keys to [`Self::handle_key`], and stops the returned
	/// timer once the dialog closes.
	pub fn new(
		dialog: Dialog,
		panel: Panel,
		field: TextCtrl,
		list: ListBox,
		source: MentionSource,
	) -> (Self, Timer<Dialog>) {
		let (results_tx, results_rx) = mpsc::channel();
		let mut pool = Pool::default();
		for candidate in source.local {
			pool.add(candidate);
		}
		let inner = Inner {
			pool,
			self_acct: source.self_acct,
			search: source.search,
			results: HashMap::new(),
			requested: HashSet::new(),
			due: None,
			popup: None,
			dismissed_at: None,
			results_tx,
			results_rx,
		};
		let completer = Self { inner: Rc::new(RefCell::new(inner)), field, list, panel };
		list.show(false);
		let clicked = completer.clone();
		list.on_selection_changed(move |_| {
			if let Some(index) = clicked.list.get_selection() {
				clicked.pick(index as usize);
			}
		});
		let timer = Timer::new(&dialog);
		let ticked = completer.clone();
		timer.on_tick(move |_| ticked.poll());
		timer.start(POLL_INTERVAL_MS, false);
		(completer, timer)
	}

	fn hide_list(&self) {
		if self.list.is_shown() {
			self.list.show(false);
			self.panel.layout();
		}
	}

	/// Re-reads the text and opens, narrows or closes the suggestions. Runs on every text
	/// change, including the ones made here while the state is borrowed - those are never
	/// typing, so a busy state just means "not now".
	pub fn refresh(&self) {
		let text = self.field.get_value();
		let caret = wx_pos_to_char(&text, self.field.get_insertion_point());
		let focused = self.field.has_focus();
		let Ok(mut s) = self.inner.try_borrow_mut() else { return };
		let Some((at, query)) = find_mention_query(&text, caret).filter(|_| focused) else {
			s.dismissed_at = None;
			s.due = None;
			let was_open = s.popup.take().is_some();
			drop(s);
			if was_open {
				self.hide_list();
			}
			return;
		};
		if s.dismissed_at == Some(at) {
			return;
		}
		s.dismissed_at = None;
		let query = query.to_lowercase();
		if query.is_empty() || s.search.is_none() || s.requested.contains(&query) {
			s.due = None;
		} else if s.due.as_ref().is_none_or(|(due, _)| *due != query) {
			s.due = Some((query.clone(), Instant::now()));
		}
		let matches = s.matches(&query);
		if matches.is_empty() {
			let was_open = s.popup.take().is_some();
			drop(s);
			if was_open {
				self.hide_list();
			}
			return;
		}
		let previous = s.popup.take();
		let opening = previous.is_none();
		let changed = previous.as_ref().is_none_or(|p| p.matches != matches);
		// Keep the highlight on the same person while the list narrows or grows around them.
		let previous_entry = previous.as_ref().and_then(|p| p.matches.get(p.selected));
		let selected = previous_entry.and_then(|e| matches.iter().position(|m| m.acct == e.acct)).unwrap_or(0);
		let moved = previous_entry.is_none_or(|e| *e != matches[selected]);
		let popup = Popup { at, matches, selected };
		let announcement = describe(&popup);
		let labels: Vec<String> = popup.matches.iter().map(MentionCandidate::label).collect();
		s.popup = Some(popup);
		drop(s);

		if changed {
			self.list.clear();
			for label in &labels {
				self.list.append(label);
			}
			self.list.set_selection(u32::try_from(selected).unwrap_or(0), true);
			if !self.list.is_shown() {
				self.list.show(true);
				self.panel.layout();
			}
		}
		// Queued behind the echo of the key just typed, and only when the highlighted person
		// changes, so a list that just grows doesn't repeat itself on every keystroke.
		if opening {
			speech::speak_queued(&format!("Mention suggestions. {announcement}"));
		} else if moved {
			speech::speak_queued(&announcement);
		}
	}

	/// Keys pressed in the post text. Returns true when the key was used up here.
	pub fn handle_key(&self, code: Option<i32>, shift: bool, ctrl: bool, alt: bool) -> bool {
		if alt {
			return false;
		}
		if ctrl && code == Some(keys::SPACE) {
			if let Ok(mut s) = self.inner.try_borrow_mut() {
				s.dismissed_at = None;
			}
			self.refresh();
			let open = self.inner.try_borrow().is_ok_and(|s| s.popup.is_some());
			if !open {
				speech::speak("No mention suggestions");
			}
			return true;
		}
		let Ok(mut s) = self.inner.try_borrow_mut() else { return false };
		let Some(popup) = s.popup.as_mut() else { return false };
		match code {
			Some(keys::UP | keys::DOWN) if !shift && !ctrl => {
				let count = popup.matches.len();
				popup.selected = if code == Some(keys::DOWN) {
					(popup.selected + 1) % count
				} else {
					(popup.selected + count - 1) % count
				};
				let selected = popup.selected;
				let announcement = describe(popup);
				drop(s);
				self.list.set_selection(u32::try_from(selected).unwrap_or(0), true);
				speech::speak(&announcement);
				true
			}
			Some(keys::RETURN | keys::TAB) if !shift && !ctrl => {
				drop(s);
				self.accept();
				true
			}
			Some(keys::ESCAPE) => {
				s.dismissed_at = Some(popup.at);
				s.popup = None;
				drop(s);
				self.hide_list();
				speech::speak("Suggestions closed");
				true
			}
			// Moving the caret sideways leaves the handle being typed.
			Some(keys::LEFT | keys::RIGHT | keys::HOME | keys::END) => {
				s.popup = None;
				drop(s);
				self.hide_list();
				false
			}
			_ => false,
		}
	}

	/// Replaces "@query" with the highlighted suggestion's "@user@domain ".
	fn accept(&self) {
		let text = self.field.get_value();
		let caret = wx_pos_to_char(&text, self.field.get_insertion_point());
		let Ok(mut s) = self.inner.try_borrow_mut() else { return };
		let Some(popup) = s.popup.take() else { return };
		let Some(entry) = popup.matches.get(popup.selected).cloned() else { return };
		// Set before the edit: the text change it fires must not reopen the list on this "@".
		s.dismissed_at = Some(popup.at);
		s.due = None;
		drop(s);

		self.hide_list();
		let insert = format!("@{} ", entry.acct);
		let from = char_to_wx_pos(&text, popup.at);
		let to = char_to_wx_pos(&text, caret.max(popup.at));
		self.field.replace(from, to, &insert);
		self.field.set_insertion_point(from + insert.chars().map(wx_width).sum::<i64>());
		let who = if entry.name.is_empty() { format!("@{}", entry.acct) } else { entry.name };
		speech::speak(&format!("{who} mentioned"));
	}

	/// A suggestion clicked in the list: picked as if highlighted and accepted.
	fn pick(&self, index: usize) {
		{
			let Ok(mut s) = self.inner.try_borrow_mut() else { return };
			match s.popup.as_mut() {
				Some(popup) if index < popup.matches.len() => popup.selected = index,
				_ => return,
			}
		}
		self.field.set_focus();
		self.accept();
	}

	/// Starts a search once typing has paused, and takes in any answers.
	fn poll(&self) {
		let mut arrived = false;
		{
			let Ok(mut s) = self.inner.try_borrow_mut() else { return };
			while let Ok((query, found)) = s.results_rx.try_recv() {
				for candidate in &found {
					s.pool.add(candidate.clone());
				}
				s.results.insert(query, found);
				arrived = true;
			}
			if let Some((query, _)) = s.due.take_if(|(_, since)| since.elapsed() >= SEARCH_DELAY)
				&& let Some((client, token)) = s.search.clone()
			{
				s.requested.insert(query.clone());
				let tx = s.results_tx.clone();
				thread::spawn(move || {
					// A failed search just leaves the timeline's people.
					let found = client
						.search_accounts(&token, &query, SEARCH_LIMIT)
						.map(|accounts| accounts.iter().map(MentionCandidate::from_account).collect())
						.unwrap_or_default();
					let _ = tx.send((query, found));
				});
			}
		}
		if arrived {
			self.refresh();
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn candidate(acct: &str, name: &str) -> MentionCandidate {
		MentionCandidate { acct: acct.into(), name: name.into() }
	}

	#[test]
	fn finds_the_handle_being_typed() {
		assert_eq!(find_mention_query("@", 1), Some((0, String::new())));
		assert_eq!(find_mention_query("hi @jan", 7), Some((3, "jan".into())));
		assert_eq!(find_mention_query("hi @jan@mastodon.soc", 20), Some((3, "jan@mastodon.soc".into())));
		assert_eq!(find_mention_query("(@ja", 4), Some((1, "ja".into())));
		assert_eq!(find_mention_query("mail a@b.com", 12), None);
		assert_eq!(find_mention_query("@jan there", 10), None);
		assert_eq!(find_mention_query("@jan there", 4), Some((0, "jan".into())));
		assert_eq!(find_mention_query("@jan\nx", 6), None);
		assert_eq!(find_mention_query(&format!("@{}", "a".repeat(90)), 91), None);
	}

	#[test]
	fn converts_native_positions() {
		let text = "a\nb😀c";
		let nl = if cfg!(windows) { 2 } else { 1 };
		assert_eq!(char_to_wx_pos(text, 3), 2 + nl);
		assert_eq!(char_to_wx_pos(text, 4), 4 + nl);
		assert_eq!(wx_pos_to_char(text, 4 + nl), 4);
		assert_eq!(wx_pos_to_char(text, 100), 5);
	}

	#[test]
	fn matches_handles_and_name_words() {
		let jane = candidate("jane@mastodon.social", "Jane Doe");
		assert!(matches_query(&jane, ""));
		assert!(matches_query(&jane, "ja"));
		assert!(matches_query(&jane, "jane@mas"));
		assert!(matches_query(&jane, "doe"));
		assert!(!matches_query(&jane, "doe@"));
		assert!(!matches_query(&jane, "x"));
	}

	#[test]
	fn pool_keeps_first_order_and_fills_in_names() {
		let mut pool = Pool::default();
		pool.add(candidate("a@x", ""));
		pool.add(candidate("b@x", "Bee"));
		pool.add(candidate("A@x", "Ay"));
		assert_eq!(pool.list, vec![candidate("a@x", "Ay"), candidate("b@x", "Bee")]);
	}
}
