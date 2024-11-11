use crate::provider::error::{ProviderError, ProviderResult};
use std::process::Command;

// FIXME: Having encoding issues at Windows
fn decode_utf8(buf: &[u8]) -> String {
    match String::from_utf8(buf.to_vec()) {
        Ok(s) => s,
        Err(_) => String::from_utf8_lossy(&buf).to_string(),
    }
}

pub fn run_cmd(cmd_args: &[&str]) -> ProviderResult<String> {
    let exe = cmd_args[0];
    let args = &cmd_args[1..];

    let output = Command::new(exe).args(args).output()?;

    if output.status.success() {
        Ok(decode_utf8(&output.stdout))
    } else if !output.stderr.is_empty() {
        Err(ProviderError::Command(
            decode_utf8(&output.stderr),
        ))
    } else {
        Err(ProviderError::Command(format!(
            "Process failed with exit code: {}",
            output.status.code().unwrap_or(-1)
        )))
    }
}
