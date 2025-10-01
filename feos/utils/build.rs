// SPDX-FileCopyrightText: 2023 SAP SE or an SAP affiliate company and IronCore contributors
// SPDX-License-Identifier: Apache-2.0

use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=.git/HEAD");

    let output = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output();

    if let Ok(output) = output {
        if output.status.success() {
            let commit_hash = String::from_utf8_lossy(&output.stdout).trim().to_string();
            println!("cargo:rustc-env=GIT_COMMIT_HASH={commit_hash}");
        }
    }
}
