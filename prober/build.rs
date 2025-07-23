use std::io::Result;

fn main() -> Result<()> {
    prost_build::compile_protos(&["proto/message.proto"], &["proto/"])?;
    Ok(())
}
