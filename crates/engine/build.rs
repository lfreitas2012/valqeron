fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=proto/valqeron/v1/valqeron.proto");

    let file_descriptor_set = protox::compile(["proto/valqeron/v1/valqeron.proto"], ["proto"])?;

    tonic_prost_build::configure()
        .build_client(true)
        .build_server(true)
        .compile_fds(file_descriptor_set)?;

    Ok(())
}
