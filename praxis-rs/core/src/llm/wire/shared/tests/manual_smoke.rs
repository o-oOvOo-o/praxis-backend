use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "manual smoke test against a real Claude-compatible endpoint"]
async fn manual_glm_claude_smoke() {
    let output_text =
        run_manual_glm_claude_prompt("Reply with exactly PONG and nothing else.").await;
    assert_eq!(output_text.trim(), "PONG");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "manual smoke test against a real Claude-compatible endpoint"]
async fn manual_glm_claude_python_code_smoke() {
    let output_text = run_manual_glm_claude_prompt(
        "Write only Python code for a function `add_numbers(a, b)` that returns their sum. No explanation.",
    )
    .await;

    assert!(
        output_text.contains("def add_numbers"),
        "expected python function name in output: {output_text}"
    );
    assert!(
        output_text.contains("return"),
        "expected return statement in output: {output_text}"
    );
}
