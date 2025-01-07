pub fn retry<F, T, E>(mut f: F, retries: usize) -> Result<T, E>
where
    F: FnMut() -> Result<T, E>,
    E: std::fmt::Debug,
{
    for i in 0..=retries {
        match f() {
            Ok(v) => return Ok(v),
            Err(e) if i == retries => return Err(e),
            Err(e) => {
                log::warn!(
                    "Failed to execute operation (retry {} of {retries}): {e:?}",
                    i + 1
                );
            }
        }
    }

    unreachable!()
}
