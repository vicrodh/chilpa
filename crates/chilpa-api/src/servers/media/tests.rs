use super::*;
use super::super::keys::Keys;
use chilpa_servers::{Reply, Request};
use std::collections::VecDeque;
use std::sync::Mutex;

const USER: &str = "501512e000854c8d832d2f1d2a26a0a0";
const TRACK: &str = "d29818a3701ef1b2d7fb8c90e2c0f2b2";
const ALBUM: &str = "b3a91c5e0d7f4e2a9c6b1f8e2d4a7c30";

struct Fixture {
    lib: chilpa_library::Library,
    keys: Keys,
    source: i64,
    track: i64,
    album: i64,
    track_reference: &'static str,
    album_reference: &'static str,
    _root: tempfile::TempDir,
}
impl Fixture {
    fn new() -> Self {
        Self::for_server("jellyfin",TRACK,ALBUM)
    }
    fn for_server(provider: &str, track_reference: &'static str, album_reference: &'static str) -> Self {
        let root = tempfile::tempdir().unwrap();
        let keys = Keys::derived(root.path().to_path_buf());
        let lib = chilpa_library::Library::in_memory().unwrap();
        let instance = super::super::keys::mint();
        let key = if provider == "plex" { chilpa_servers::Credential::Plex {account:"fixture-account".into(),server:"fixture-session".into()} }
            else { chilpa_servers::Credential::Token {token:"fixture-session".into()} };
        assert!(keys.put(&instance,&key));
        let source = lib.add_server(&chilpa_library::NewServer {
            provider:provider.into(),instance,label:"Music server".into(),address:"http://fixture.invalid".into(),identity:"fixture-machine".into(),
            libraries:"[]".into(),details:format!(r#"{{"user_id":"{USER}"}}"#),..Default::default()
        }).unwrap();
        let stored = lib.server(source).unwrap().unwrap();
        let generation = lib.begin_server_sync(source).unwrap();
        let tracks: Vec<_> = [track_reference,"124"].into_iter().map(|id| chilpa_library::ServerTrack {
            external_id:net::entry(&stored,id),title:"Track".into(),artist:"Artist".into(),album:"Album".into(),
            album_artist:"Artist".into(),album_key:album_reference.into(),..Default::default()
        }).collect();
        lib.save_server_tracks(source,generation,&tracks).unwrap();
        lib.finish_server_sync(source,generation).unwrap();
        let track = lib.track_by_entry(&net::entry(&stored,track_reference)).unwrap().unwrap().0;
        let album = lib.browse_ids(&crate::Filter::default(),crate::TrackOrder::Album).unwrap().album_ids[0];
        Self {lib,keys,source,track,album,track_reference,album_reference,_root:root}
    }
    fn write(&self, album: bool) -> writing::Write {
        let id = if album {self.album} else {self.track};
        writing::Write {session:super::super::session::Scope::of(&self.lib.server(self.source).unwrap().unwrap()),source:self.source,playlist:id,request:"media-fixture".into(),change:writing::Change::DeleteMedia {
            target:self.lib.server_media_target(album,id).unwrap().unwrap(),remote:if album {self.album_reference} else {self.track_reference}.into(),
        }}
    }
}
#[derive(Default)]
struct Answers { replies: Mutex<VecDeque<Reply>>, asked: Mutex<Vec<String>> }
impl Answers {
    fn ready(id: &str, kind: &str, permission: bool, deletion: u16) -> Self {
        let body = json!({"Items":[{"Id":id,"Type":kind,"CanDelete":permission}]}).to_string().into_bytes();
        Self {replies:Mutex::new([Reply {status:200,headers:Default::default(),body},Reply {status:deletion,headers:Default::default(),body:vec![]}].into()),..Default::default()}
    }
}
impl Net for Answers {
    fn send(&self, request: &Request) -> Result<Reply,String> {
        self.asked.lock().unwrap().push(request.method.to_string());
        self.replies.lock().unwrap().pop_front().ok_or_else(|| "unexpected request".into())
    }
}
#[test]
fn confirmed_music_deletion_changes_the_mirror_only_after_the_server_accepts() {
    for album in [false,true] {
        let fixture = Fixture::new();
        let net = Answers::ready(if album {ALBUM} else {TRACK},if album {"MusicAlbum"} else {"Audio"},true,204);
        let result = writing::apply(&fixture.lib,&net,&fixture.keys,&Mutex::new(()),&fixture.write(album));
        assert!(result.media);
        assert!(result.why.is_empty(),"{}",result.why);
        assert_eq!(fixture.lib.track_count(false).unwrap(),if album {0} else {1});
        assert_eq!(fixture.lib.track_count(true).unwrap(),2);
        assert_eq!(*net.asked.lock().unwrap(),["GET","DELETE"]);
    }
}
#[test]
fn refused_permissions_and_failed_deletions_leave_every_local_row_present() {
    for (permission,status,requests) in [(false,204,1),(true,403,2),(true,500,2)] {
        let fixture = Fixture::new();
        let net = Answers::ready(TRACK,"Audio",permission,status);
        let result = writing::apply(&fixture.lib,&net,&fixture.keys,&Mutex::new(()),&fixture.write(false));
        assert!(!result.why.is_empty());
        assert_eq!(fixture.lib.track_count(false).unwrap(),2);
        assert_eq!(net.asked.lock().unwrap().len(),requests);
    }
}
#[test]
fn local_confirmation_is_bound_to_the_same_server_and_item() {
    let fixture = Fixture::new();
    let mut api = Chilpa::with_listed(vec![],vec![]);
    let source = fixture.source;
    let id = fixture.track;
    api.library = Some(fixture.lib);
    let info: Value = serde_json::from_str(&api.server_media_info("track",id)).unwrap();
    assert_eq!(info["ref"],TRACK);
    let refused: Value = serde_json::from_str(&api.server_media_delete("track",id,"another item",source)).unwrap();
    assert!(refused["error"].is_string());
    assert!(api.servers.media.job.is_none());
    assert_eq!(api.library().unwrap().track_count(false).unwrap(),2);
    assert_eq!(api.server_media_info("track",-1),"{}");
}

#[test]
fn a_confirmed_deletion_cannot_cross_an_account_edit() {
    let fixture = Fixture::new();
    let write = fixture.write(false);
    let stored = fixture.lib.server(fixture.source).unwrap().unwrap();
    fixture.lib.edit_server(fixture.source,&chilpa_library::NewServer {
        provider:stored.provider,instance:stored.instance,label:stored.label,address:stored.address,
        account:"another-user".into(),identity:stored.identity,product:stored.product,
        libraries:stored.libraries,details:stored.details,..Default::default()
    }).unwrap();
    let net = Answers::ready(TRACK,"Audio",true,204);
    let result = writing::apply(&fixture.lib,&net,&fixture.keys,&Mutex::new(()),&write);
    assert!(!result.why.is_empty());
    assert!(net.asked.lock().unwrap().is_empty());
    assert_eq!(fixture.lib.track_count(true).unwrap(),2);
}

#[test]
fn numeric_music_ids_keep_the_same_confirmed_deletion_path() {
    for album in [false,true] {
        let fixture = Fixture::for_server("plex","123","456");
        let reference = if album {"456"} else {"123"};
        let item = json!({"MediaContainer":{"Metadata":[{"type":if album {"album"} else {"track"},"ratingKey":reference}]}});
        let policy = json!({"MediaContainer":{"machineIdentifier":"fixture-machine","allowMediaDeletion":true}});
        let replies = [policy,item].into_iter().map(|document| Reply {status:200,headers:Default::default(),body:document.to_string().into_bytes()})
            .chain([Reply {status:200,headers:Default::default(),body:vec![]}]).collect();
        let net = Answers {replies:Mutex::new(replies),..Default::default()};
        let result = writing::apply(&fixture.lib,&net,&fixture.keys,&Mutex::new(()),&fixture.write(album));
        assert!(result.why.is_empty(),"{}",result.why);
        assert_eq!(fixture.lib.track_count(false).unwrap(),if album {0} else {1});
        assert_eq!(*net.asked.lock().unwrap(),["GET","GET","DELETE"]);
    }
}
