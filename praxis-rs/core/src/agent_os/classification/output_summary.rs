use praxis_utils_output_truncation::truncate_to_char_boundary;

pub(in crate::agent_os) fn summarize_output(raw_output: &[u8]) -> String {
    let text = String::from_utf8_lossy(raw_output);
    let mut summary = text.lines().take(20).collect::<Vec<_>>().join("\n");
    truncate_to_char_boundary(&mut summary, 2_000);
    summary
}
