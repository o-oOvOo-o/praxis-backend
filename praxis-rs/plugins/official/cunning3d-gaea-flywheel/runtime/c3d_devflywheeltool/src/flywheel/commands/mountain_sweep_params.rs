#[derive(Clone, Debug)]
struct MountainSweepParams {
    index: usize,
    style: String,
    bulk: String,
    reduce_details: bool,
    scale: f32,
    height: f32,
    seed: i32,
    x: f32,
    y: f32,
    terrain_width: f32,
    terrain_height: f32,
    resolution: u32,
}
