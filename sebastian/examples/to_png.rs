//! Helper: render a .mmd to a PNG. `cargo run --features raster --example to_png -- in.mmd out.png`

#[cfg(feature = "raster")]
fn main() {
    let mut args = std::env::args().skip(1);
    let (input, output) = (args.next().unwrap(), args.next().unwrap());
    let src = std::fs::read_to_string(&input).unwrap();
    let png = sebastian::render_png(&src, "my-svg").unwrap();
    std::fs::write(&output, png).unwrap();
}

#[cfg(not(feature = "raster"))]
fn main() {
    eprintln!("build with --features raster");
}
