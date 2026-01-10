use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=src/shaders/normals.metal");

    let shader_path = "src/shaders/normals.metal";
    let air_path = "src/shaders/normals.air";
    let metallib_path = "src/shaders/normals.metallib";

    let status = Command::new("xcrun")
        .args(["-sdk", "macosx", "metal", "-c", shader_path, "-o", air_path])
        .status()
        .expect("Failed to compile shader to AIR");

    if !status.success() {
        panic!("Shader compilation failed");
    }

    let status = Command::new("xcrun")
        .args(["-sdk", "macosx", "metallib", air_path, "-o", metallib_path])
        .status()
        .expect("Failed to link metallib");

    if !status.success() {
        panic!("Metallib linking failed");
    }
}
