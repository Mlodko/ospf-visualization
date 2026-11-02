fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_prost_build::configure()
    .build_server(false)
    .compile_protos(
        &[
            "submodules/gnmi/proto/gnmi/gnmi.proto",
            "submodules/gnmi/proto/gnmi_ext/gnmi_ext.proto"
        ],
        &[
            "submodules/gnmi/proto/"
        ])?;
    Ok(())
}