use crate::{conflict, invalid, state::Pending, transport::Transport, Cancellation, Plan, Progress,
    RelativePath, Result, State, Target};

fn marker() -> RelativePath { RelativePath::new(".chilpa-sync/identity.json").unwrap() }

pub fn inspect_target(transport: &mut dyn Transport, cancel: &Cancellation) -> Result<Option<Target>> {
    let Some(info) = transport.stat(&marker())? else { return Ok(None) };
    if info.directory || info.size > 4096 { return Err(invalid("damaged sync identity")); }
    struct Bounded(Vec<u8>);
    impl std::io::Write for Bounded {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            if self.0.len().saturating_add(b.len()) > 4096 { return Err(std::io::Error::other("sync identity exceeds 4 KiB")); }
            self.0.extend_from_slice(b); Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
    }
    let mut raw = Bounded(Vec::new());
    transport.read(&marker(), &mut raw, cancel)?;
    let target: Target = serde_json::from_slice(&raw.0).map_err(|_| invalid("damaged sync identity"))?;
    target.validate()?;
    Ok(Some(target))
}

pub(crate) fn check_target(transport: &mut dyn Transport, target: &Target, cancel: &Cancellation) -> Result<()> {
    if inspect_target(transport, cancel)?.as_ref().map(|t| &t.id) != Some(&target.id) {
        return Err(conflict("the selected storage changed or disconnected"));
    }
    Ok(())
}

/// I write identity once per selected root, so ports, drive letters and MTP handles are irrelevant.
pub fn pair(state: &mut State, transport: &mut dyn Transport, name: &str, cancel: &Cancellation) -> Result<Target> {
    cancel.check()?;
    let target = if let Some(target) = inspect_target(transport, cancel)? { target } else {
        if state.target()?.is_some() { return Err(conflict("the paired storage identity is missing")); }
        if name.trim().is_empty() || name.len() > 256 { return Err(invalid("a sync storage needs a name of 1 to 256 bytes")); }
        let target = Target { version:1, id:crate::token(), name:name.trim().into() };
        let bytes = serde_json::to_vec(&target).map_err(|e| invalid(e.to_string()))?;
        transport.create_dir_all(&RelativePath::new(".chilpa-sync")?)?;
        let stage = RelativePath::new(format!(".chilpa-sync/identity-{}.tmp", crate::token()))?;
        transport.write_new(&stage, &mut bytes.as_slice(), bytes.len() as u64, cancel, &mut |_| ())?;
        let expected = crate::model::fingerprint(&mut bytes.as_slice(), cancel)?;
        if transport.fingerprint(&stage, cancel)?.as_ref() != Some(&expected) { return Err(conflict("storage identity failed verification")); }
        transport.rename_new(&stage, &marker())?;
        target
    };
    state.bind(&target)?;
    Ok(target)
}

fn discard_stage(transport: &mut dyn Transport, pending: &Pending) -> Result<()> {
    if let Some(info) = transport.stat(&pending.stage)? {
        if info.directory { return Err(conflict("a directory occupies sync staging")); }
        transport.remove_file(&pending.stage)?;
    }
    Ok(())
}

/// I reconcile the journal against bytes on the device, including the crash window between
/// a successful rename and its local database commit. I never infer success from a lost reply.
pub fn recover(state: &mut State, transport: &mut dyn Transport, cancel: &Cancellation) -> Result<usize> {
    let target = state.target()?.ok_or_else(|| invalid("pair a storage before recovery"))?;
    check_target(transport, &target, cancel)?;
    let mut recovered = 0;
    for (position, pending) in state.pending()? {
        cancel.check()?;
        check_target(transport, &target, cancel)?;
        let operation = &pending.operation;
        let actual = transport.fingerprint(&operation.path, cancel)?;
        let backup = transport.fingerprint(&pending.backup, cancel)?;
        if backup.is_some() && backup != operation.previous { return Err(conflict(format!("sync backup changed at {}", pending.backup))); }
        let wanted = operation.desired.as_ref().map(|m| &m.fingerprint);
        let published = match wanted {
            Some(wanted) => actual.as_ref() == Some(wanted),
            // A completed removal can have lost its backup already, just before journal cleanup.
            None => actual.is_none(),
        };
        if published {
            state.published(operation)?;
            transport.published(&operation.path, cancel)?;
            if backup.is_some() { transport.remove_file(&pending.backup)?; }
            discard_stage(transport, &pending)?;
        } else if actual == operation.previous && backup.is_none() {
            discard_stage(transport, &pending)?;
        } else if actual.is_none() && backup.is_some() {
            transport.rename_new(&pending.backup, &operation.path)?;
            discard_stage(transport, &pending)?;
        } else {
            return Err(conflict(format!("external changes prevent sync recovery at {}", operation.path)));
        }
        state.finished(position)?;
        recovered += 1;
    }
    Ok(recovered)
}

/// `approved_plan` binds the confirmation to this exact preview. Removal needs a separate
/// explicit approval, and it remains limited to unchanged files recorded by this profile.
pub fn synchronize(state: &mut State, transport: &mut dyn Transport, plan: &Plan,
    approved_plan: &str, approve_removals: bool, cancel: &Cancellation, progress: &mut (dyn FnMut(Progress) + Send)) -> Result<()> {
    if approved_plan != plan.id() || (plan.deletion_count() > 0 && !approve_removals) { return Err(conflict("this sync preview has not been approved")); }
    if state.target()?.as_ref().map(|t| &t.id) != Some(&plan.target.id) || state.revision()? != plan.revision {
        return Err(conflict("the sync profile changed after this preview"));
    }
    check_target(transport, &plan.target, cancel)?;
    cancel.check()?;
    if plan.operations.is_empty() { return Ok(()); }
    let free = transport.available_bytes()?;
    if free < plan.required_bytes() { return Err(crate::Error::Space { required:plan.required_bytes(), available:free }); }
    for operation in &plan.operations {
        if transport.fingerprint(&operation.path, cancel)? != operation.previous {
            return Err(conflict(format!("device content changed after preview at {}", operation.path)));
        }
    }
    state.start(&plan.operations)?;
    let pending = state.pending()?;
    for (index, (position, pending)) in pending.iter().enumerate() {
        cancel.check()?;
        check_target(transport, &plan.target, cancel)?;
        let operation = &pending.operation;
        if let Some(wanted) = &operation.desired {
            let source = operation.source.as_ref().ok_or_else(|| invalid("sync operation has no source"))?;
            if let Some(parent) = pending.stage.parent() { transport.create_dir_all(&parent)?; }
            let mut input = source.reader()?;
            transport.write_new(&pending.stage, &mut input, wanted.fingerprint.size, cancel, &mut |bytes| {
                progress(Progress { completed_files:index, total_files:plan.actions().len(),
                    current_path:operation.path.to_string(), transferred_bytes:bytes });
            })?;
            if transport.fingerprint(&pending.stage, cancel)?.as_ref() != Some(&wanted.fingerprint) {
                return Err(conflict(format!("transferred bytes failed verification at {}", operation.path)));
            }
        }
        cancel.check()?;
        check_target(transport, &plan.target, cancel)?;
        if transport.fingerprint(&operation.path, cancel)? != operation.previous {
            return Err(conflict(format!("device content changed before publication at {}", operation.path)));
        }
        if operation.previous.is_some() {
            if let Some(parent) = pending.backup.parent() { transport.create_dir_all(&parent)?; }
            transport.rename_new(&operation.path, &pending.backup)?;
        }
        if operation.desired.is_some() {
            if let Some(parent) = operation.path.parent() { transport.create_dir_all(&parent)?; }
            transport.rename_new(&pending.stage, &operation.path)?;
        }
        if transport.fingerprint(&operation.path, cancel)? != operation.desired.as_ref().map(|m| m.fingerprint.clone()) {
            return Err(conflict(format!("published device content failed verification at {}", operation.path)));
        }
        state.published(operation)?;
        transport.published(&operation.path, cancel)?;
        if operation.previous.is_some() { transport.remove_file(&pending.backup)?; }
        state.finished(*position)?;
        progress(Progress { completed_files:index + 1, total_files:plan.actions().len(),
            current_path:operation.path.to_string(), transferred_bytes:operation.desired.as_ref().map_or(0, |m| m.fingerprint.size) });
    }
    Ok(())
}
