//! Emit the notices supplied by our exact two-face version.
fn main() {
    print!("{}", two_face::acknowledgement::listing().to_md());
}
