use std::{env, fs, path::Path};

fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let project_dir = Path::new(&manifest_dir);
    let build_type = env::var("PROFILE").unwrap();
    let target_dir = project_dir.join("..").join("target").join(build_type);

    println!("{:?}", project_dir);
    println!("{:?}", target_dir);

    fs::copy(
        project_dir.join("picup-srv.toml"),
        target_dir.join("picup-srv.toml"),
    )
    .unwrap();
}
