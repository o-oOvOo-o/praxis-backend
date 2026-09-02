use std::fs::OpenOptions;
use std::io;
use std::io::Seek;
use std::io::SeekFrom;
use std::io::Write;
use std::path::Path;

#[cfg(unix)]
use std::fs::File;

pub(crate) enum AppendError {
    OffsetChanged { actual: u64 },
    BeforeWrite(io::Error),
    DuringWrite(io::Error),
}

pub(crate) fn append_at(
    path: &Path,
    expected_offset: u64,
    bytes: &[u8],
) -> Result<(), AppendError> {
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .map_err(AppendError::BeforeWrite)?;
    let actual = file
        .seek(SeekFrom::End(0))
        .map_err(AppendError::BeforeWrite)?;
    if actual != expected_offset {
        return Err(AppendError::OffsetChanged { actual });
    }
    #[cfg(test)]
    if let Some(Failure::PartialWrite(byte_count)) = take_failure(FailureKind::Write) {
        let byte_count = byte_count.min(bytes.len());
        file.write_all(&bytes[..byte_count])
            .map_err(AppendError::DuringWrite)?;
        return Err(AppendError::DuringWrite(injected_error("frame write")));
    }
    file.write_all(bytes).map_err(AppendError::DuringWrite)?;
    Ok(())
}

pub(crate) fn sync_file(path: &Path) -> io::Result<()> {
    #[cfg(test)]
    if take_failure(FailureKind::Sync).is_some() {
        return Err(injected_error("file sync"));
    }
    OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)?
        .sync_all()
}

pub(crate) fn truncate_and_sync(path: &Path, len: u64) -> io::Result<()> {
    let file = OpenOptions::new().read(true).write(true).open(path)?;
    file.set_len(len)?;
    file.sync_all()
}

pub(crate) fn sync_directory(path: &Path) -> io::Result<()> {
    sync_directory_impl(path)
}

#[cfg(unix)]
fn sync_directory_impl(path: &Path) -> io::Result<()> {
    File::open(path)?.sync_all()
}

#[cfg(not(unix))]
fn sync_directory_impl(_path: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
#[derive(Clone, Copy, Debug)]
pub(crate) enum Failure {
    PartialWrite(usize),
    FileSync,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FailureKind {
    Write,
    Sync,
}

#[cfg(test)]
impl Failure {
    const fn kind(self) -> FailureKind {
        match self {
            Self::PartialWrite(_) => FailureKind::Write,
            Self::FileSync => FailureKind::Sync,
        }
    }
}

#[cfg(test)]
thread_local! {
    static NEXT_FAILURE: std::cell::Cell<Option<Failure>> = const { std::cell::Cell::new(None) };
}

#[cfg(test)]
pub(crate) fn fail_next(failure: Failure) {
    NEXT_FAILURE.with(|slot| {
        assert!(
            slot.replace(Some(failure)).is_none(),
            "test failure already armed"
        );
    });
}

#[cfg(test)]
fn take_failure(kind: FailureKind) -> Option<Failure> {
    NEXT_FAILURE.with(|slot| {
        let failure = slot.get();
        if failure.is_some_and(|failure| failure.kind() == kind) {
            slot.set(None);
            failure
        } else {
            None
        }
    })
}

#[cfg(test)]
fn injected_error(operation: &str) -> io::Error {
    io::Error::other(format!("injected journal {operation} failure"))
}
