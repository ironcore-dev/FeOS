fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .protoc_arg("--experimental_allow_proto3_optional")
        .compile_protos(
            &[
                "proto/feos.proto",
                "proto/container.proto",
                "proto/isolated_container.proto",
            ],
            &["proto"],
        )?;
    Ok(())
}
