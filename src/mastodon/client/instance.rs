//! Instance metadata lookup.

use anyhow::Result;

use crate::mastodon::{
	InstanceInfo, MastodonClient, PollLimits,
	instance::{InstanceResponse, InstanceV2Response},
};

impl MastodonClient {
	pub fn get_instance_info(&self) -> Result<InstanceInfo> {
		let url = self.base_url.join("api/v1/instance")?;
		let info: InstanceResponse = Self::send_json(self.http.get(url), "fetch instance info")?;
		let max_chars =
			info.configuration.as_ref().and_then(|c| c.statuses.as_ref()).and_then(|s| s.max_characters).unwrap_or(500)
				as usize;
		let poll_limits =
			info.configuration.as_ref().and_then(|c| c.polls.as_ref()).map(PollLimits::from_config).unwrap_or_default();
		let streaming_url = info.urls.and_then(|u| u.streaming_api);
		Ok(InstanceInfo { max_post_chars: max_chars, poll_limits, streaming_url })
	}

	/// How many custom profile fields the instance allows, if it says.
	///
	/// Mastodon 4.6 and later report it on the v2 instance endpoint; Pleroma
	/// and Akkoma report it on v1. Anything else, including older Mastodon,
	/// reports nothing.
	pub fn get_max_profile_fields(&self) -> Option<usize> {
		let from_v2 = self
			.base_url
			.join("api/v2/instance")
			.ok()
			.and_then(|url| Self::send_json::<InstanceV2Response>(self.http.get(url), "fetch instance info").ok())
			.and_then(|info| info.configuration?.accounts?.max_profile_fields);
		let max = from_v2.or_else(|| {
			let url = self.base_url.join("api/v1/instance").ok()?;
			let info: InstanceResponse = Self::send_json(self.http.get(url), "fetch instance info").ok()?;
			info.pleroma?.metadata?.fields_limits?.max_fields
		})?;
		usize::try_from(max).ok()
	}
}
