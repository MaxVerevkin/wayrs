fn main() {
    println!("cargo:rustc-link-lib=EGL");
    println!("cargo:rustc-link-lib=drm");
}
