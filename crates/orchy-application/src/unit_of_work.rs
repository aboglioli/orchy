use std::future::Future;

use orchy_core::{DomainError, UnitOfWork};

use crate::error::{ApplicationError, ApplicationResult};

/// Runs past this many times only under a storm of writers to the same entities.
const ATTEMPTS: u32 = 16;

/// Runs a use case as one unit: if any step fails, none of its writes land. When something it
/// only read changed before it landed, it runs again from scratch, so what it decides always
/// rests on what is there when it lands; another writer having landed first is what made the
/// first run stale, so running again makes progress.
pub(crate) async fn atomically<T, F>(
    unit: &dyn UnitOfWork,
    work: impl Fn() -> F,
) -> ApplicationResult<T>
where
    T: Send,
    F: Future<Output = ApplicationResult<T>> + Send,
{
    let mut attempt = 1;
    loop {
        match once(unit, work()).await {
            Err(ApplicationError::Domain(DomainError::Contended(_))) if attempt < ATTEMPTS => {
                attempt += 1;
            }
            other => return other,
        }
    }
}

async fn once<T: Send>(
    unit: &dyn UnitOfWork,
    work: impl Future<Output = ApplicationResult<T>> + Send,
) -> ApplicationResult<T> {
    let mut output = None;
    unit.run(Box::pin(async {
        let value = work.await.map_err(|ApplicationError::Domain(e)| e)?;
        output = Some(value);
        Ok(())
    }))
    .await?;
    Ok(output.expect("a unit of work that succeeded ran its work to the end"))
}
