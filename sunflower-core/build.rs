use std::{env, fs, path::Path, process::Command};

fn main() {
    tonic_build::compile_protos("protocol.proto")
        .unwrap_or_else(|e| panic!("Failed to compile protos {:?}", e));

    get_commit_hash();
}

fn get_commit_hash() {
    // Get the git hash
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    let git_hash = String::from_utf8(output.stdout).unwrap();

    // Create the output directory if it doesn't exist
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("git_hash.rs");

    // Write the git hash to a file that will be included in the build
    fs::write(
        dest_path,
        format!("pub const GIT_HASH: &str = \"{}\";", git_hash.trim()),
    )
    .unwrap();

    // Tell cargo to rerun this script only if we have new commits
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs");
}
