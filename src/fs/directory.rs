use crate::driver::Op;

use std::{io, path::Path};

/// Removes an empty directory.
///
/// # Examples
///
/// ```no_run
/// use async_uring::fs::remove_dir;
///
/// fn main() -> Result<(), Box<dyn std::error::Error>> {
///     async_uring::start(async {
///         remove_dir("/some/dir").await?;
///         Ok::<(), std::io::Error>(())
///     })?;
///     Ok(())
/// }
/// ```
pub async fn remove_dir<P: AsRef<Path>>(path: P) -> io::Result<()> {
    let op = Op::unlink_dir(path.as_ref())?;
    let completion = op.await;
    completion.result?;

    Ok(())
}
