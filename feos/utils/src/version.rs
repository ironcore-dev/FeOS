// SPDX-FileCopyrightText: 2023 SAP SE or an SAP affiliate company and IronCore contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(not(feature = "git-version"))]
pub fn full_version_string() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[cfg(feature = "git-version")]
pub fn full_version_string() -> String {
    let version = env!("CARGO_PKG_VERSION");
    let commit_hash = option_env!("GIT_COMMIT_HASH").unwrap_or("???????");
    format!("{version} ({commit_hash})")
}
