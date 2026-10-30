//! I keep the username and credential in the same bounded sealed file.
use chilpa_meta::discogs::Credential;
use chilpa_secrets::SecretBox;
use serde::{Deserialize,Serialize};
use std::{io::{Read,Write},path::Path};
const MAX_BYTES:u64=32*1024;
#[derive(Serialize,Deserialize)]
pub(super) struct Saved {pub credential:Credential,pub username:String}
pub(super) fn load(root:&Path,secrets:&SecretBox)->Result<Option<Saved>,()> {
    let file=match std::fs::File::open(root.join("credential.sealed")) {
        Ok(file)=>file,Err(e) if e.kind()==std::io::ErrorKind::NotFound=>return Ok(None),Err(_)=>return Err(()),
    };
    let mut bytes=Vec::new();file.take(MAX_BYTES+1).read_to_end(&mut bytes).map_err(|_|())?;
    if bytes.len() as u64>MAX_BYTES {return Err(());}
    let plain=secrets.open_sealed(&bytes).map_err(|e|eprintln!("chilpa-discogs: the saved account not opened: step={} backend={:?}",e.kind(),secrets.backend()))?;
    let saved:Saved=serde_json::from_slice(&plain).map_err(|_|())?;
    if saved.username.is_empty() || saved.username.len()>128 || saved.credential.header(1,"validation").is_none() {return Err(());}
    Ok(Some(saved))
}
pub(super) fn save(root:&Path,secrets:&SecretBox,saved:&Saved)->Result<(),()> {
    let plain=serde_json::to_vec(saved).map_err(|_|())?;
    if plain.len() as u64>MAX_BYTES-128 {return Err(());}
    let sealed=secrets.seal(&plain).map_err(|_|())?;
    std::fs::create_dir_all(root).map_err(|_|())?;
    let mut file=tempfile::NamedTempFile::new_in(root).map_err(|_|())?;
    file.write_all(&sealed).map_err(|_|())?;file.as_file().sync_all().map_err(|_|())?;
    file.persist(root.join("credential.sealed")).map_err(|_|())?;
    Ok(())
}
pub(super) fn forget(root:&Path)->Result<(),()> {
    match std::fs::remove_file(root.join("credential.sealed")) {
        Ok(())=>Ok(()),Err(e) if e.kind()==std::io::ErrorKind::NotFound=>Ok(()),Err(_)=>Err(()),
    }
}
