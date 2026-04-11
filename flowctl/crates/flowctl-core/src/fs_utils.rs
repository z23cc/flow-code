use std::fs;
use std::path::Path;

/// Atomically write bytes by writing to a sibling temp file and renaming.
pub(crate) fn atomic_write(path: &Path, content: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, content)?;
    fs::rename(&tmp, path)?;
    Ok(())
}
