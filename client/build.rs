fn main() {
    tonic_build::configure()
        .build_server(false)
        .compile(&["../proto/feos.proto"], &["../proto"])
        .unwrap();
}
