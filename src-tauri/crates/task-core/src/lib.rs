//! Experimental task authority. No window, SQL, executor or filesystem dependency.
//! Mutations are persisted before observation. Publication is a two-phase journal;
//! an unresolved publication blocks cancellation until startup reconciliation.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;
pub const MAX_RETAINED_TASKS: usize = 1024;
pub const MAX_SNAPSHOT_BYTES: usize = 65536;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    Queued,
    Running,
    Paused,
    Succeeded,
    Failed,
    Cancelled,
    Interrupted,
}
impl Status {
    pub fn terminal(&self) -> bool {
        !matches!(self, Self::Queued | Self::Running | Self::Paused)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub id: String,
    pub owner: String,
    pub executor: String,
    pub resource: String,
    pub sequence: u64,
    pub status: Status,
    pub cancel_requested: bool,
    pub progress: u8,
    pub checkpoint: Option<String>,
    pub publication: Option<String>,
    pub result_refs: Vec<String>,
    pub error: Option<String>,
}
#[derive(Clone, Debug)]
pub enum Action {
    Start,
    Progress(u8),
    RequestCancel,
    ConfirmStopped,
    Succeed(Vec<String>),
    Fail(String),
    Checkpoint(String),
    /// Executor confirms it has stopped at the durable checkpoint before this action.
    ConfirmPaused(String),
    Resume,
    /// Startup coordinator has verified checkpoint identity, version and files.
    RecoverCheckpoint(String),
    PreparePublication(String),
    Published {
        journal: String,
        refs: Vec<String>,
    },
    /// A batch executor published one output and continues; retain its references.
    PublicationCommitted {
        journal: String,
        refs: Vec<String>,
    },
    PublicationAborted(String),
    RecoverPartialPublication {
        journal: String,
        refs: Vec<String>,
    },
    Recover {
        published: Option<Vec<String>>,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Invalid,
    NotFound,
    Denied,
    Stale,
    Terminal,
    CancelPending,
    PublicationPending,
    RecoveryRequired,
    Capacity,
    Storage(String),
}

fn label(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':'))
}
fn reference(value: &str) -> bool {
    !value.is_empty() && value.len() <= 2048 && !value.contains('\0')
}
fn references(refs: &[String]) -> bool {
    refs.len() <= 32 && refs.iter().all(|r| reference(r))
}
impl Snapshot {
    pub fn queued(
        id: String,
        owner: String,
        executor: String,
        resource: String,
    ) -> Result<Self, Error> {
        if ![&id, &owner, &executor, &resource]
            .into_iter()
            .all(|v| label(v))
        {
            return Err(Error::Invalid);
        }
        Ok(Self {
            id,
            owner,
            executor,
            resource,
            sequence: 1,
            status: Status::Queued,
            cancel_requested: false,
            progress: 0,
            checkpoint: None,
            publication: None,
            result_refs: vec![],
            error: None,
        })
    }
    pub fn validate(&self) -> Result<(), Error> {
        if self.sequence == 0
            || self.sequence > 9_007_199_254_740_991
            || self.progress > 100
            || ![&self.id, &self.owner, &self.executor, &self.resource]
                .into_iter()
                .all(|v| label(v))
            || !references(&self.result_refs)
            || self.checkpoint.as_ref().is_some_and(|v| !reference(v))
            || self.publication.as_ref().is_some_and(|v| !reference(v))
            || self
                .error
                .as_ref()
                .is_some_and(|v| !reference(v) || v.len() > 1024)
            || (self.publication.is_some()
                && (self.cancel_requested
                    || matches!(
                        self.status,
                        Status::Queued | Status::Paused | Status::Cancelled | Status::Failed
                    )))
            || (self.status == Status::Paused && self.checkpoint.is_none())
            || (self.status == Status::Succeeded && self.progress != 100)
            || (self.status == Status::Cancelled && !self.cancel_requested)
        {
            return Err(Error::Invalid);
        }
        Ok(())
    }
    pub fn transition(&self, action: Action) -> Result<Self, Error> {
        if self.status.terminal() {
            return Err(Error::Terminal);
        }
        self.validate()?;
        let mut next = self.clone();
        match action {
            Action::Start if self.status == Status::Queued && !self.cancel_requested => {
                next.status = Status::Running
            }
            Action::Progress(value) if self.status == Status::Running && value <= 100 => {
                next.progress = self.progress.max(value)
            }
            Action::RequestCancel => {
                if self.publication.is_some() {
                    return Err(Error::PublicationPending);
                }
                if self.cancel_requested {
                    return Err(Error::CancelPending);
                }
                next.cancel_requested = true;
            }
            Action::ConfirmStopped if self.cancel_requested && self.publication.is_none() => {
                next.status = Status::Cancelled
            }
            Action::Succeed(refs) if self.status == Status::Running && references(&refs) => {
                if self.cancel_requested {
                    return Err(Error::CancelPending);
                }
                if self.publication.is_some() {
                    return Err(Error::PublicationPending);
                }
                next.status = Status::Succeeded;
                next.progress = 100;
                for reference in refs {
                    if !next.result_refs.contains(&reference) {
                        next.result_refs.push(reference);
                    }
                }
                if !references(&next.result_refs) {
                    return Err(Error::Invalid);
                }
            }
            Action::Fail(message) if reference(&message) && message.len() <= 1024 => {
                if self.publication.is_some() {
                    return Err(Error::PublicationPending);
                }
                if self.cancel_requested {
                    return Err(Error::CancelPending);
                }
                next.status = Status::Failed;
                next.error = Some(message);
            }
            Action::Checkpoint(checkpoint)
                if self.status == Status::Running && reference(&checkpoint) =>
            {
                next.checkpoint = Some(checkpoint)
            }
            Action::ConfirmPaused(checkpoint)
                if self.status == Status::Running && reference(&checkpoint) =>
            {
                if self.cancel_requested {
                    return Err(Error::CancelPending);
                }
                if self.publication.is_some() {
                    return Err(Error::PublicationPending);
                }
                next.checkpoint = Some(checkpoint);
                next.status = Status::Paused;
            }
            Action::Resume if self.status == Status::Paused && !self.cancel_requested => {
                next.status = Status::Queued;
            }
            Action::RecoverCheckpoint(checkpoint)
                if self.checkpoint.as_ref() == Some(&checkpoint)
                    && self.publication.is_none()
                    && !self.cancel_requested =>
            {
                next.status = Status::Paused;
            }
            Action::PreparePublication(journal)
                if self.status == Status::Running && reference(&journal) =>
            {
                if self.cancel_requested {
                    return Err(Error::CancelPending);
                }
                if self.publication.is_some() {
                    return Err(Error::PublicationPending);
                }
                next.publication = Some(journal);
            }
            Action::Published { journal, refs }
                if references(&refs) && self.publication.as_ref() == Some(&journal) =>
            {
                next.status = Status::Succeeded;
                next.progress = 100;
                for reference in refs {
                    if !next.result_refs.contains(&reference) {
                        next.result_refs.push(reference);
                    }
                }
                if !references(&next.result_refs) {
                    return Err(Error::Invalid);
                }
            }
            Action::PublicationCommitted { journal, refs }
                if references(&refs) && self.publication.as_ref() == Some(&journal) =>
            {
                for reference in refs {
                    if !next.result_refs.contains(&reference) {
                        next.result_refs.push(reference);
                    }
                }
                if !references(&next.result_refs) {
                    return Err(Error::Invalid);
                }
                next.publication = None;
            }
            Action::PublicationAborted(journal) if self.publication.as_ref() == Some(&journal) => {
                next.publication = None
            }
            Action::RecoverPartialPublication { journal, refs }
                if references(&refs) && self.publication.as_ref() == Some(&journal) =>
            {
                for reference in refs {
                    if !next.result_refs.contains(&reference) {
                        next.result_refs.push(reference);
                    }
                }
                if !references(&next.result_refs) {
                    return Err(Error::Invalid);
                }
                next.publication = None;
                next.status = Status::Interrupted;
                next.error = Some(
                    "Executor stopped after verified partial publication; retry remaining work"
                        .into(),
                );
            }
            Action::Recover { published } => {
                if let Some(refs) = published {
                    if self.publication.is_none() || !references(&refs) {
                        return Err(Error::Invalid);
                    }
                    next.status = Status::Succeeded;
                    next.progress = 100;
                    for reference in refs {
                        if !next.result_refs.contains(&reference) {
                            next.result_refs.push(reference);
                        }
                    }
                    if !references(&next.result_refs) {
                        return Err(Error::Invalid);
                    }
                } else {
                    // Checkpoints are hints, not proof of executor resumability.
                    next.status = Status::Interrupted;
                    next.error = Some(
                        "Executor stopped; validate checkpoint or output journal before retry"
                            .into(),
                    );
                }
            }
            _ => return Err(Error::Invalid),
        }
        next.sequence = self
            .sequence
            .checked_add(1)
            .filter(|n| *n <= 9_007_199_254_740_991)
            .ok_or(Error::Invalid)?;
        Ok(next)
    }
}

/// CAS must atomically compare the durable revision and replace the record.
/// None means create-only. Persistence failures must leave the old record intact.
pub trait Repository: Send + Sync {
    fn list(&self) -> Result<Vec<Snapshot>, Error>;
    fn save(&self, expected: Option<u64>, next: &Snapshot) -> Result<(), Error>;
    /// Atomically preserve a terminal snapshot in history and remove it from the
    /// bounded hot set. Repositories without durable history fail closed.
    fn archive(&self, _snapshot: &Snapshot) -> Result<(), Error> {
        Err(Error::Capacity)
    }
}
/// Delivery can be missed: consumers reconnect with list(), merging by sequence.
pub trait Observer: Send + Sync {
    fn persisted(&self, snapshot: &Snapshot);
}
#[derive(Clone, Debug)]
pub struct Limits {
    pub queued: usize,
    pub retained: usize,
    pub resources: BTreeMap<String, usize>,
}
impl Limits {
    fn validate(&self) -> Result<(), Error> {
        if self.retained > MAX_RETAINED_TASKS
            || self.resources.len() > 64
            || self.queued == 0
            || self.retained < self.queued
            || self.resources.is_empty()
            || self.resources.iter().any(|(key, n)| !label(key) || *n == 0)
        {
            return Err(Error::Invalid);
        }
        Ok(())
    }
}
pub struct Runtime<R, O> {
    repository: R,
    observer: O,
    limits: Limits,
    records: Mutex<BTreeMap<String, Snapshot>>,
    pending_recovery: Mutex<BTreeSet<String>>,
}
impl<R: Repository, O: Observer> Runtime<R, O> {
    /// Startup does not pretend to resume live work. Call recover with verified
    /// output journals before starting any new executor.
    pub fn open(repository: R, observer: O, limits: Limits) -> Result<Self, Error> {
        limits.validate()?;
        let records = repository.list()?;
        if records.len() > limits.retained {
            return Err(Error::Capacity);
        }
        let mut by_id = BTreeMap::new();
        for record in records {
            record.validate()?;
            if by_id.insert(record.id.clone(), record).is_some() {
                return Err(Error::Invalid);
            }
        }
        Ok(Self {
            repository,
            observer,
            limits,
            pending_recovery: Mutex::new(
                by_id
                    .values()
                    .filter(|r| !r.status.terminal())
                    .map(|r| r.id.clone())
                    .collect(),
            ),
            records: Mutex::new(by_id),
        })
    }
    /// Composition-root enumeration for restart reconciliation; never a plugin RPC.
    pub fn list_all(&self) -> Result<Vec<Snapshot>, Error> {
        Ok(self
            .records
            .lock()
            .map_err(|_| Error::Invalid)?
            .values()
            .cloned()
            .collect())
    }
    pub fn list(&self, owner: &str) -> Result<Vec<Snapshot>, Error> {
        Ok(self
            .records
            .lock()
            .map_err(|_| Error::Invalid)?
            .values()
            .filter(|r| r.owner == owner)
            .cloned()
            .collect())
    }
    pub fn submit(&self, next: Snapshot) -> Result<Snapshot, Error> {
        // Only a host-created queued record enters the runtime.
        if next
            != Snapshot::queued(
                next.id.clone(),
                next.owner.clone(),
                next.executor.clone(),
                next.resource.clone(),
            )?
        {
            return Err(Error::Invalid);
        }
        let mut records = self.records.lock().map_err(|_| Error::Invalid)?;
        if !self
            .pending_recovery
            .lock()
            .map_err(|_| Error::Invalid)?
            .is_empty()
        {
            return Err(Error::RecoveryRequired);
        }
        if records.contains_key(&next.id) {
            return Err(Error::Stale);
        }
        if !self.limits.resources.contains_key(&next.resource) {
            return Err(Error::Denied);
        }
        if records
            .values()
            .filter(|r| r.status == Status::Queued)
            .count()
            >= self.limits.queued
        {
            return Err(Error::Capacity);
        }
        if records.len() >= self.limits.retained {
            let archived = records
                .values()
                .find(|r| r.status.terminal())
                .cloned()
                .ok_or(Error::Capacity)?;
            self.repository.archive(&archived)?;
            records.remove(&archived.id);
        }
        self.repository.save(None, &next)?;
        records.insert(next.id.clone(), next.clone());
        drop(records);
        self.observer.persisted(&next);
        Ok(next)
    }
    pub fn apply(
        &self,
        owner: &str,
        id: &str,
        expected: u64,
        action: Action,
    ) -> Result<Snapshot, Error> {
        let mut records = self.records.lock().map_err(|_| Error::Invalid)?;
        let old = records.get(id).ok_or(Error::NotFound)?;
        if old.owner != owner {
            return Err(Error::Denied);
        }
        if old.sequence != expected {
            return Err(Error::Stale);
        }
        let recovering = matches!(
            action,
            Action::Recover { .. }
                | Action::RecoverCheckpoint(_)
                | Action::RecoverPartialPublication { .. }
        );
        let mut recovery = self.pending_recovery.lock().map_err(|_| Error::Invalid)?;
        if recovering {
            if !recovery.contains(id) {
                return Err(Error::Invalid);
            }
        } else if !recovery.is_empty() {
            return Err(Error::RecoveryRequired);
        }
        if matches!(action, Action::Resume)
            && records
                .values()
                .filter(|r| r.status == Status::Queued)
                .count()
                >= self.limits.queued
        {
            return Err(Error::Capacity);
        }
        if matches!(action, Action::Start) {
            // Recovered startup is a prerequisite: persisted running records
            // cannot consume phantom leases forever.
            let capacity = self
                .limits
                .resources
                .get(&old.resource)
                .ok_or(Error::Denied)?;
            if records
                .values()
                .filter(|r| r.resource == old.resource && r.status == Status::Running)
                .count()
                >= *capacity
            {
                return Err(Error::Capacity);
            }
        }
        let next = old.transition(action)?;
        self.repository.save(Some(expected), &next)?;
        records.insert(id.into(), next.clone());
        if recovering {
            recovery.remove(id);
        }
        drop(recovery);
        drop(records);
        self.observer.persisted(&next);
        Ok(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    #[derive(Clone, Default)]
    struct Memory {
        rows: Arc<Mutex<BTreeMap<String, Snapshot>>>,
        fail: Arc<AtomicBool>,
    }
    impl Repository for Memory {
        fn list(&self) -> Result<Vec<Snapshot>, Error> {
            Ok(self.rows.lock().unwrap().values().cloned().collect())
        }
        fn save(&self, expected: Option<u64>, next: &Snapshot) -> Result<(), Error> {
            if self.fail.load(Ordering::SeqCst) {
                return Err(Error::Storage("injected".into()));
            }
            let mut rows = self.rows.lock().unwrap();
            if rows.get(&next.id).map(|s| s.sequence) != expected {
                return Err(Error::Stale);
            }
            rows.insert(next.id.clone(), next.clone());
            Ok(())
        }
    }
    #[derive(Clone, Default)]
    struct Events {
        seen: Arc<Mutex<Vec<Snapshot>>>,
        repository: Memory,
    }
    impl Observer for Events {
        fn persisted(&self, snapshot: &Snapshot) {
            assert_eq!(
                self.repository.rows.lock().unwrap().get(&snapshot.id),
                Some(snapshot)
            );
            self.seen.lock().unwrap().push(snapshot.clone());
        }
    }
    fn limits() -> Limits {
        Limits {
            queued: 2,
            retained: 8,
            resources: BTreeMap::from([("cpu".into(), 1)]),
        }
    }
    fn queued(id: &str) -> Snapshot {
        Snapshot::queued(id.into(), "diary".into(), "document".into(), "cpu".into()).unwrap()
    }
    fn runtime() -> (Runtime<Memory, Events>, Memory, Events) {
        let repo = Memory::default();
        let observer = Events {
            repository: repo.clone(),
            ..Default::default()
        };
        (
            Runtime::open(repo.clone(), observer.clone(), limits()).unwrap(),
            repo,
            observer,
        )
    }
    #[test]
    fn persistence_failure_never_emits_or_mutates_in_memory_truth() {
        let (rt, repo, events) = runtime();
        rt.submit(queued("a")).unwrap();
        repo.fail.store(true, Ordering::SeqCst);
        assert_eq!(
            rt.apply("diary", "a", 1, Action::Start),
            Err(Error::Storage("injected".into()))
        );
        assert_eq!(rt.list("diary").unwrap()[0].sequence, 1);
        assert_eq!(events.seen.lock().unwrap().len(), 1);
        repo.fail.store(false, Ordering::SeqCst);
        assert_eq!(
            rt.apply("diary", "a", 1, Action::Start).unwrap().sequence,
            2
        );
    }
    #[test]
    fn bounded_queue_resource_lease_and_owner_cas() {
        let (rt, _, _) = runtime();
        rt.submit(queued("a")).unwrap();
        rt.submit(queued("b")).unwrap();
        assert_eq!(rt.submit(queued("c")), Err(Error::Capacity));
        assert_eq!(rt.apply("other", "a", 1, Action::Start), Err(Error::Denied));
        rt.apply("diary", "a", 1, Action::Start).unwrap();
        assert_eq!(
            rt.apply("diary", "b", 1, Action::Start),
            Err(Error::Capacity)
        );
        assert_eq!(
            rt.apply("diary", "a", 1, Action::Fail("late".into())),
            Err(Error::Stale)
        );
        rt.apply("diary", "a", 2, Action::Succeed(vec![])).unwrap();
        rt.apply("diary", "b", 1, Action::Start).unwrap();
        assert_eq!(
            rt.apply("diary", "a", 3, Action::Progress(1)),
            Err(Error::Terminal)
        );
    }
    #[test]
    fn cancel_waits_for_executor_stop_and_cannot_publish() {
        let (rt, _, _) = runtime();
        rt.submit(queued("a")).unwrap();
        rt.apply("diary", "a", 1, Action::Start).unwrap();
        let pending = rt.apply("diary", "a", 2, Action::RequestCancel).unwrap();
        assert_eq!(pending.status, Status::Running);
        assert_eq!(
            rt.apply("diary", "a", 3, Action::Succeed(vec![])),
            Err(Error::CancelPending)
        );
        assert_eq!(
            rt.apply(
                "diary",
                "a",
                3,
                Action::PreparePublication("output:1".into())
            ),
            Err(Error::CancelPending)
        );
        assert_eq!(
            rt.apply("diary", "a", 3, Action::ConfirmStopped)
                .unwrap()
                .status,
            Status::Cancelled
        );
    }
    #[test]
    fn durable_publication_reservation_wins_late_cancel_and_restart_reconciles() {
        let (rt, repo, events) = runtime();
        rt.submit(queued("a")).unwrap();
        rt.apply("diary", "a", 1, Action::Start).unwrap();
        rt.apply(
            "diary",
            "a",
            2,
            Action::PreparePublication("output:1".into()),
        )
        .unwrap();
        assert_eq!(
            rt.apply("diary", "a", 3, Action::RequestCancel),
            Err(Error::PublicationPending)
        );
        // Simulates a successful file publish followed by failed terminal DB write.
        repo.fail.store(true, Ordering::SeqCst);
        assert!(matches!(
            rt.apply(
                "diary",
                "a",
                3,
                Action::Published {
                    journal: "output:1".into(),
                    refs: vec!["file:1".into()]
                }
            ),
            Err(Error::Storage(_))
        ));
        drop(rt);
        repo.fail.store(false, Ordering::SeqCst);
        let restarted = Runtime::open(repo, events, limits()).unwrap();
        assert_eq!(
            restarted.submit(queued("blocked")),
            Err(Error::RecoveryRequired)
        );
        let recovered = restarted
            .apply(
                "diary",
                "a",
                3,
                Action::Recover {
                    published: Some(vec!["file:1".into()]),
                },
            )
            .unwrap();
        assert_eq!(recovered.status, Status::Succeeded);
        assert_eq!(recovered.result_refs, vec!["file:1"]);
        assert_eq!(
            restarted.apply("diary", "a", 4, Action::RequestCancel),
            Err(Error::Terminal)
        );
    }
    #[test]
    fn restart_does_not_claim_checkpoint_is_running_or_resumable() {
        let (rt, repo, events) = runtime();
        rt.submit(queued("a")).unwrap();
        rt.apply("diary", "a", 1, Action::Start).unwrap();
        rt.apply("diary", "a", 2, Action::Checkpoint("checkpoint:1".into()))
            .unwrap();
        drop(rt);
        let restarted = Runtime::open(repo, events, limits()).unwrap();
        assert_eq!(
            restarted.submit(queued("blocked")),
            Err(Error::RecoveryRequired)
        );
        let recovered = restarted
            .apply("diary", "a", 3, Action::Recover { published: None })
            .unwrap();
        assert_eq!(recovered.status, Status::Interrupted);
        assert_eq!(recovered.checkpoint.as_deref(), Some("checkpoint:1"));
        assert_eq!(
            restarted.apply("diary", "a", 4, Action::Start),
            Err(Error::Terminal)
        );
    }
    #[test]
    fn cancellation_and_publication_race_has_exactly_one_winner() {
        let (rt, _, _) = runtime();
        let rt = Arc::new(rt);
        rt.submit(queued("a")).unwrap();
        rt.apply("diary", "a", 1, Action::Start).unwrap();
        let barrier = Arc::new(std::sync::Barrier::new(3));
        let cancel_rt = rt.clone();
        let cancel_barrier = barrier.clone();
        let cancel = std::thread::spawn(move || {
            cancel_barrier.wait();
            cancel_rt.apply("diary", "a", 2, Action::RequestCancel)
        });
        let publish_rt = rt.clone();
        let publish_barrier = barrier.clone();
        let publish = std::thread::spawn(move || {
            publish_barrier.wait();
            publish_rt.apply(
                "diary",
                "a",
                2,
                Action::PreparePublication("output:1".into()),
            )
        });
        barrier.wait();
        let c = cancel.join().unwrap();
        let p = publish.join().unwrap();
        assert_ne!(c.is_ok(), p.is_ok());
        assert!(matches!(c, Err(Error::Stale)) || matches!(p, Err(Error::Stale)));
    }
    #[test]
    fn logically_corrupt_durable_record_is_rejected_without_overwrite() {
        let repo = Memory::default();
        let mut invalid = queued("a");
        invalid.status = Status::Cancelled;
        repo.rows
            .lock()
            .unwrap()
            .insert("a".into(), invalid.clone());
        let observer = Events {
            repository: repo.clone(),
            ..Default::default()
        };
        assert!(matches!(
            Runtime::open(repo.clone(), observer, limits()),
            Err(Error::Invalid)
        ));
        assert_eq!(repo.rows.lock().unwrap().get("a"), Some(&invalid));
        invalid.status = Status::Queued;
        invalid.publication = Some("output:1".into());
        assert_eq!(invalid.validate(), Err(Error::Invalid));
    }
    #[test]
    fn confirmed_pause_releases_lease_and_resume_reacquires_it_through_queue() {
        let (rt, _, _) = runtime();
        rt.submit(queued("a")).unwrap();
        rt.apply("diary", "a", 1, Action::Start).unwrap();
        let paused = rt
            .apply(
                "diary",
                "a",
                2,
                Action::ConfirmPaused("checkpoint/a".into()),
            )
            .unwrap();
        assert_eq!(paused.status, Status::Paused);
        rt.submit(queued("b")).unwrap();
        rt.apply("diary", "b", 1, Action::Start).unwrap();
        let resumed = rt.apply("diary", "a", 3, Action::Resume).unwrap();
        assert_eq!(resumed.status, Status::Queued);
        assert_eq!(
            rt.apply("diary", "a", 4, Action::Start),
            Err(Error::Capacity)
        );
        rt.apply("diary", "b", 2, Action::Succeed(vec![])).unwrap();
        let active = rt.apply("diary", "a", 4, Action::Start).unwrap();
        assert_eq!(active.status, Status::Running);
        assert_eq!(active.checkpoint.as_deref(), Some("checkpoint/a"));
        let cancel = rt.apply("diary", "a", 5, Action::RequestCancel).unwrap();
        assert_eq!(
            rt.apply(
                "diary",
                "a",
                cancel.sequence,
                Action::ConfirmPaused("checkpoint/a".into())
            ),
            Err(Error::CancelPending)
        );
    }
    #[test]
    fn checkpoint_recovery_requires_matching_verified_reference_and_never_starts_worker_implicitly()
    {
        let (rt, repo, events) = runtime();
        rt.submit(queued("a")).unwrap();
        rt.apply("diary", "a", 1, Action::Start).unwrap();
        rt.apply("diary", "a", 2, Action::Checkpoint("checkpoint/a".into()))
            .unwrap();
        drop(rt);
        let recovered = Runtime::open(repo, events, limits()).unwrap();
        assert_eq!(
            recovered.apply(
                "diary",
                "a",
                3,
                Action::RecoverCheckpoint("checkpoint/other".into())
            ),
            Err(Error::Invalid)
        );
        let paused = recovered
            .apply(
                "diary",
                "a",
                3,
                Action::RecoverCheckpoint("checkpoint/a".into()),
            )
            .unwrap();
        assert_eq!(paused.status, Status::Paused);
        assert_eq!(recovered.list("diary").unwrap()[0].status, Status::Paused);
        assert_eq!(
            recovered.apply("diary", "a", 4, Action::Start),
            Err(Error::Invalid)
        );
        let queued = recovered.apply("diary", "a", 4, Action::Resume).unwrap();
        assert_eq!(queued.status, Status::Queued);
    }
}
