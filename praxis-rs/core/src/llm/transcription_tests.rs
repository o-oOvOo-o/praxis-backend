use super::*;

fn audio() -> TranscriptionAudio {
    TranscriptionAudio {
        media_type: "audio/wav".to_string(),
        bytes: b"RIFF".to_vec(),
    }
}

#[test]
fn builds_audio_data_uri() {
    assert_eq!(audio_data_uri(&audio()), "data:audio/wav;base64,UklGRg==");
}

#[test]
fn parses_openai_chat_audio_response() {
    let body = r#"{"choices":[{"message":{"content":"hello world"}}]}"#;

    let text = parse_text_response("qwen", body, Some("choices.0.message.content")).unwrap();

    assert_eq!(text, "hello world");
}

#[test]
fn parses_plain_text_process_response() {
    let text = parse_process_stdout("local", b"hello from local\n", None).unwrap();

    assert_eq!(text, "hello from local");
}

#[test]
fn parses_llama_mtmd_qwen_asr_marker() {
    let body = b"\nlanguage English<asr_text>Hello, Praxis Voice Input.\n";

    let text = parse_llama_mtmd_stdout("qwen3-asr", body, None).unwrap();

    assert_eq!(text, "Hello, Praxis Voice Input.");
}

#[test]
fn builds_plain_json_request_with_language_and_extra_body() {
    let request = TranscriptionRequest {
        provider_id: None,
        model: None,
        audio: audio(),
        language: Some("zh".to_string()),
    };
    let extra = serde_json::from_value::<Map<String, Value>>(json!({
        "hotwords": ["Praxis", "Cunning3D"]
    }))
    .unwrap();

    let body = plain_json_request("qwen3-asr-flash", &request, Some(&extra));

    assert_eq!(body["model"], "qwen3-asr-flash");
    assert_eq!(body["language"], "zh");
    assert_eq!(body["audio"]["data"], "UklGRg==");
    assert_eq!(body["hotwords"][0], "Praxis");
}

#[test]
fn builds_default_whisper_cpp_args() {
    let provider = TranscriptionProviderConfig {
        kind: TranscriptionProviderKind::LocalProcess,
        model: None,
        base_url: None,
        api_key_env: None,
        command: Some("whisper-cli".to_string()),
        args: Vec::new(),
        env: BTreeMap::new(),
        timeout_ms: None,
        metadata: BTreeMap::new(),
    };

    let args = whisper_cpp_args(
        &provider,
        "F:/models/model.gguf",
        Path::new("F:/tmp/input.wav"),
        Path::new("F:/tmp/transcript"),
        "zh",
    );

    assert_eq!(args[0], "-m");
    assert!(args.contains(&"F:/models/model.gguf".to_string()));
    assert!(args.contains(&"F:/tmp/input.wav".to_string()));
    assert!(args.contains(&"F:/tmp/transcript".to_string()));
    assert!(args.contains(&"zh".to_string()));
    assert!(args.contains(&"-otxt".to_string()));
}

#[test]
fn expands_custom_whisper_cpp_arg_placeholders() {
    let provider = TranscriptionProviderConfig {
        kind: TranscriptionProviderKind::LocalProcess,
        model: None,
        base_url: None,
        api_key_env: None,
        command: Some("whisper-cli".to_string()),
        args: vec![
            "--model".to_string(),
            "{model}".to_string(),
            "--file={audio}".to_string(),
            "--output={output}".to_string(),
            "--language={language}".to_string(),
        ],
        env: BTreeMap::new(),
        timeout_ms: None,
        metadata: BTreeMap::new(),
    };

    let args = whisper_cpp_args(
        &provider,
        "F:/models/model.gguf",
        Path::new("F:/tmp/input.wav"),
        Path::new("F:/tmp/transcript"),
        "auto",
    );

    assert_eq!(
        args,
        vec![
            "--model",
            "F:/models/model.gguf",
            "--file=F:/tmp/input.wav",
            "--output=F:/tmp/transcript",
            "--language=auto",
        ]
    );
}

#[test]
fn builds_default_llama_mtmd_args() {
    let provider = TranscriptionProviderConfig {
        kind: TranscriptionProviderKind::LocalProcess,
        model: None,
        base_url: None,
        api_key_env: None,
        command: Some("llama-mtmd-cli".to_string()),
        args: Vec::new(),
        env: BTreeMap::new(),
        timeout_ms: None,
        metadata: BTreeMap::new(),
    };

    let args = llama_mtmd_args(
        &provider,
        "F:/models/qwen.gguf",
        "F:/models/mmproj.gguf",
        Path::new("F:/tmp/input.wav"),
        "Transcribe the audio.",
    );

    assert!(args.contains(&"-m".to_string()));
    assert!(args.contains(&"F:/models/qwen.gguf".to_string()));
    assert!(args.contains(&"--mmproj".to_string()));
    assert!(args.contains(&"F:/models/mmproj.gguf".to_string()));
    assert!(args.contains(&"--audio".to_string()));
    assert!(args.contains(&"F:/tmp/input.wav".to_string()));
    assert!(args.contains(&"Transcribe the audio.".to_string()));
}

#[test]
fn expands_custom_llama_mtmd_arg_placeholders() {
    let provider = TranscriptionProviderConfig {
        kind: TranscriptionProviderKind::LocalProcess,
        model: None,
        base_url: None,
        api_key_env: None,
        command: Some("llama-mtmd-cli".to_string()),
        args: vec![
            "-m".to_string(),
            "{model}".to_string(),
            "--mmproj={mmproj}".to_string(),
            "--audio={audio}".to_string(),
            "-p".to_string(),
            "{prompt}".to_string(),
        ],
        env: BTreeMap::new(),
        timeout_ms: None,
        metadata: BTreeMap::new(),
    };

    let args = llama_mtmd_args(
        &provider,
        "F:/models/qwen.gguf",
        "F:/models/mmproj.gguf",
        Path::new("F:/tmp/input.wav"),
        "Transcribe.",
    );

    assert_eq!(
        args,
        vec![
            "-m",
            "F:/models/qwen.gguf",
            "--mmproj=F:/models/mmproj.gguf",
            "--audio=F:/tmp/input.wav",
            "-p",
            "Transcribe.",
        ]
    );
}
