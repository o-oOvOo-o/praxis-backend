use super::image_generation_artifact_path;
use super::save_image_generation_result;
use crate::error::PraxisErr;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn save_image_generation_result_saves_base64_to_png_in_praxis_home() {
    let praxis_home = tempfile::tempdir().expect("create Praxis home");
    let expected_path =
        image_generation_artifact_path(praxis_home.path(), "session-1", "ig_save_base64");
    let _ = std::fs::remove_file(&expected_path);

    let saved_path =
        save_image_generation_result(praxis_home.path(), "session-1", "ig_save_base64", "Zm9v")
            .await
            .expect("image should be saved");

    assert_eq!(saved_path, expected_path);
    assert_eq!(std::fs::read(&saved_path).expect("saved file"), b"foo");
    let _ = std::fs::remove_file(&saved_path);
}

#[tokio::test]
async fn save_image_generation_result_rejects_data_url_payload() {
    let result = "data:image/jpeg;base64,Zm9v";
    let praxis_home = tempfile::tempdir().expect("create Praxis home");

    let err = save_image_generation_result(praxis_home.path(), "session-1", "ig_456", result)
        .await
        .expect_err("data url payload should error");
    assert!(matches!(err, PraxisErr::InvalidRequest(_)));
}

#[tokio::test]
async fn save_image_generation_result_overwrites_existing_file() {
    let praxis_home = tempfile::tempdir().expect("create Praxis home");
    let existing_path =
        image_generation_artifact_path(praxis_home.path(), "session-1", "ig_overwrite");
    std::fs::create_dir_all(
        existing_path
            .parent()
            .expect("generated image path should have a parent"),
    )
    .expect("create image output dir");
    std::fs::write(&existing_path, b"existing").expect("seed existing image");

    let saved_path =
        save_image_generation_result(praxis_home.path(), "session-1", "ig_overwrite", "Zm9v")
            .await
            .expect("image should be saved");

    assert_eq!(saved_path, existing_path);
    assert_eq!(std::fs::read(&saved_path).expect("saved file"), b"foo");
    let _ = std::fs::remove_file(&saved_path);
}

#[tokio::test]
async fn save_image_generation_result_sanitizes_call_id_for_praxis_home_output_path() {
    let praxis_home = tempfile::tempdir().expect("create Praxis home");
    let expected_path = image_generation_artifact_path(praxis_home.path(), "session-1", "../ig/..");
    let _ = std::fs::remove_file(&expected_path);

    let saved_path =
        save_image_generation_result(praxis_home.path(), "session-1", "../ig/..", "Zm9v")
            .await
            .expect("image should be saved");

    assert_eq!(saved_path, expected_path);
    assert_eq!(std::fs::read(&saved_path).expect("saved file"), b"foo");
    let _ = std::fs::remove_file(&saved_path);
}

#[tokio::test]
async fn save_image_generation_result_rejects_non_standard_base64() {
    let praxis_home = tempfile::tempdir().expect("create Praxis home");
    let err = save_image_generation_result(praxis_home.path(), "session-1", "ig_urlsafe", "_-8")
        .await
        .expect_err("non-standard base64 should error");
    assert!(matches!(err, PraxisErr::InvalidRequest(_)));
}

#[tokio::test]
async fn save_image_generation_result_rejects_non_base64_data_urls() {
    let praxis_home = tempfile::tempdir().expect("create Praxis home");
    let err = save_image_generation_result(
        praxis_home.path(),
        "session-1",
        "ig_svg",
        "data:image/svg+xml,<svg/>",
    )
    .await
    .expect_err("non-base64 data url should error");
    assert!(matches!(err, PraxisErr::InvalidRequest(_)));
}
