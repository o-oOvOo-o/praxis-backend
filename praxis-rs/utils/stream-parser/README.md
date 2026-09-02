# praxis-utils-stream-parser

Small, dependency-free utilities for deterministic incremental text parsing.

## What it provides

- `TextProjector`: host-composable stage for incremental model text
- `TextProjection<E>`: renderable text plus typed events emitted by a stage
- `HiddenTagProjector<T>`: generic stage that hides inline tags and emits their contents
- `CitationProjector`: convenience stage for `<praxis-memory-citation>...</praxis-memory-citation>`
- `strip_citations(...)`: one-shot helper for non-streamed strings
- `Utf8ProjectionAdapter<P>`: transactional adapter for raw byte streams

## Design contracts

- Public parsers are state machines; callers never need to align chunks with characters, lines, or tags.
- Text is emitted only after it can no longer be a delimiter prefix.
- Inline parsing has explicit visible and hidden states; line parsing has explicit probe and stream states.
- Literal selection is shared: earliest delimiter wins, then the longest delimiter, then declaration order.
- UTF-8 decoding is transactional per pushed byte chunk. Invalid bytes never partially reach the wrapped parser.
- `finish()` drains pending state, auto-closes an active tag or block, and is then idempotent.

The public behavior is protected by both immediate-output unit tests and exhaustive character/byte split
contracts. Internal parser organization may evolve without changing rendered text, extracted payload order,
or stream-finalization semantics.

Run `cargo run -p praxis-utils-stream-parser --example stream_contract` for the dependency-free public API
contract exercised by repository validation environments that compile through runnable targets.

## Why this exists

Some model outputs arrive as a stream and may contain hidden markup (for example
`<praxis-memory-citation>...</praxis-memory-citation>`) split across chunk boundaries. Parsing each chunk
independently is incorrect because tags can be split (`<praxis-memory-` + `citation>`).

This crate projects a model stream into renderable text and typed events. Host
code chooses the stages it needs; renderers never need to understand hidden
model markup.

## Example: citation streaming

```rust
use praxis_utils_stream_parser::CitationProjector;
use praxis_utils_stream_parser::TextProjector;

let mut parser = CitationProjector::new();

let first = parser.project("Hello <praxis-memory-");
assert_eq!(first.renderable, "Hello ");
assert!(first.events.is_empty());

let second = parser.project("citation>doc A</praxis-memory-citation> world");
assert_eq!(second.renderable, " world");
assert_eq!(second.events, vec!["doc A".to_string()]);

let tail = parser.close();
assert!(tail.renderable.is_empty());
assert!(tail.events.is_empty());
```

## Example: raw byte streaming with split UTF-8 code points

```rust
use praxis_utils_stream_parser::CitationProjector;
use praxis_utils_stream_parser::Utf8ProjectionAdapter;

# fn demo() -> Result<(), praxis_utils_stream_parser::Utf8ProjectionError> {
let mut parser = Utf8ProjectionAdapter::new(CitationProjector::new());

// "é" split across chunks: 0xC3 + 0xA9
let first = parser.push_bytes(&[b'H', 0xC3])?;
assert_eq!(first.renderable, "H");

let second = parser.push_bytes(&[0xA9, b'!'])?;
assert_eq!(second.renderable, "é!");

let tail = parser.close()?;
assert!(tail.renderable.is_empty());
# Ok(())
# }
```

## Example: custom hidden tags

```rust
use praxis_utils_stream_parser::HiddenTagProjector;
use praxis_utils_stream_parser::InlineTagSpec;
use praxis_utils_stream_parser::TextProjector;

#[derive(Clone, Debug, PartialEq, Eq)]
enum Tag {
    Secret,
}

let mut parser = HiddenTagProjector::new(vec![InlineTagSpec {
    tag: Tag::Secret,
    open: "<secret>",
    close: "</secret>",
}]);

let out = parser.project("a<secret>x</secret>b");
assert_eq!(out.renderable, "ab");
assert_eq!(out.events.len(), 1);
assert_eq!(out.events[0].content, "x");
```

## Known limitations

- Tags are matched literally and case-sensitively
- No nested tag support
- A push may produce an empty chunk while a possible delimiter is buffered
