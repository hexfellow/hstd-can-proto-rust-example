use std::io::Result;
fn main() -> Result<()> {
    prost_build::compile_protos(
        &[
            "src/hstd-can-proto/hexstd_can_msg.proto",
            "src/hstd-can-proto/hex_bridge.proto",
            "src/hstd-can-proto/hex_bridge_control.proto",
        ],
        &["src/hstd-can-proto"],
    )?;
    Ok(())
}
