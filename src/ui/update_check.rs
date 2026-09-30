use std::sync::Arc;

use ship_shape::{UpdateChannel as ShipChannel, UpdaterConfig, ui::CheckTrigger};
use wxdragon::prelude::*;

const FEDRA_GITHUB_REPO: &str = "trypsynth/fedra";
const FEDRA_MINISIGN_KEY: &str = "RWTlkclKA9G3Jhv3wkicYywPfi5XqULERn6LrK7aIv9nYQUPbhQaxSqZ";

pub fn run_update_check(frame: Frame, silent: bool) {
	let config = crate::config::ConfigStore::new().load();
	let channel = match config.update_channel {
		crate::config::UpdateChannel::Stable => ShipChannel::Stable,
		crate::config::UpdateChannel::Dev => ShipChannel::Dev,
	};
	let updater_config = Arc::new(
		UpdaterConfig::new(FEDRA_GITHUB_REPO, "fedra", "Fedra", FEDRA_MINISIGN_KEY, env!("CARGO_PKG_VERSION"))
			.with_commit(env!("FEDRA_COMMIT_HASH"))
			.with_install_kind(crate::config::home().kind().into())
			.with_asset_suffix(if cfg!(target_arch = "aarch64") { "-arm64" } else { "-x64" }),
	);
	let trigger = if silent { CheckTrigger::Automatic } else { CheckTrigger::Manual };
	ship_shape::ui::run_update_check(updater_config, &frame, channel, trigger);
}
