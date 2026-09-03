fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=proto/valqeron/v1/issuer.proto");
    println!("cargo:rerun-if-changed=proto/valqeron/v1/base.proto");
    println!("cargo:rerun-if-changed=proto/valqeron/v1/admin.proto");

    let file_descriptor_set = protox::compile(
        [
            "proto/valqeron/v1/base.proto",
            "proto/valqeron/v1/issuer.proto",
            "proto/valqeron/v1/admin.proto",
        ],
        ["proto"],
    )?;

    tonic_prost_build::configure()
        .build_client(true)
        .build_server(true)
        .compile_fds(file_descriptor_set)?;

    Ok(())
}
