//! Records the platform this CLI is built for, so it can ask for the release bundle of the same
//! platform: `mythos-<version>-<target>.zip`.

fn main() {
    let target = std::env::var("TARGET").expect("cargo sets TARGET for build scripts");
    println!("cargo:rustc-env=MYTHOS_TARGET={target}");
    println!("cargo:rerun-if-changed=build.rs");
}
