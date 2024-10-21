fn main() {
    tonic_build::configure()
        .build_server(false)
        .compile(
            &[
                "../proto/feos.proto",
                "../proto/container.proto",
                "../proto/isolated_container.proto",
            ],
            &["../proto"],
        )
        .unwrap();
}
