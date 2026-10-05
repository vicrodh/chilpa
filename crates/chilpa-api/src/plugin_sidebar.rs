//! The listener's source order survives package updates and disabled sources.
use std::path::Path;

fn stored(root: &Path) -> Vec<String> {
    std::fs::read(root.join(".integration/sidebar-order.json")).ok().and_then(|data| serde_json::from_slice(&data).ok()).unwrap_or_default()
}

pub(crate) fn ordered(root: &Path, mut ids: Vec<String>) -> Vec<String> {
    let order = stored(root);
    ids.sort_by_key(|id| (order.iter().position(|v| v == id).unwrap_or(usize::MAX),id.clone()));
    ids.dedup();
    ids
}

pub(crate) fn place(root: &Path, id: &str, position: usize, active: Vec<String>) -> Result<(),String> {
    let mut shown = ordered(root,active);
    let Some(old) = shown.iter().position(|v| v == id) else { return Err("The source is not active.".into()) };
    if position >= shown.len() { return Err("Invalid sidebar position.".into()); }
    shown.remove(old); shown.insert(position,id.into());
    // Disabled entries retain their relative order after the visible sources.
    for saved in stored(root) { if !shown.contains(&saved) { shown.push(saved); } }
    let directory=root.join(".integration");
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let mut temporary=tempfile::NamedTempFile::new_in(&directory).map_err(|e| e.to_string())?;
    use std::io::Write;
    temporary.write_all(&serde_json::to_vec(&shown).map_err(|e|e.to_string())?).and_then(|_|temporary.as_file().sync_all()).map_err(|e|e.to_string())?;
    temporary.persist(directory.join("sidebar-order.json")).map_err(|e|e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn moving_a_source_persists_without_dropping_disabled_sources() {
        let root=tempfile::tempdir().unwrap();
        let ids=vec!["a".into(),"b".into(),"c".into()];
        place(root.path(),"c",0,ids.clone()).unwrap();
        assert_eq!(ordered(root.path(),ids.clone()),vec!["c","a","b"]);
        place(root.path(),"b",0,vec!["b".into(),"a".into()]).unwrap();
        assert_eq!(ordered(root.path(),ids),vec!["b","a","c"]);
        assert!(place(root.path(),"unknown",0,vec!["a".into()]).is_err());
    }
}
