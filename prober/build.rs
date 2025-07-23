use std::io::Result;

fn main() -> Result<()> {
    prost_build::compile_protos(&["../protobuf/message.proto"], &["../protobuf/"])?;
    Ok(())
}
