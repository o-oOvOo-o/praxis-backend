#[derive(Clone, Debug)]
struct TerracesCompareCase {
    name: String,
    input_map: String,
    resolution: u32,
    num: u32,
    uniformity: f32,
    steepness: f32,
    intensity: f32,
    seed: i32,
    force_zero: bool,
}
