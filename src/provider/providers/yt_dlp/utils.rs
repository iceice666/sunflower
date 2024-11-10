use crate::provider::error::{ProviderError, ProviderResult};
use std::process::Command;

pub fn run_cmd(cmd_args: &[&str]) -> ProviderResult<String> {
    let exe = cmd_args[0];
    let args = &cmd_args[1..];

    let output = Command::new(exe).args(args).output()?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else if !output.stderr.is_empty() {
        Err(ProviderError::Other(
            String::from_utf8_lossy(&output.stderr).to_string(),
        ))
    } else {
        Err(ProviderError::Other(format!(
            "Process failed with exit code: {}",
            output.status.code().unwrap_or(-1)
        )))
    }
}
