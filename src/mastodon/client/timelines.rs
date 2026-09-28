//! Paged timeline, notification, and conversation fetches.

use anyhow::Result;
use serde::Deserialize;

use crate::{
	mastodon::{Conversation, MastodonClient, Notification, Status},
	timeline::TimelineType,
};

#[derive(Debug, Default, Deserialize)]
pub struct Markers {
	pub home: Option<Marker>,
	pub notifications: Option<Marker>,
}

#[derive(Debug, Deserialize)]
pub struct Marker {
	pub last_read_id: String,
}

impl MastodonClient {
	pub fn get_markers(&self, access_token: &str) -> Result<Markers> {
		let mut url = self.base_url.join("api/v1/markers")?;
		url.query_pairs_mut().append_pair("timeline[]", "home").append_pair("timeline[]", "notifications");
		self.get_json(access_token, url, "fetch read positions")
	}

	pub fn set_marker(&self, access_token: &str, timeline: &str, last_read_id: &str) -> Result<()> {
		let url = self.base_url.join("api/v1/markers")?;
		let form = [(format!("{timeline}[last_read_id]"), last_read_id)];
		Self::send_empty(self.http.post(url).bearer_auth(access_token).form(&form), "save read position")
	}

	pub fn get_timeline(
		&self,
		access_token: &str,
		timeline_type: &TimelineType,
		limit: Option<u32>,
		max_id: Option<&str>,
	) -> Result<(Vec<Status>, Option<String>)> {
		let mut url = self.base_url.join(&timeline_type.api_path())?;
		{
			let mut query = url.query_pairs_mut();
			for (key, value) in timeline_type.api_query_params() {
				query.append_pair(key, value);
			}
			if let Some(limit) = limit {
				query.append_pair("limit", &limit.to_string());
			}
			if let Some(max_id) = max_id {
				query.append_pair("max_id", max_id);
			}
		}
		let mut request = self.http.get(url);
		if timeline_type.requires_auth() {
			request = request.bearer_auth(access_token);
		}
		Self::send_json_paged(request, "fetch timeline")
	}

	pub fn get_notifications(
		&self,
		access_token: &str,
		timeline_type: &TimelineType,
		limit: Option<u32>,
		max_id: Option<&str>,
	) -> Result<(Vec<Notification>, Option<String>)> {
		let mut url = self.base_url.join(&timeline_type.api_path())?;
		{
			let mut query = url.query_pairs_mut();
			for (key, value) in timeline_type.api_query_params() {
				query.append_pair(key, value);
			}
			if let Some(limit) = limit {
				query.append_pair("limit", &limit.to_string());
			}
			if let Some(max_id) = max_id {
				query.append_pair("max_id", max_id);
			}
		}
		Self::send_json_paged(self.http.get(url).bearer_auth(access_token), "fetch notifications")
	}

	pub fn get_conversations(
		&self,
		access_token: &str,
		limit: Option<u32>,
		max_id: Option<&str>,
	) -> Result<(Vec<Conversation>, Option<String>)> {
		let mut url = self.base_url.join("api/v1/conversations")?;
		{
			let mut query = url.query_pairs_mut();
			if let Some(limit) = limit {
				query.append_pair("limit", &limit.to_string());
			}
			if let Some(max_id) = max_id {
				query.append_pair("max_id", max_id);
			}
		}
		Self::send_json_paged(self.http.get(url).bearer_auth(access_token), "fetch conversations")
	}
}
