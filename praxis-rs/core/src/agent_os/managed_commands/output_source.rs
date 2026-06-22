use crate::agent_os::classification::summarize_output;
use crate::exec::ExecOutputSpool;

pub(in crate::agent_os) enum ManagedCommandOutputSource<'a> {
    Bytes(&'a [u8]),
    Spool {
        spool: ExecOutputSpool,
        fallback_raw_output: &'a [u8],
    },
}

impl ManagedCommandOutputSource<'_> {
    pub(in crate::agent_os) fn is_empty(&self) -> bool {
        match self {
            Self::Bytes(bytes) => bytes.is_empty(),
            Self::Spool { spool, .. } => spool.is_empty(),
        }
    }

    pub(in crate::agent_os) fn byte_len(&self) -> usize {
        match self {
            Self::Bytes(bytes) => bytes.len(),
            Self::Spool { spool, .. } => spool.total_bytes(),
        }
    }

    pub(in crate::agent_os) fn summary(&self) -> String {
        match self {
            Self::Bytes(bytes) => summarize_output(bytes),
            Self::Spool {
                fallback_raw_output,
                ..
            } => summarize_output(fallback_raw_output),
        }
    }
}
