use crate::client_common::ResponseEvent;
use crate::client_common::ResponseStream;
use praxis_protocol::models::ContentItem;
use praxis_protocol::models::ResponseItem;
use tokio::sync::mpsc;

const CHANNEL_OPEN: &str = "<|channel>";
const CHANNEL_CLOSE: &str = "<channel|>";

pub(super) fn filter_native_local_output(mut stream: ResponseStream) -> ResponseStream {
    let (tx_event, rx_event) = mpsc::channel(64);
    tokio::spawn(async move {
        let mut sanitizer = LocalChannelSanitizer::default();
        while let Some(event) = stream.rx_event.recv().await {
            let event = match event {
                Ok(event) => sanitize_event(event, &mut sanitizer).map(Ok),
                Err(err) => Some(Err(err)),
            };
            let Some(event) = event else {
                continue;
            };
            if tx_event.send(event).await.is_err() {
                break;
            }
        }
    });
    ResponseStream { rx_event }
}

fn sanitize_event(
    event: ResponseEvent,
    sanitizer: &mut LocalChannelSanitizer,
) -> Option<ResponseEvent> {
    match event {
        ResponseEvent::OutputTextDelta(delta) => {
            let delta = sanitizer.push(&delta);
            (!delta.is_empty()).then_some(ResponseEvent::OutputTextDelta(delta))
        }
        ResponseEvent::OutputItemDone(item) => {
            Some(ResponseEvent::OutputItemDone(sanitize_response_item(item)))
        }
        ResponseEvent::OutputItemAdded(item) => {
            Some(ResponseEvent::OutputItemAdded(sanitize_response_item(item)))
        }
        other => Some(other),
    }
}

fn sanitize_response_item(mut item: ResponseItem) -> ResponseItem {
    if let ResponseItem::Message { content, .. } = &mut item {
        for item in content {
            match item {
                ContentItem::InputText { text } | ContentItem::OutputText { text } => {
                    *text = sanitize_complete_text(text);
                }
                ContentItem::InputImage { .. } => {}
            }
        }
    }
    item
}

fn sanitize_complete_text(text: &str) -> String {
    let mut sanitizer = LocalChannelSanitizer::default();
    let mut clean = sanitizer.push(text);
    clean.push_str(&sanitizer.flush());
    clean
}

#[derive(Default)]
struct LocalChannelSanitizer {
    pending: String,
}

impl LocalChannelSanitizer {
    fn push(&mut self, text: &str) -> String {
        self.pending.push_str(text);
        self.drain(false)
    }

    fn flush(&mut self) -> String {
        self.drain(true)
    }

    fn drain(&mut self, flush: bool) -> String {
        let mut input = std::mem::take(&mut self.pending);
        let mut output = String::new();
        loop {
            if let Some(start) = input.find(CHANNEL_OPEN) {
                output.push_str(&input[..start]);
                let after_open = &input[start + CHANNEL_OPEN.len()..];
                if let Some(close) = after_open.find(CHANNEL_CLOSE) {
                    input = after_open[close + CHANNEL_CLOSE.len()..].to_string();
                    continue;
                }
                if !flush {
                    self.pending = input[start..].to_string();
                }
                return output;
            }
            if let Some(close) = input.find(CHANNEL_CLOSE) {
                output.push_str(&input[..close]);
                input = input[close + CHANNEL_CLOSE.len()..].to_string();
                continue;
            }
            let keep = if flush {
                0
            } else {
                partial_marker_suffix_len(&input)
            };
            if keep == 0 {
                output.push_str(&input);
                self.pending.clear();
            } else {
                let split = input.len() - keep;
                output.push_str(&input[..split]);
                self.pending = input[split..].to_string();
            }
            return output;
        }
    }
}

fn partial_marker_suffix_len(input: &str) -> usize {
    [CHANNEL_OPEN, CHANNEL_CLOSE]
        .into_iter()
        .flat_map(|marker| (1..marker.len()).map(move |len| &marker[..len]))
        .filter(|prefix| input.ends_with(*prefix))
        .map(str::len)
        .max()
        .unwrap_or(0)
}
