// SPDX-FileCopyrightText: 2023 SAP SE or an SAP affiliate company and IronCore contributors
// SPDX-License-Identifier: Apache-2.0

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto_dir = "../../proto/v1";

    tonic_build::configure()
        .protoc_arg("--experimental_allow_proto3_optional")
        .compile(
            &[
                format!("{proto_dir}/vm.proto"),
                format!("{proto_dir}/host.proto"),
                format!("{proto_dir}/image.proto"),
            ],
            &[proto_dir],
        )?;
    Ok(())
}
