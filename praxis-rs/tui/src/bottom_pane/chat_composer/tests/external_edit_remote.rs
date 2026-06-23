use super::*;

#[test]
fn apply_external_edit_rebuilds_text_and_attachments() {
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
    composer.textarea.insert_element(&placeholder);
    composer.attached_images.push(AttachedImage {
        placeholder: placeholder.clone(),
        path: PathBuf::from("img.png"),
    });
    composer
        .pending_pastes
        .push(("[Pasted]".to_string(), "data".to_string()));

    composer.apply_external_edit(format!("Edited {placeholder} text"));

    assert_eq!(
        composer.current_text(),
        format!("Edited {placeholder} text")
    );
    assert!(composer.pending_pastes.is_empty());
    assert_eq!(composer.attached_images.len(), 1);
    assert_eq!(composer.attached_images[0].placeholder, placeholder);
    assert_eq!(composer.textarea.cursor(), composer.current_text().len());
}

#[test]
fn apply_external_edit_drops_missing_attachments() {
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
    composer.textarea.insert_element(&placeholder);
    composer.attached_images.push(AttachedImage {
        placeholder: placeholder.clone(),
        path: PathBuf::from("img.png"),
    });

    composer.apply_external_edit("No images here".to_string());

    assert_eq!(composer.current_text(), "No images here".to_string());
    assert!(composer.attached_images.is_empty());
}

#[test]
fn apply_external_edit_renumbers_image_placeholders() {
    let (tx, _rx) = unbounded_channel::<AppEvent>();
    let sender = AppEventSender::new(tx);
    let mut composer = ChatComposer::new(
        /*has_input_focus*/ true,
        sender,
        /*enhanced_keys_supported*/ false,
        "Ask Praxis to do anything".to_string(),
        /*disable_paste_burst*/ false,
    );

    let first_path = PathBuf::from("img1.png");
    let second_path = PathBuf::from("img2.png");
    composer.attach_image(first_path);
    composer.attach_image(second_path.clone());

    let placeholder2 = local_image_label_text(/*label_number*/ 2);
    composer.apply_external_edit(format!("Keep {placeholder2}"));

    let placeholder1 = local_image_label_text(/*label_number*/ 1);
    assert_eq!(composer.current_text(), format!("Keep {placeholder1}"));
    assert_eq!(composer.attached_images.len(), 1);
    assert_eq!(composer.attached_images[0].placeholder, placeholder1);
    assert_eq!(composer.local_image_paths(), vec![second_path]);
    assert_eq!(composer.textarea.element_payloads(), vec![placeholder1]);
}

#[test]
fn current_text_with_pending_expands_placeholders() {
    let (tx, _rx) = unbounded_channel::<AppEvent>();
    let sender = AppEventSender::new(tx);
    let mut composer = ChatComposer::new(
        /*has_input_focus*/ true,
        sender,
        /*enhanced_keys_supported*/ false,
        "Ask Praxis to do anything".to_string(),
        /*disable_paste_burst*/ false,
    );

    let placeholder = "[Pasted Content 5 chars]".to_string();
    composer.textarea.insert_element(&placeholder);
    composer
        .pending_pastes
        .push((placeholder.clone(), "hello".to_string()));

    assert_eq!(
        composer.current_text_with_pending(),
        "hello".to_string(),
        "placeholder should expand to actual text"
    );
}

#[test]
fn apply_external_edit_limits_duplicates_to_occurrences() {
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
    composer.textarea.insert_element(&placeholder);
    composer.attached_images.push(AttachedImage {
        placeholder: placeholder.clone(),
        path: PathBuf::from("img.png"),
    });

    composer.apply_external_edit(format!("{placeholder} extra {placeholder}"));

    assert_eq!(
        composer.current_text(),
        format!("{placeholder} extra {placeholder}")
    );
    assert_eq!(composer.attached_images.len(), 1);
}

#[test]
fn remote_images_do_not_modify_textarea_text_or_elements() {
    let (tx, _rx) = unbounded_channel::<AppEvent>();
    let sender = AppEventSender::new(tx);
    let mut composer = ChatComposer::new(
        /*has_input_focus*/ true,
        sender,
        /*enhanced_keys_supported*/ false,
        "Ask Praxis to do anything".to_string(),
        /*disable_paste_burst*/ false,
    );

    composer.set_remote_image_urls(vec![
        "https://example.com/one.png".to_string(),
        "https://example.com/two.png".to_string(),
    ]);

    assert_eq!(composer.current_text(), "");
    assert_eq!(composer.text_elements(), Vec::<TextElement>::new());
}

#[test]
fn attach_image_after_remote_prefix_uses_offset_label() {
    let (tx, _rx) = unbounded_channel::<AppEvent>();
    let sender = AppEventSender::new(tx);
    let mut composer = ChatComposer::new(
        /*has_input_focus*/ true,
        sender,
        /*enhanced_keys_supported*/ false,
        "Ask Praxis to do anything".to_string(),
        /*disable_paste_burst*/ false,
    );

    composer.set_remote_image_urls(vec![
        "https://example.com/one.png".to_string(),
        "https://example.com/two.png".to_string(),
    ]);
    composer.attach_image(PathBuf::from("/tmp/local.png"));

    assert_eq!(composer.attached_images[0].placeholder, "[Image #3]");
    assert_eq!(composer.current_text(), "[Image #3]");
}

#[test]
fn prepare_submission_keeps_remote_offset_local_placeholder_numbering() {
    let (tx, _rx) = unbounded_channel::<AppEvent>();
    let sender = AppEventSender::new(tx);
    let mut composer = ChatComposer::new(
        /*has_input_focus*/ true,
        sender,
        /*enhanced_keys_supported*/ false,
        "Ask Praxis to do anything".to_string(),
        /*disable_paste_burst*/ false,
    );

    composer.set_remote_image_urls(vec!["https://example.com/one.png".to_string()]);
    let base_text = "[Image #2] hello".to_string();
    let base_elements = vec![TextElement::new(
        (0.."[Image #2]".len()).into(),
        Some("[Image #2]".to_string()),
    )];
    composer.set_text_content(
        base_text,
        base_elements,
        vec![PathBuf::from("/tmp/local.png")],
    );

    let (submitted_text, submitted_elements) = composer
        .prepare_submission_text(/*record_history*/ true)
        .expect("remote+local submission should be generated");
    assert_eq!(submitted_text, "[Image #2] hello");
    assert_eq!(
        submitted_elements,
        vec![TextElement::new(
            (0.."[Image #2]".len()).into(),
            Some("[Image #2]".to_string())
        )]
    );
}

#[test]
fn prepare_submission_with_only_remote_images_returns_empty_text() {
    let (tx, _rx) = unbounded_channel::<AppEvent>();
    let sender = AppEventSender::new(tx);
    let mut composer = ChatComposer::new(
        /*has_input_focus*/ true,
        sender,
        /*enhanced_keys_supported*/ false,
        "Ask Praxis to do anything".to_string(),
        /*disable_paste_burst*/ false,
    );

    composer.set_remote_image_urls(vec!["https://example.com/one.png".to_string()]);
    let (submitted_text, submitted_elements) = composer
        .prepare_submission_text(/*record_history*/ true)
        .expect("remote-only submission should be generated");
    assert_eq!(submitted_text, "");
    assert!(submitted_elements.is_empty());
}

#[test]
fn delete_selected_remote_image_relabels_local_placeholders() {
    let (tx, _rx) = unbounded_channel::<AppEvent>();
    let sender = AppEventSender::new(tx);
    let mut composer = ChatComposer::new(
        /*has_input_focus*/ true,
        sender,
        /*enhanced_keys_supported*/ false,
        "Ask Praxis to do anything".to_string(),
        /*disable_paste_burst*/ false,
    );

    composer.set_remote_image_urls(vec![
        "https://example.com/one.png".to_string(),
        "https://example.com/two.png".to_string(),
    ]);
    composer.attach_image(PathBuf::from("/tmp/local.png"));
    composer.textarea.set_cursor(/*pos*/ 0);

    let _ = composer.handle_key_event(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
    let _ = composer.handle_key_event(KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE));
    assert_eq!(
        composer.remote_image_urls(),
        vec!["https://example.com/one.png".to_string()]
    );
    assert_eq!(composer.current_text(), "[Image #2]");
    assert_eq!(composer.attached_images[0].placeholder, "[Image #2]");

    let _ = composer.handle_key_event(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
    let _ = composer.handle_key_event(KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE));
    assert_eq!(composer.remote_image_urls(), Vec::<String>::new());
    assert_eq!(composer.current_text(), "[Image #1]");
    assert_eq!(composer.attached_images[0].placeholder, "[Image #1]");
}

#[test]
fn input_disabled_ignores_keypresses_and_hides_cursor() {
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

    composer.set_text_content("hello".to_string(), Vec::new(), Vec::new());
    composer.set_input_enabled(
        /*enabled*/ false,
        Some("Input disabled for test.".to_string()),
    );

    let (result, needs_redraw) =
        composer.handle_key_event(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));

    assert_eq!(result, InputResult::None);
    assert!(!needs_redraw);
    assert_eq!(composer.current_text(), "hello");

    let area = Rect {
        x: 0,
        y: 0,
        width: 40,
        height: 5,
    };
    assert_eq!(composer.cursor_pos(area), None);
}
