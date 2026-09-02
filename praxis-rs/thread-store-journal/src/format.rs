mod stored_events;

use self::stored_events::StoredEvents;
use crate::JournalBatch;
use crate::JournalDurability;
use praxis_thread_store_contracts::AchievedDurability;
use praxis_thread_store_contracts::BatchId;
use praxis_thread_store_contracts::CanonicalEncode;
use praxis_thread_store_contracts::CanonicalHasher;
use praxis_thread_store_contracts::CommandId;
use praxis_thread_store_contracts::CommittedEventRef;
use praxis_thread_store_contracts::Digest;
use praxis_thread_store_contracts::ThreadCommandReceipt;
use praxis_thread_store_contracts::ThreadEventEnvelope;
use praxis_thread_store_contracts::ThreadHead;
use praxis_thread_store_contracts::ThreadId;
use praxis_thread_store_contracts::ThreadRevision;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashSet;
use std::io::Write;

pub(crate) const SEGMENT_HEADER_LEN: usize = 80;
pub(crate) const FRAME_HEADER_LEN: usize = 184;
pub(crate) const FRAME_TRAILER_LEN: usize = 52;
const SEGMENT_MAGIC: &[u8; 8] = b"PRXTSJ01";
const FRAME_MAGIC: &[u8; 8] = b"PRXBTH01";
const TRAILER_MAGIC: &[u8; 8] = b"PRXCOM01";
const FORMAT_VERSION: u16 = 1;
const JSON_CODEC: u8 = 1;
const BATCH_DIGEST_DOMAIN: &str = "praxis.thread-store.journal-batch.v1";
const LEGACY_STORED_BATCH_SCHEMA_VERSION: u32 = 1;
const STORED_BATCH_SCHEMA_VERSION: u32 = 2;
const STORED_RECEIPT_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug)]
pub(crate) struct SegmentHeader {
    pub sequence: u64,
    pub first_revision: ThreadRevision,
    pub thread_id: [u8; 16],
    pub previous_record_digest: Digest,
}

#[derive(Clone, Debug)]
pub(crate) struct FrameHeader {
    pub payload_len: u64,
    pub start_revision: ThreadRevision,
    pub end_revision: ThreadRevision,
    pub event_count: u32,
    pub batch_id: [u8; 16],
    pub command_id: [u8; 16],
    pub command_digest: Digest,
    pub previous_record_digest: Digest,
    pub last_record_digest: Digest,
    pub payload_crc32c: u32,
}

#[derive(Clone, Debug)]
pub(crate) struct EncodedFrame {
    pub bytes: Vec<u8>,
    pub header: FrameHeader,
    pub stored: StoredBatch,
}

#[derive(Clone, Debug)]
pub(crate) struct DecodedFrame {
    pub header: FrameHeader,
    pub stored: StoredBatch,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StoredBatch {
    schema_version: u32,
    thread_id: ThreadId,
    batch_id: BatchId,
    command_id: CommandId,
    command_digest: Digest,
    expected_revision: ThreadRevision,
    recorded_at_unix_ms: i64,
    batch_digest: Digest,
    #[serde(default, rename = "receipt", skip_serializing_if = "Option::is_none")]
    legacy_receipt: Option<ThreadCommandReceipt>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    durability: Option<AchievedDurability>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    receipt_schema_version: Option<u32>,
    events: StoredEvents,
}

impl StoredBatch {
    pub(crate) const fn command_id(&self) -> CommandId {
        self.command_id
    }

    pub(crate) const fn command_digest(&self) -> Digest {
        self.command_digest
    }

    pub(crate) fn into_receipt(self) -> Result<ThreadCommandReceipt, String> {
        self.derive_receipt()
    }

    pub(crate) fn into_receipt_and_events(
        self,
    ) -> Result<(ThreadCommandReceipt, Vec<ThreadEventEnvelope>), String> {
        let receipt = self.derive_receipt()?;
        Ok((receipt, self.events.into_vec()))
    }

    fn derive_receipt(&self) -> Result<ThreadCommandReceipt, String> {
        match self.schema_version {
            LEGACY_STORED_BATCH_SCHEMA_VERSION => {
                return self
                    .legacy_receipt
                    .clone()
                    .ok_or_else(|| "legacy stored batch is missing its receipt".to_string());
            }
            STORED_BATCH_SCHEMA_VERSION => {}
            _ => return Err("unsupported stored batch schema version".to_string()),
        }
        let durability = self
            .durability
            .ok_or_else(|| "stored batch is missing its durability".to_string())?;
        let receipt_schema_version = self
            .receipt_schema_version
            .ok_or_else(|| "stored batch is missing its receipt schema version".to_string())?;
        let events = self
            .events
            .iter()
            .map(|event| CommittedEventRef {
                event_id: event.event_id,
                revision: event.revision,
            })
            .collect();
        let mut receipt = ThreadCommandReceipt::applied(
            self.command_id,
            self.command_digest,
            self.batch_id,
            self.expected_revision,
            events,
            self.batch_digest,
            durability,
            self.recorded_at_unix_ms,
        );
        receipt.schema_version = receipt_schema_version;
        Ok(receipt)
    }

    pub(crate) fn into_events(self) -> impl ExactSizeIterator<Item = ThreadEventEnvelope> {
        self.events.consume()
    }
}

pub(crate) fn encode_segment_header(
    sequence: u64,
    first_revision: ThreadRevision,
    thread_id: ThreadId,
    previous_record_digest: Digest,
) -> [u8; SEGMENT_HEADER_LEN] {
    let mut bytes = Vec::with_capacity(SEGMENT_HEADER_LEN);
    bytes.extend_from_slice(SEGMENT_MAGIC);
    push_u16(&mut bytes, FORMAT_VERSION);
    push_u16(&mut bytes, SEGMENT_HEADER_LEN as u16);
    push_u64(&mut bytes, sequence);
    push_u64(&mut bytes, first_revision.get());
    bytes.extend_from_slice(thread_id.as_uuid().as_bytes());
    bytes.extend_from_slice(previous_record_digest.as_bytes());
    let crc = crc32c::crc32c(&bytes);
    push_u32(&mut bytes, crc);
    let mut header = [0; SEGMENT_HEADER_LEN];
    header.copy_from_slice(&bytes);
    header
}

pub(crate) fn decode_segment_header(bytes: &[u8]) -> Result<SegmentHeader, String> {
    if bytes.len() != SEGMENT_HEADER_LEN {
        return Err("incomplete segment header".to_string());
    }
    if &bytes[..8] != SEGMENT_MAGIC {
        return Err("invalid segment magic".to_string());
    }
    if u16_at(bytes, 8) != FORMAT_VERSION {
        return Err("unsupported segment format version".to_string());
    }
    if usize::from(u16_at(bytes, 10)) != SEGMENT_HEADER_LEN {
        return Err("invalid segment header length".to_string());
    }
    if u32_at(bytes, SEGMENT_HEADER_LEN - 4) != crc32c::crc32c(&bytes[..SEGMENT_HEADER_LEN - 4]) {
        return Err("segment header CRC32C mismatch".to_string());
    }
    let mut thread_id = [0; 16];
    thread_id.copy_from_slice(&bytes[28..44]);
    let mut digest = [0; 32];
    digest.copy_from_slice(&bytes[44..76]);
    Ok(SegmentHeader {
        sequence: u64_at(bytes, 12),
        first_revision: ThreadRevision::new(u64_at(bytes, 20)),
        thread_id,
        previous_record_digest: Digest::from_bytes(digest),
    })
}

pub(crate) fn encode_batch(
    batch: JournalBatch,
    durability: JournalDurability,
    max_payload_bytes: u64,
) -> Result<EncodedFrame, String> {
    let batch_digest = compute_batch_digest(&batch);
    let stored = StoredBatch {
        schema_version: STORED_BATCH_SCHEMA_VERSION,
        thread_id: batch.thread_id,
        batch_id: batch.batch_id,
        command_id: batch.command_id,
        command_digest: batch.command_digest,
        expected_revision: batch.expected_revision,
        recorded_at_unix_ms: batch.recorded_at_unix_ms,
        batch_digest,
        legacy_receipt: None,
        durability: Some(match durability {
            JournalDurability::Buffered => AchievedDurability::Buffered,
            JournalDurability::Durable => AchievedDurability::Durable,
        }),
        receipt_schema_version: Some(STORED_RECEIPT_SCHEMA_VERSION),
        // Preserve the caller's allocation on append; only decoded single-event frames inline.
        events: StoredEvents::preserve_allocation(batch.events),
    };
    let first = stored
        .events
        .first()
        .ok_or_else(|| "event batch is empty".to_string())?;
    let last = stored
        .events
        .last()
        .ok_or_else(|| "event batch is empty".to_string())?;
    let start_revision = first.revision;
    let end_revision = last.revision;
    let previous_record_digest = first.previous_record_digest;
    let last_record_digest = last.record_digest;
    let event_count = u32::try_from(stored.events.len())
        .map_err(|_| "event count exceeds the frame format".to_string())?;
    let mut bytes = Vec::with_capacity(FRAME_HEADER_LEN);
    bytes.resize(FRAME_HEADER_LEN, 0);
    serde_json::to_writer(
        PayloadWriter {
            bytes: &mut bytes,
            max_payload_bytes,
        },
        &stored,
    )
    .map_err(|error| error.to_string())?;
    let payload = &bytes[FRAME_HEADER_LEN..];
    let payload_len =
        u64::try_from(payload.len()).map_err(|_| "batch payload length overflow".to_string())?;
    let header = FrameHeader {
        payload_len,
        start_revision,
        end_revision,
        event_count,
        batch_id: *stored.batch_id.as_uuid().as_bytes(),
        command_id: *stored.command_id.as_uuid().as_bytes(),
        command_digest: stored.command_digest,
        previous_record_digest,
        last_record_digest,
        payload_crc32c: crc32c::crc32c(&payload),
    };
    let header_bytes = encode_frame_header(&header);
    let frame_len = bytes
        .len()
        .checked_add(FRAME_TRAILER_LEN)
        .ok_or_else(|| "frame length overflow".to_string())?;
    let frame_len = u64::try_from(frame_len).map_err(|_| "frame length exceeds u64".to_string())?;
    let trailer = encode_trailer(frame_len, stored.batch_digest);
    bytes[..FRAME_HEADER_LEN].copy_from_slice(&header_bytes);
    bytes.reserve(FRAME_TRAILER_LEN);
    bytes.extend_from_slice(&trailer);
    Ok(EncodedFrame {
        bytes,
        header,
        stored,
    })
}

struct PayloadWriter<'a> {
    bytes: &'a mut Vec<u8>,
    max_payload_bytes: u64,
}

impl Write for PayloadWriter<'_> {
    fn write(&mut self, input: &[u8]) -> std::io::Result<usize> {
        let current = self.bytes.len().saturating_sub(FRAME_HEADER_LEN);
        let next = current.checked_add(input.len()).ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "frame payload length overflow",
            )
        })?;
        if u64::try_from(next).map_or(true, |len| len > self.max_payload_bytes) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "frame payload exceeds configured maximum",
            ));
        }
        self.bytes.extend_from_slice(input);
        Ok(input.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

pub(crate) fn decode_frame_header(bytes: &[u8]) -> Result<FrameHeader, String> {
    if bytes.len() != FRAME_HEADER_LEN {
        return Err("incomplete frame header".to_string());
    }
    if &bytes[..8] != FRAME_MAGIC {
        return Err("invalid frame magic".to_string());
    }
    if u16_at(bytes, 8) != FORMAT_VERSION || bytes[10] != JSON_CODEC || bytes[11] != 0 {
        return Err("unsupported frame format or codec".to_string());
    }
    if usize::try_from(u32_at(bytes, 12)).ok() != Some(FRAME_HEADER_LEN) {
        return Err("invalid frame header length".to_string());
    }
    if u32_at(bytes, FRAME_HEADER_LEN - 4) != crc32c::crc32c(&bytes[..FRAME_HEADER_LEN - 4]) {
        return Err("frame header CRC32C mismatch".to_string());
    }
    let mut batch_id = [0; 16];
    batch_id.copy_from_slice(&bytes[48..64]);
    let mut command_id = [0; 16];
    command_id.copy_from_slice(&bytes[64..80]);
    Ok(FrameHeader {
        payload_len: u64_at(bytes, 16),
        start_revision: ThreadRevision::new(u64_at(bytes, 24)),
        end_revision: ThreadRevision::new(u64_at(bytes, 32)),
        event_count: u32_at(bytes, 40),
        batch_id,
        command_id,
        command_digest: digest_at(bytes, 80),
        previous_record_digest: digest_at(bytes, 112),
        last_record_digest: digest_at(bytes, 144),
        payload_crc32c: u32_at(bytes, 176),
    })
}

pub(crate) fn frame_len(header: &FrameHeader) -> Result<u64, String> {
    let fixed = u64::try_from(FRAME_HEADER_LEN + FRAME_TRAILER_LEN)
        .map_err(|_| "frame fixed length overflow".to_string())?;
    fixed
        .checked_add(header.payload_len)
        .ok_or_else(|| "frame length overflow".to_string())
}

pub(crate) fn decode_complete_frame(bytes: &[u8]) -> Result<DecodedFrame, String> {
    if bytes.len() < FRAME_HEADER_LEN + FRAME_TRAILER_LEN {
        return Err("incomplete frame".to_string());
    }
    let header = decode_frame_header(&bytes[..FRAME_HEADER_LEN])?;
    let expected_len = frame_len(&header)?;
    if u64::try_from(bytes.len()).ok() != Some(expected_len) {
        return Err("frame length does not match its header".to_string());
    }
    let payload_end = bytes.len() - FRAME_TRAILER_LEN;
    let payload = &bytes[FRAME_HEADER_LEN..payload_end];
    if crc32c::crc32c(payload) != header.payload_crc32c {
        return Err("frame payload CRC32C mismatch".to_string());
    }
    let trailer = &bytes[payload_end..];
    if &trailer[..8] != TRAILER_MAGIC {
        return Err("missing atomic batch commit trailer".to_string());
    }
    if u64_at(trailer, 8) != expected_len {
        return Err("commit trailer frame length mismatch".to_string());
    }
    if u32_at(trailer, FRAME_TRAILER_LEN - 4) != crc32c::crc32c(&trailer[..FRAME_TRAILER_LEN - 4]) {
        return Err("commit trailer CRC32C mismatch".to_string());
    }
    let stored: StoredBatch = serde_json::from_slice(payload)
        .map_err(|error| format!("invalid batch payload: {error}"))?;
    if digest_at(trailer, 16) != stored.batch_digest {
        return Err("commit trailer batch digest mismatch".to_string());
    }
    Ok(DecodedFrame { header, stored })
}

pub(crate) fn validate_stored_batch(
    header: &FrameHeader,
    stored: &StoredBatch,
    thread_id: ThreadId,
    previous_head: ThreadHead,
) -> Result<ThreadHead, String> {
    validate_receipt_encoding(stored)?;
    if stored.thread_id != thread_id
        || header.batch_id != *stored.batch_id.as_uuid().as_bytes()
        || header.command_id != *stored.command_id.as_uuid().as_bytes()
        || header.command_digest != stored.command_digest
    {
        return Err("frame identity does not match its payload".to_string());
    }
    if stored.expected_revision != previous_head.revision
        || header.previous_record_digest != previous_head.record_digest
    {
        return Err("batch does not continue the preceding thread head".to_string());
    }
    let count =
        usize::try_from(header.event_count).map_err(|_| "event count exceeds usize".to_string())?;
    if count == 0 || count != stored.events.len() {
        return Err("frame event count does not match its payload".to_string());
    }
    let expected_start = previous_head
        .revision
        .checked_next()
        .ok_or_else(|| "thread revision overflow".to_string())?;
    if header.start_revision != expected_start {
        return Err("frame start revision is not contiguous".to_string());
    }
    let mut prior = previous_head.record_digest;
    let check_duplicate_ids = stored.events.len() > 1;
    let mut event_ids = HashSet::new();
    if check_duplicate_ids {
        event_ids.reserve(stored.events.len());
    }
    for (index, event) in stored.events.iter().enumerate() {
        let offset = u64::try_from(index)
            .map_err(|_| "event index exceeds u64".to_string())?
            .checked_add(1)
            .ok_or_else(|| "event index overflow".to_string())?;
        let revision = previous_head
            .revision
            .checked_advance(offset)
            .ok_or_else(|| "thread revision overflow".to_string())?;
        if event.thread_id != thread_id
            || event.batch_id != stored.batch_id
            || event.sequence != u32::try_from(index).map_err(|_| "event index exceeds u32")?
            || event.revision != revision
            || event.previous_record_digest != prior
            || !event.has_valid_digests()
        {
            return Err(format!("invalid event at batch sequence {index}"));
        }
        if check_duplicate_ids && !event_ids.insert(event.event_id) {
            return Err("duplicate event id inside one batch".to_string());
        }
        prior = event.record_digest;
    }
    let last = stored
        .events
        .last()
        .ok_or_else(|| "event batch is empty".to_string())?;
    if header.end_revision != last.revision || header.last_record_digest != last.record_digest {
        return Err("frame end does not match its final event".to_string());
    }
    if stored.batch_digest != compute_stored_batch_digest(stored) {
        return Err("canonical batch digest mismatch".to_string());
    }
    Ok(ThreadHead {
        revision: last.revision,
        record_digest: last.record_digest,
    })
}

fn validate_receipt_encoding(stored: &StoredBatch) -> Result<(), String> {
    match stored.schema_version {
        LEGACY_STORED_BATCH_SCHEMA_VERSION => {
            if stored.durability.is_some() || stored.receipt_schema_version.is_some() {
                return Err("legacy stored batch contains canonical receipt metadata".to_string());
            }
            let receipt = stored
                .legacy_receipt
                .as_ref()
                .ok_or_else(|| "legacy stored batch is missing its receipt".to_string())?;
            receipt
                .validate_causality(stored.command_digest)
                .map_err(|error| format!("invalid stored receipt: {error}"))?;
            if receipt.command_id != stored.command_id
                || receipt.batch_id != Some(stored.batch_id)
                || !receipt_events_match(receipt, &stored.events)
                || receipt.batch_digest != Some(stored.batch_digest)
                || receipt.recorded_at_unix_ms != stored.recorded_at_unix_ms
                || !is_committed_durability(receipt.durability)
            {
                return Err("stored receipt does not describe its batch".to_string());
            }
        }
        STORED_BATCH_SCHEMA_VERSION => {
            if stored.legacy_receipt.is_some()
                || !stored.durability.is_some_and(is_committed_durability)
                || stored.receipt_schema_version != Some(STORED_RECEIPT_SCHEMA_VERSION)
            {
                return Err("canonical stored batch has invalid receipt metadata".to_string());
            }
        }
        _ => return Err("unsupported stored batch schema version".to_string()),
    }
    Ok(())
}

const fn is_committed_durability(durability: AchievedDurability) -> bool {
    matches!(
        durability,
        AchievedDurability::Buffered | AchievedDurability::Durable
    )
}

fn receipt_events_match(receipt: &ThreadCommandReceipt, events: &[ThreadEventEnvelope]) -> bool {
    receipt.events.len() == events.len()
        && receipt.events.iter().zip(events).all(|(committed, event)| {
            committed.event_id == event.event_id && committed.revision == event.revision
        })
}

fn compute_batch_digest(batch: &JournalBatch) -> Digest {
    compute_digest_fields(
        batch.thread_id,
        batch.batch_id,
        batch.command_id,
        batch.command_digest,
        batch.expected_revision,
        batch.recorded_at_unix_ms,
        &batch.events,
    )
}

fn compute_stored_batch_digest(batch: &StoredBatch) -> Digest {
    compute_digest_fields(
        batch.thread_id,
        batch.batch_id,
        batch.command_id,
        batch.command_digest,
        batch.expected_revision,
        batch.recorded_at_unix_ms,
        &batch.events,
    )
}

#[allow(clippy::too_many_arguments)]
fn compute_digest_fields(
    thread_id: ThreadId,
    batch_id: BatchId,
    command_id: CommandId,
    command_digest: Digest,
    expected_revision: ThreadRevision,
    recorded_at_unix_ms: i64,
    events: &[ThreadEventEnvelope],
) -> Digest {
    let mut hasher = CanonicalHasher::domain(BATCH_DIGEST_DOMAIN);
    thread_id.encode_canonical(&mut hasher);
    batch_id.encode_canonical(&mut hasher);
    command_id.encode_canonical(&mut hasher);
    command_digest.encode_canonical(&mut hasher);
    expected_revision.encode_canonical(&mut hasher);
    recorded_at_unix_ms.encode_canonical(&mut hasher);
    hasher.u64(events.len() as u64);
    for event in events {
        event.record_digest.encode_canonical(&mut hasher);
    }
    hasher.finish()
}

fn encode_frame_header(header: &FrameHeader) -> [u8; FRAME_HEADER_LEN] {
    let mut bytes = Vec::with_capacity(FRAME_HEADER_LEN);
    bytes.extend_from_slice(FRAME_MAGIC);
    push_u16(&mut bytes, FORMAT_VERSION);
    bytes.push(JSON_CODEC);
    bytes.push(0);
    push_u32(&mut bytes, FRAME_HEADER_LEN as u32);
    push_u64(&mut bytes, header.payload_len);
    push_u64(&mut bytes, header.start_revision.get());
    push_u64(&mut bytes, header.end_revision.get());
    push_u32(&mut bytes, header.event_count);
    push_u32(&mut bytes, 0);
    bytes.extend_from_slice(&header.batch_id);
    bytes.extend_from_slice(&header.command_id);
    bytes.extend_from_slice(header.command_digest.as_bytes());
    bytes.extend_from_slice(header.previous_record_digest.as_bytes());
    bytes.extend_from_slice(header.last_record_digest.as_bytes());
    push_u32(&mut bytes, header.payload_crc32c);
    let crc = crc32c::crc32c(&bytes);
    push_u32(&mut bytes, crc);
    let mut encoded = [0; FRAME_HEADER_LEN];
    encoded.copy_from_slice(&bytes);
    encoded
}

fn encode_trailer(frame_len: u64, batch_digest: Digest) -> [u8; FRAME_TRAILER_LEN] {
    let mut bytes = Vec::with_capacity(FRAME_TRAILER_LEN);
    bytes.extend_from_slice(TRAILER_MAGIC);
    push_u64(&mut bytes, frame_len);
    bytes.extend_from_slice(batch_digest.as_bytes());
    let crc = crc32c::crc32c(&bytes);
    push_u32(&mut bytes, crc);
    let mut trailer = [0; FRAME_TRAILER_LEN];
    trailer.copy_from_slice(&bytes);
    trailer
}

fn digest_at(bytes: &[u8], offset: usize) -> Digest {
    let mut digest = [0; 32];
    digest.copy_from_slice(&bytes[offset..offset + 32]);
    Digest::from_bytes(digest)
}

fn u16_at(bytes: &[u8], offset: usize) -> u16 {
    let mut value = [0; 2];
    value.copy_from_slice(&bytes[offset..offset + 2]);
    u16::from_le_bytes(value)
}

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    let mut value = [0; 4];
    value.copy_from_slice(&bytes[offset..offset + 4]);
    u32::from_le_bytes(value)
}

fn u64_at(bytes: &[u8], offset: usize) -> u64 {
    let mut value = [0; 8];
    value.copy_from_slice(&bytes[offset..offset + 8]);
    u64::from_le_bytes(value)
}

fn push_u16(bytes: &mut Vec<u8>, value: u16) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn push_u32(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn push_u64(bytes: &mut Vec<u8>, value: u64) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;
    use praxis_thread_store_contracts::EventId;
    use praxis_thread_store_contracts::NewThreadEvent;
    use praxis_thread_store_contracts::ThreadActor;
    use praxis_thread_store_contracts::ThreadCommand;
    use praxis_thread_store_contracts::ThreadCommandHeader;
    use praxis_thread_store_contracts::ThreadEventBody;
    use uuid::Uuid;

    fn batch() -> (ThreadId, JournalBatch) {
        let thread_id = ThreadId::from_uuid(Uuid::from_u128(1));
        let actor = ThreadActor::User;
        let correlation_id = None;
        let command = ThreadCommand::SetName {
            name: Some("legacy".to_string()),
        };
        let command = ThreadCommandHeader::new(
            CommandId::from_uuid(Uuid::from_u128(2)),
            thread_id,
            ThreadRevision::ZERO,
            &actor,
            &correlation_id,
            &command,
        );
        let batch_id = BatchId::from_uuid(Uuid::from_u128(3));
        let event = ThreadEventEnvelope::new(NewThreadEvent {
            thread_id,
            revision: ThreadRevision::new(1),
            event_id: EventId::from_uuid(Uuid::from_u128(4)),
            batch_id,
            sequence: 0,
            recorded_at_unix_ms: 1,
            actor: ThreadActor::User,
            correlation_id: None,
            causation_id: None,
            body: ThreadEventBody::ThreadNameSet {
                name: Some("legacy".to_string()),
            },
            previous_record_digest: Digest::ZERO,
        });
        (
            thread_id,
            JournalBatch::new(command, batch_id, 1, vec![event]),
        )
    }

    #[test]
    fn legacy_receipt_payload_remains_readable() {
        let (thread_id, batch) = batch();
        let encoded =
            encode_batch(batch, JournalDurability::Durable, u64::MAX).expect("encode batch");
        let expected = encoded
            .stored
            .clone()
            .into_receipt()
            .expect("derive receipt");
        let mut legacy = encoded.stored;
        legacy.schema_version = LEGACY_STORED_BATCH_SCHEMA_VERSION;
        legacy.legacy_receipt = Some(expected.clone());
        legacy.durability = None;
        legacy.receipt_schema_version = None;
        let payload = serde_json::to_vec(&legacy).expect("encode legacy payload");
        let decoded: StoredBatch = serde_json::from_slice(&payload).expect("decode legacy payload");

        validate_stored_batch(&encoded.header, &decoded, thread_id, ThreadHead::EMPTY)
            .expect("validate legacy payload");
        assert_eq!(
            decoded.into_receipt().expect("read legacy receipt"),
            expected
        );
    }

    #[test]
    fn payload_limit_stops_serialization_at_the_boundary() {
        let (_, batch) = batch();
        let error = encode_batch(batch, JournalDurability::Durable, 1)
            .expect_err("reject oversized payload");
        assert!(error.contains("frame payload exceeds configured maximum"));
    }

    #[test]
    fn single_event_payload_keeps_array_schema_without_vec_storage() {
        let (_, batch) = batch();
        let encoded =
            encode_batch(batch, JournalDurability::Durable, u64::MAX).expect("encode batch");
        let payload = serde_json::to_value(&encoded.stored).expect("serialize stored batch");
        assert_eq!(
            payload["events"].as_array().map(Vec::len),
            Some(1),
            "the durable JSON schema remains an event array"
        );
        let decoded: StoredBatch = serde_json::from_value(payload).expect("decode stored batch");
        assert!(matches!(decoded.events, StoredEvents::One(_)));
    }

    #[test]
    fn stored_batch_exposes_only_consuming_event_projection() {
        let source = include_str!("format.rs");
        assert!(!source.contains("fn events(&self)"));
        assert!(source.contains("fn into_events(self)"));
    }
}
