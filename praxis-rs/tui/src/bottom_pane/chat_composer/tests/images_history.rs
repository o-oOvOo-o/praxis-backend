use super::*;

#[test]
fn attach_image_and_submit_includes_local_image_paths() {
    let (tx, _rx) = unbounded_channel::<AppEvent>();
    let sender = AppEventSender::new(tx);
    let mut composer = ChatComposer::new(
        /*has_input_focus*/ true,
        sender,
        /*enhanced_keys_supported*/ false,
        "Ask Praxis to do anything".to_string(),
        /*disable_paste_burst*/ false,
    );
    let path = PathBuf::from("/tmp/image1.png");
    composer.attach_image(path.clone());
    composer.handle_paste(" hi".into());
    let (result, _) = composer.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    match result {
        InputResult::Submitted {
            text,
            text_elements,
        } => {
            assert_eq!(text, "[Image #1] hi");
            assert_eq!(text_elements.len(), 1);
            assert_eq!(text_elements[0].placeholder(&text), Some("[Image #1]"));
            assert_eq!(
                text_elements[0].byte_range,
                ByteRange {
                    start: 0,
                    end: "[Image #1]".len()
                }
            );
        }
        _ => panic!("expected Submitted"),
    }
    let imgs = composer.take_recent_submission_images();
    assert_eq!(vec![path], imgs);
}

#[test]
fn submit_captures_recent_mention_bindings_before_clearing_textarea() {
    let (tx, _rx) = unbounded_channel::<AppEvent>();
    let sender = AppEventSender::new(tx);
    let mut composer = ChatComposer::new(
        /*has_input_focus*/ true,
        sender,
        /*enhanced_keys_supported*/ false,
        "Ask Praxis to do anything".to_string(),
        /*disable_paste_burst*/ false,
    );

    let mention_bindings = vec![MentionBinding {
        mention: "figma".to_string(),
        path: "/tmp/user/figma/SKILL.md".to_string(),
    }];
    composer.set_text_content_with_mention_bindings(
        "$figma please".to_string(),
        Vec::new(),
        Vec::new(),
        mention_bindings.clone(),
    );

    let (result, _) = composer.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(matches!(result, InputResult::Submitted { .. }));
    assert_eq!(
        composer.take_recent_submission_mention_bindings(),
        mention_bindings
    );
    assert!(composer.take_mention_bindings().is_empty());
}

#[test]
fn history_navigation_restores_remote_and_local_image_attachments() {
    let (tx, _rx) = unbounded_channel::<AppEvent>();
    let sender = AppEventSender::new(tx);
    let mut composer = ChatComposer::new(
        /*has_input_focus*/ true,
        sender,
        /*enhanced_keys_supported*/ false,
        "Ask Praxis to do anything".to_string(),
        /*disable_paste_burst*/ false,
    );
    let remote_image_url = "https://example.com/remote.png".to_string();
    composer.set_remote_image_urls(vec![remote_image_url.clone()]);
    let path = PathBuf::from("/tmp/image1.png");
    composer.attach_image(path.clone());

    let (result, _needs_redraw) =
        composer.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(matches!(result, InputResult::Submitted { .. }));

    let _ = composer.take_remote_image_urls();
    composer.set_text_content(String::new(), Vec::new(), Vec::new());

    let _ = composer.handle_key_event(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));

    let text = composer.current_text();
    assert_eq!(text, "[Image #2]");
    let text_elements = composer.text_elements();
    assert_eq!(text_elements.len(), 1);
    assert_eq!(text_elements[0].placeholder(&text), Some("[Image #2]"));
    assert_eq!(composer.local_image_paths(), vec![path]);
    assert_eq!(composer.remote_image_urls(), vec![remote_image_url]);
}

#[test]
fn history_navigation_restores_remote_only_submissions() {
    let (tx, _rx) = unbounded_channel::<AppEvent>();
    let sender = AppEventSender::new(tx);
    let mut composer = ChatComposer::new(
        /*has_input_focus*/ true,
        sender,
        /*enhanced_keys_supported*/ false,
        "Ask Praxis to do anything".to_string(),
        /*disable_paste_burst*/ false,
    );
    let remote_image_urls = vec![
        "https://example.com/one.png".to_string(),
        "https://example.com/two.png".to_string(),
    ];
    composer.set_remote_image_urls(remote_image_urls.clone());

    let (submitted_text, submitted_elements) = composer
        .prepare_submission_text(/*record_history*/ true)
        .expect("remote-only submission should be prepared");
    assert_eq!(submitted_text, "");
    assert!(submitted_elements.is_empty());

    let _ = composer.take_remote_image_urls();
    composer.set_text_content(String::new(), Vec::new(), Vec::new());

    let _ = composer.handle_key_event(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
    assert_eq!(composer.current_text(), "");
    assert!(composer.text_elements().is_empty());
    assert_eq!(composer.remote_image_urls(), remote_image_urls);
}

#[test]
fn history_navigation_leaves_cursor_at_end_of_line() {
    let (tx, _rx) = unbounded_channel::<AppEvent>();
    let sender = AppEventSender::new(tx);
    let mut composer = ChatComposer::new(
        /*has_input_focus*/ true,
        sender,
        /*enhanced_keys_supported*/ false,
        "Ask Praxis to do anything".to_string(),
        /*disable_paste_burst*/ false,
    );

    type_chars_humanlike(&mut composer, &['f', 'i', 'r', 's', 't']);
    let (result, _needs_redraw) =
        composer.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(matches!(result, InputResult::Submitted { .. }));

    type_chars_humanlike(&mut composer, &['s', 'e', 'c', 'o', 'n', 'd']);
    let (result, _needs_redraw) =
        composer.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(matches!(result, InputResult::Submitted { .. }));

    let (_result, _needs_redraw) =
        composer.handle_key_event(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
    assert_eq!(composer.textarea.text(), "second");
    assert_eq!(composer.textarea.cursor(), composer.textarea.text().len());

    let (_result, _needs_redraw) =
        composer.handle_key_event(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
    assert_eq!(composer.textarea.text(), "first");
    assert_eq!(composer.textarea.cursor(), composer.textarea.text().len());

    let (_result, _needs_redraw) =
        composer.handle_key_event(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    assert_eq!(composer.textarea.text(), "second");
    assert_eq!(composer.textarea.cursor(), composer.textarea.text().len());

    let (_result, _needs_redraw) =
        composer.handle_key_event(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    assert!(composer.textarea.is_empty());
    assert_eq!(composer.textarea.cursor(), composer.textarea.text().len());
}

#[test]
fn set_text_content_reattaches_images_without_placeholder_metadata() {
    let (tx, _rx) = unbounded_channel::<AppEvent>();
    let sender = AppEventSender::new(tx);
    let mut composer = ChatComposer::new(
        /*has_input_focus*/ true,
        sender,
        /*enhanced_keys_supported*/ false,
        "Ask Praxis to do anything".to_string(),
        /*disable_paste_burst*/ false,
    );

    let placeholder = local_image_label_text(/*label_number*/ 1);
    let text = format!("{placeholder} restored");
    let text_elements = vec![TextElement::new(
        (0..placeholder.len()).into(),
        /*placeholder*/ None,
    )];
    let path = PathBuf::from("/tmp/image1.png");

    composer.set_text_content(text, text_elements, vec![path.clone()]);

    assert_eq!(composer.local_image_paths(), vec![path]);
}

#[test]
fn large_paste_preserves_image_text_elements_on_submit() {
    let (tx, _rx) = unbounded_channel::<AppEvent>();
    let sender = AppEventSender::new(tx);
    let mut composer = ChatComposer::new(
        /*has_input_focus*/ true,
        sender,
        /*enhanced_keys_supported*/ false,
        "Ask Praxis to do anything".to_string(),
        /*disable_paste_burst*/ false,
    );

    let large_content = "x".repeat(LARGE_PASTE_CHAR_THRESHOLD + 5);
    composer.handle_paste(large_content.clone());
    composer.handle_paste(" ".into());
    let path = PathBuf::from("/tmp/image_with_paste.png");
    composer.attach_image(path.clone());

    let (result, _) = composer.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    match result {
        InputResult::Submitted {
            text,
            text_elements,
        } => {
            let expected = format!("{large_content} [Image #1]");
            assert_eq!(text, expected);
            assert_eq!(text_elements.len(), 1);
            assert_eq!(text_elements[0].placeholder(&text), Some("[Image #1]"));
            assert_eq!(
                text_elements[0].byte_range,
                ByteRange {
                    start: large_content.len() + 1,
                    end: large_content.len() + 1 + "[Image #1]".len(),
                }
            );
        }
        _ => panic!("expected Submitted"),
    }
    let imgs = composer.take_recent_submission_images();
    assert_eq!(vec![path], imgs);
}

#[test]
fn large_paste_with_leading_whitespace_trims_and_shifts_elements() {
    let (tx, _rx) = unbounded_channel::<AppEvent>();
    let sender = AppEventSender::new(tx);
    let mut composer = ChatComposer::new(
        /*has_input_focus*/ true,
        sender,
        /*enhanced_keys_supported*/ false,
        "Ask Praxis to do anything".to_string(),
        /*disable_paste_burst*/ false,
    );

    let large_content = format!("  {}", "x".repeat(LARGE_PASTE_CHAR_THRESHOLD + 5));
    composer.handle_paste(large_content.clone());
    composer.handle_paste(" ".into());
    let path = PathBuf::from("/tmp/image_with_trim.png");
    composer.attach_image(path.clone());

    let (result, _) = composer.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    match result {
        InputResult::Submitted {
            text,
            text_elements,
        } => {
            let trimmed = large_content.trim().to_string();
            assert_eq!(text, format!("{trimmed} [Image #1]"));
            assert_eq!(text_elements.len(), 1);
            assert_eq!(text_elements[0].placeholder(&text), Some("[Image #1]"));
            assert_eq!(
                text_elements[0].byte_range,
                ByteRange {
                    start: trimmed.len() + 1,
                    end: trimmed.len() + 1 + "[Image #1]".len(),
                }
            );
        }
        _ => panic!("expected Submitted"),
    }
    let imgs = composer.take_recent_submission_images();
    assert_eq!(vec![path], imgs);
}

#[test]
fn pasted_crlf_normalizes_newlines_for_elements() {
    let (tx, _rx) = unbounded_channel::<AppEvent>();
    let sender = AppEventSender::new(tx);
    let mut composer = ChatComposer::new(
        /*has_input_focus*/ true,
        sender,
        /*enhanced_keys_supported*/ false,
        "Ask Praxis to do anything".to_string(),
        /*disable_paste_burst*/ false,
    );

    let pasted = "line1\r\nline2\r\n".to_string();
    composer.handle_paste(pasted);
    composer.handle_paste(" ".into());
    let path = PathBuf::from("/tmp/image_crlf.png");
    composer.attach_image(path.clone());

    let (result, _) = composer.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    match result {
        InputResult::Submitted {
            text,
            text_elements,
        } => {
            assert_eq!(text, "line1\nline2\n [Image #1]");
            assert!(!text.contains('\r'));
            assert_eq!(text_elements.len(), 1);
            assert_eq!(text_elements[0].placeholder(&text), Some("[Image #1]"));
            assert_eq!(
                text_elements[0].byte_range,
                ByteRange {
                    start: "line1\nline2\n ".len(),
                    end: "line1\nline2\n [Image #1]".len(),
                }
            );
        }
        _ => panic!("expected Submitted"),
    }
    let imgs = composer.take_recent_submission_images();
    assert_eq!(vec![path], imgs);
}

#[test]
fn suppressed_submission_restores_pending_paste_payload() {
    let (tx, _rx) = unbounded_channel::<AppEvent>();
    let sender = AppEventSender::new(tx);
    let mut composer = ChatComposer::new(
        /*has_input_focus*/ true,
        sender,
        /*enhanced_keys_supported*/ false,
        "Ask Praxis to do anything".to_string(),
        /*disable_paste_burst*/ false,
    );

    composer.textarea.set_text_clearing_elements("/unknown ");
    composer.textarea.set_cursor("/unknown ".len());
    let large_content = "x".repeat(LARGE_PASTE_CHAR_THRESHOLD + 5);
    composer.handle_paste(large_content.clone());
    let placeholder = composer
        .pending_pastes
        .first()
        .expect("expected pending paste")
        .0
        .clone();

    let (result, _) = composer.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(matches!(result, InputResult::None));
    assert_eq!(composer.pending_pastes.len(), 1);
    assert_eq!(composer.textarea.text(), format!("/unknown {placeholder}"));

    composer.textarea.set_cursor(/*pos*/ 0);
    composer.textarea.insert_str(" ");
    let (result, _) = composer.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    match result {
        InputResult::Submitted {
            text,
            text_elements,
        } => {
            assert_eq!(text, format!("/unknown {large_content}"));
            assert!(text_elements.is_empty());
        }
        _ => panic!("expected Submitted"),
    }
    assert!(composer.pending_pastes.is_empty());
}

#[test]
fn attach_image_without_text_submits_empty_text_and_images() {
    let (tx, _rx) = unbounded_channel::<AppEvent>();
    let sender = AppEventSender::new(tx);
    let mut composer = ChatComposer::new(
        /*has_input_focus*/ true,
        sender,
        /*enhanced_keys_supported*/ false,
        "Ask Praxis to do anything".to_string(),
        /*disable_paste_burst*/ false,
    );
    let path = PathBuf::from("/tmp/image2.png");
    composer.attach_image(path.clone());
    let (result, _) = composer.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    match result {
        InputResult::Submitted {
            text,
            text_elements,
        } => {
            assert_eq!(text, "[Image #1]");
            assert_eq!(text_elements.len(), 1);
            assert_eq!(text_elements[0].placeholder(&text), Some("[Image #1]"));
            assert_eq!(
                text_elements[0].byte_range,
                ByteRange {
                    start: 0,
                    end: "[Image #1]".len()
                }
            );
        }
        _ => panic!("expected Submitted"),
    }
    let imgs = composer.take_recent_submission_images();
    assert_eq!(imgs.len(), 1);
    assert_eq!(imgs[0], path);
    assert!(composer.attached_images.is_empty());
}

#[test]
fn duplicate_image_placeholders_get_suffix() {
    let (tx, _rx) = unbounded_channel::<AppEvent>();
    let sender = AppEventSender::new(tx);
    let mut composer = ChatComposer::new(
        /*has_input_focus*/ true,
        sender,
        /*enhanced_keys_supported*/ false,
        "Ask Praxis to do anything".to_string(),
        /*disable_paste_burst*/ false,
    );
    let path = PathBuf::from("/tmp/image_dup.png");
    composer.attach_image(path.clone());
    composer.handle_paste(" ".into());
    composer.attach_image(path);

    let text = composer.textarea.text().to_string();
    assert!(text.contains("[Image #1]"));
    assert!(text.contains("[Image #2]"));
    assert_eq!(composer.attached_images[0].placeholder, "[Image #1]");
    assert_eq!(composer.attached_images[1].placeholder, "[Image #2]");
}

#[test]
fn image_placeholder_backspace_behaves_like_text_placeholder() {
    let (tx, _rx) = unbounded_channel::<AppEvent>();
    let sender = AppEventSender::new(tx);
    let mut composer = ChatComposer::new(
        /*has_input_focus*/ true,
        sender,
        /*enhanced_keys_supported*/ false,
        "Ask Praxis to do anything".to_string(),
        /*disable_paste_burst*/ false,
    );
    let path = PathBuf::from("/tmp/image3.png");
    composer.attach_image(path.clone());
    let placeholder = composer.attached_images[0].placeholder.clone();

    // Case 1: backspace at end
    composer
        .textarea
        .move_cursor_to_end_of_line(/*move_down_at_eol*/ false);
    composer.handle_key_event(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
    assert!(!composer.textarea.text().contains(&placeholder));
    assert!(composer.attached_images.is_empty());

    // Re-add and ensure backspace at element start does not delete the placeholder.
    composer.attach_image(path);
    let placeholder2 = composer.attached_images[0].placeholder.clone();
    // Move cursor to roughly middle of placeholder
    if let Some(start_pos) = composer.textarea.text().find(&placeholder2) {
        let mid_pos = start_pos + (placeholder2.len() / 2);
        composer.textarea.set_cursor(mid_pos);
        composer.handle_key_event(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        assert!(composer.textarea.text().contains(&placeholder2));
        assert_eq!(composer.attached_images.len(), 1);
    } else {
        panic!("Placeholder not found in textarea");
    }
}

#[test]
fn backspace_with_multibyte_text_before_placeholder_does_not_panic() {
    use crossterm::event::KeyCode;
    use crossterm::event::KeyEvent;
    use crossterm::event::KeyModifiers;

    let (tx, _rx) = unbounded_channel::<AppEvent>();
    let sender = AppEventSender::new(tx);
    let mut composer = ChatComposer::new(
        /*has_input_focus*/ true,
        sender,
        /*enhanced_keys_supported*/ false,
        "Ask Praxis to do anything".to_string(),
        /*disable_paste_burst*/ false,
    );

    // Insert an image placeholder at the start
    let path = PathBuf::from("/tmp/image_multibyte.png");
    composer.attach_image(path);
    // Add multibyte text after the placeholder
    composer.textarea.insert_str("日本語");

    // Cursor is at end; pressing backspace should delete the last character
    // without panicking and leave the placeholder intact.
    composer.handle_key_event(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));

    assert_eq!(composer.attached_images.len(), 1);
    assert!(composer.textarea.text().starts_with("[Image #1]"));
}

#[test]
fn deleting_one_of_duplicate_image_placeholders_removes_one_entry() {
    let (tx, _rx) = unbounded_channel::<AppEvent>();
    let sender = AppEventSender::new(tx);
    let mut composer = ChatComposer::new(
        /*has_input_focus*/ true,
        sender,
        /*enhanced_keys_supported*/ false,
        "Ask Praxis to do anything".to_string(),
        /*disable_paste_burst*/ false,
    );

    let path1 = PathBuf::from("/tmp/image_dup1.png");
    let path2 = PathBuf::from("/tmp/image_dup2.png");

    composer.attach_image(path1);
    // separate placeholders with a space for clarity
    composer.handle_paste(" ".into());
    composer.attach_image(path2.clone());

    let placeholder1 = composer.attached_images[0].placeholder.clone();
    let placeholder2 = composer.attached_images[1].placeholder.clone();
    let text = composer.textarea.text().to_string();
    let start1 = text.find(&placeholder1).expect("first placeholder present");
    let end1 = start1 + placeholder1.len();
    composer.textarea.set_cursor(end1);

    // Backspace should delete the first placeholder and its mapping.
    composer.handle_key_event(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));

    let new_text = composer.textarea.text().to_string();
    assert_eq!(
        1,
        new_text.matches(&placeholder1).count(),
        "one placeholder remains after deletion"
    );
    assert_eq!(
        0,
        new_text.matches(&placeholder2).count(),
        "second placeholder was relabeled"
    );
    assert_eq!(
        1,
        new_text.matches("[Image #1]").count(),
        "remaining placeholder relabeled to #1"
    );
    assert_eq!(
        vec![AttachedImage {
            path: path2,
            placeholder: "[Image #1]".to_string()
        }],
        composer.attached_images,
        "one image mapping remains"
    );
}

#[test]
fn deleting_reordered_image_one_renumbers_text_in_place() {
    use crossterm::event::KeyCode;
    use crossterm::event::KeyEvent;
    use crossterm::event::KeyModifiers;

    let (tx, _rx) = unbounded_channel::<AppEvent>();
    let sender = AppEventSender::new(tx);
    let mut composer = ChatComposer::new(
        /*has_input_focus*/ true,
        sender,
        /*enhanced_keys_supported*/ false,
        "Ask Praxis to do anything".to_string(),
        /*disable_paste_burst*/ false,
    );

    let path1 = PathBuf::from("/tmp/image_first.png");
    let path2 = PathBuf::from("/tmp/image_second.png");
    let placeholder1 = local_image_label_text(/*label_number*/ 1);
    let placeholder2 = local_image_label_text(/*label_number*/ 2);

    // Placeholders can be reordered in the text buffer; deleting image #1 should renumber
    // image #2 wherever it appears, not just after the cursor.
    let text = format!("Test {placeholder2} test {placeholder1}");
    let start2 = text.find(&placeholder2).expect("placeholder2 present");
    let start1 = text.find(&placeholder1).expect("placeholder1 present");
    let text_elements = vec![
        TextElement::new(
            ByteRange {
                start: start2,
                end: start2 + placeholder2.len(),
            },
            Some(placeholder2),
        ),
        TextElement::new(
            ByteRange {
                start: start1,
                end: start1 + placeholder1.len(),
            },
            Some(placeholder1.clone()),
        ),
    ];
    composer.set_text_content(text, text_elements, vec![path1, path2.clone()]);

    let end1 = start1 + placeholder1.len();
    composer.textarea.set_cursor(end1);

    composer.handle_key_event(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));

    assert_eq!(
        composer.textarea.text(),
        format!("Test {placeholder1} test ")
    );
    assert_eq!(
        vec![AttachedImage {
            path: path2,
            placeholder: placeholder1
        }],
        composer.attached_images,
        "attachment renumbered after deletion"
    );
}

#[test]
fn deleting_first_text_element_renumbers_following_text_element() {
    use crossterm::event::KeyCode;
    use crossterm::event::KeyEvent;
    use crossterm::event::KeyModifiers;

    let (tx, _rx) = unbounded_channel::<AppEvent>();
    let sender = AppEventSender::new(tx);
    let mut composer = ChatComposer::new(
        /*has_input_focus*/ true,
        sender,
        /*enhanced_keys_supported*/ false,
        "Ask Praxis to do anything".to_string(),
        /*disable_paste_burst*/ false,
    );

    let path1 = PathBuf::from("/tmp/image_first.png");
    let path2 = PathBuf::from("/tmp/image_second.png");

    // Insert two adjacent atomic elements.
    composer.attach_image(path1);
    composer.attach_image(path2.clone());
    assert_eq!(composer.textarea.text(), "[Image #1][Image #2]");
    assert_eq!(composer.attached_images.len(), 2);

    // Delete the first element using normal textarea editing (forward Delete at cursor start).
    composer.textarea.set_cursor(/*pos*/ 0);
    composer.handle_key_event(KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE));

    // Remaining image should be renumbered and the textarea element updated.
    assert_eq!(composer.attached_images.len(), 1);
    assert_eq!(composer.attached_images[0].path, path2);
    assert_eq!(composer.attached_images[0].placeholder, "[Image #1]");
    assert_eq!(composer.textarea.text(), "[Image #1]");
}

#[test]
fn pasting_filepath_attaches_image() {
    let tmp = tempdir().expect("create TempDir");
    let tmp_path: PathBuf = tmp.path().join("praxis_tui_test_paste_image.png");
    let img: ImageBuffer<Rgba<u8>, Vec<u8>> =
        ImageBuffer::from_fn(3, 2, |_x, _y| Rgba([1, 2, 3, 255]));
    img.save(&tmp_path).expect("failed to write temp png");

    let (tx, _rx) = unbounded_channel::<AppEvent>();
    let sender = AppEventSender::new(tx);
    let mut composer = ChatComposer::new(
        /*has_input_focus*/ true,
        sender,
        /*enhanced_keys_supported*/ false,
        "Ask Praxis to do anything".to_string(),
        /*disable_paste_burst*/ false,
    );

    let needs_redraw = composer.handle_paste(tmp_path.to_string_lossy().to_string());
    assert!(needs_redraw);
    assert!(composer.textarea.text().starts_with("[Image #1] "));

    let imgs = composer.take_recent_submission_images();
    assert_eq!(imgs, vec![tmp_path]);
}
