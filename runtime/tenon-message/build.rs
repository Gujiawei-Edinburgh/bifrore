fn main() -> Result<(), Box<dyn std::error::Error>> {
    prost_build::Config::new()
        .compile_protos(
            &[
                "src/daemon-message.proto",
                "src/cp-message.proto",
                "src/egress-message.proto",
            ],
            &["src"],
        )?;
    Ok(())
}
