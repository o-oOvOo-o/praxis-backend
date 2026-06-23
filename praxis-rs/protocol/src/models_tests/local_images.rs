use super::*;

#[test]
fn mixed_remote_and_local_images_share_label_sequence() -> Result<()> {
    let image_url = "data:image/png;base64,abc".to_string();
    let dir = tempdir()?;
    let local_path = dir.path().join("local.png");
    // A tiny valid PNG (1x1) so this test doesn't depend on cross-crate file paths, which
    // break under Bazel sandboxing.
    const TINY_PNG_BYTES: &[u8] = &[
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6,
        0, 0, 0, 31, 21, 196, 137, 0, 0, 0, 11, 73, 68, 65, 84, 120, 156, 99, 96, 0, 2, 0, 0, 5, 0,
        1, 122, 94, 171, 63, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
    ];
    std::fs::write(&local_path, TINY_PNG_BYTES)?;

    let item = ResponseInputItem::from(vec![
        UserInput::Image {
            image_url: image_url.clone(),
        },
        UserInput::LocalImage { path: local_path },
    ]);

    match item {
        ResponseInputItem::Message { content, .. } => {
            assert_eq!(
                content.first(),
                Some(&ContentItem::InputText {
                    text: image_open_tag_text(),
                })
            );
            assert_eq!(content.get(1), Some(&ContentItem::InputImage { image_url }));
            assert_eq!(
                content.get(2),
                Some(&ContentItem::InputText {
                    text: image_close_tag_text(),
                })
            );
            assert_eq!(
                content.get(3),
                Some(&ContentItem::InputText {
                    text: local_image_open_tag_text(/*label_number*/ 2),
                })
            );
            assert!(matches!(
                content.get(4),
                Some(ContentItem::InputImage { .. })
            ));
            assert_eq!(
                content.get(5),
                Some(&ContentItem::InputText {
                    text: image_close_tag_text(),
                })
            );
        }
        other => panic!("expected message response but got {other:?}"),
    }

    Ok(())
}

#[test]
fn local_image_read_error_adds_placeholder() -> Result<()> {
    let dir = tempdir()?;
    let missing_path = dir.path().join("missing-image.png");

    let item = ResponseInputItem::from(vec![UserInput::LocalImage {
        path: missing_path.clone(),
    }]);

    match item {
        ResponseInputItem::Message { content, .. } => {
            assert_eq!(content.len(), 1);
            match &content[0] {
                ContentItem::InputText { text } => {
                    let display_path = missing_path.display().to_string();
                    assert!(
                        text.contains(&display_path),
                        "placeholder should mention missing path: {text}"
                    );
                    assert!(
                        text.contains("could not read"),
                        "placeholder should mention read issue: {text}"
                    );
                }
                other => panic!("expected placeholder text but found {other:?}"),
            }
        }
        other => panic!("expected message response but got {other:?}"),
    }

    Ok(())
}

#[test]
fn local_image_non_image_adds_placeholder() -> Result<()> {
    let dir = tempdir()?;
    let json_path = dir.path().join("example.json");
    std::fs::write(&json_path, br#"{"hello":"world"}"#)?;

    let item = ResponseInputItem::from(vec![UserInput::LocalImage {
        path: json_path.clone(),
    }]);

    match item {
        ResponseInputItem::Message { content, .. } => {
            assert_eq!(content.len(), 1);
            match &content[0] {
                ContentItem::InputText { text } => {
                    assert!(
                        text.contains("unsupported image `application/json`"),
                        "placeholder should mention unsupported image MIME: {text}"
                    );
                    assert!(
                        text.contains(&json_path.display().to_string()),
                        "placeholder should mention path: {text}"
                    );
                }
                other => panic!("expected placeholder text but found {other:?}"),
            }
        }
        other => panic!("expected message response but got {other:?}"),
    }

    Ok(())
}

#[test]
fn local_image_unsupported_image_format_adds_placeholder() -> Result<()> {
    let dir = tempdir()?;
    let svg_path = dir.path().join("example.svg");
    std::fs::write(
        &svg_path,
        br#"<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"></svg>"#,
    )?;

    let item = ResponseInputItem::from(vec![UserInput::LocalImage {
        path: svg_path.clone(),
    }]);

    match item {
        ResponseInputItem::Message { content, .. } => {
            assert_eq!(content.len(), 1);
            let expected = format!(
                "Praxis cannot attach image at `{}`: unsupported image `image/svg+xml`.",
                svg_path.display()
            );
            match &content[0] {
                ContentItem::InputText { text } => assert_eq!(text, &expected),
                other => panic!("expected placeholder text but found {other:?}"),
            }
        }
        other => panic!("expected message response but got {other:?}"),
    }

    Ok(())
}
