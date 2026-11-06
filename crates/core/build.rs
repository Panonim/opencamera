fn main() {
    // Rebuild this program whenever the phone website is rebuilt.
    println!("cargo:rerun-if-changed=../../apps/mobile/dist");
}
