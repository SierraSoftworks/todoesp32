//! A small async retry helper.

use core::fmt::Debug;
use core::future::Future;

/// Run `f`, retrying up to `retries` additional times on error.
///
/// Returns the first `Ok` value, or the final `Err` once all attempts are
/// exhausted. Each failed attempt is logged at `warn` level.
pub async fn retry<F, Fut, T, E>(mut f: F, retries: usize) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
    E: Debug,
{
    let mut attempt = 0;
    loop {
        match f().await {
            Ok(v) => return Ok(v),
            Err(e) if attempt == retries => return Err(e),
            Err(e) => {
                log::warn!(
                    "Failed to execute operation (retry {} of {retries}): {e:?}",
                    attempt + 1
                );
                attempt += 1;
            }
        }
    }
}
