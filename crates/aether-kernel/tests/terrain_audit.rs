use aether_kernel::{Terrain, height};

#[test]
fn rust_and_javascript_heights_match_at_extreme_coordinates() {
    let t = Terrain {
        seed: 99,
        relief: 30,
        scale: 150,
        plaza: 40,
    };
    for (x, y, js_bits) in [
        (137.5, -912.25, 0xc0149fdeba15c4d0),
        (1.3e21, 0.0, 0x4028878bc64ce73c),
        (1e308, 0.0, 0x4010b81be6708422),
        (-1.4e21, 0.0, 0xc0285d5b7df25295),
    ] {
        assert_eq!(
            height(&t, x, y).to_bits(),
            js_bits,
            "x={x:e} y={y:e}: Rust height differs from JavaScript"
        );
    }
}
