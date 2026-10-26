use super::*;
use chilpa_meta::{Http,Reply};
use std::{collections::VecDeque,path::Path,time::{Duration,Instant}};
#[derive(Default)]
struct Network {replies:Mutex<VecDeque<Reply>>,asked:Mutex<Vec<(String,Option<String>)>>}
impl Network {
    fn reply(self,status:u16,body:&str)->Self {
        self.replies.lock().unwrap().push_back(Reply {status,headers:Default::default(),body:body.as_bytes().to_vec()});self
    }
    fn next(&self,url:&str,authorization:Option<&str>)->Result<Reply,String> {
        self.asked.lock().unwrap().push((url.into(),authorization.map(str::to_string)));
        self.replies.lock().unwrap().pop_front().ok_or("unexpected request".into())
    }
}
impl Http for Network {
    fn get(&self,url:&str,_:&str,_:Duration,_:usize)->Result<Reply,String> {self.next(url,None)}
    fn get_as(&self,url:&str,_:&str,_:Duration,_:usize,authorization:&str)->Result<Reply,String> {self.next(url,Some(authorization))}
}
fn derived(root:&Path)->Result<chilpa_secrets::SecretBox,()> {chilpa_secrets::SecretBox::derived_only("discogs-unit",root).map_err(|_|())}
fn ready(account:&Account)->DiscogsStatus {
    let start=Instant::now();
    loop {let status=account.status();if !status.checking {return status;}
        assert!(start.elapsed()<Duration::from_secs(12),"the account did not finish");std::thread::sleep(Duration::from_millis(10));}
}
#[test]
fn sealed_roundtrip_keeps_username_and_token_out_of_plain_files_and_disconnect_removes_it() {
    let root=tempfile::tempdir().unwrap();let secrets=derived(root.path()).unwrap();
    let saved=store::Saved {credential:Credential::Token("MARKER_TOKEN".into()),username:"MARKER_USER".into()};
    store::save(root.path(),&secrets,&saved).unwrap();
    let restored=store::load(root.path(),&secrets).unwrap().unwrap();
    assert_eq!(restored.credential,saved.credential);assert_eq!(restored.username,saved.username);
    for entry in std::fs::read_dir(root.path()).unwrap().flatten() {
        let bytes=std::fs::read(entry.path()).unwrap();let text=String::from_utf8_lossy(&bytes);
        assert!(!text.contains("MARKER_TOKEN") && !text.contains("MARKER_USER"));
    }
    store::forget(root.path()).unwrap();assert!(store::load(root.path(),&secrets).unwrap().is_none());
}
#[test]
fn damaged_or_oversized_credentials_are_preserved_without_becoming_an_account() {
    let root=tempfile::tempdir().unwrap();let secrets=derived(root.path()).unwrap();let file=root.path().join("credential.sealed");
    for bytes in [b"damaged".to_vec(),vec![1;33000]] {
        std::fs::write(&file,&bytes).unwrap();assert!(store::load(root.path(),&secrets).is_err());
        assert_eq!(std::fs::read(&file).unwrap(),bytes);
    }
}
#[test]
fn bad_tokens_are_not_kept_and_valid_tokens_publish_the_verified_user() {
    for status in [401,200] {
        let root=tempfile::tempdir().unwrap();let network=Arc::new(Network::default().reply(status,r#"{"username":"tester"}"#));
        let mut account=Account::start_with(network.clone(),root.path().into(),derived);ready(&account);
        account.ask(Order::Token("MARKER_TOKEN".into()));let result=ready(&account);
        assert_eq!(result.user,if status==200 {"tester"} else {""});
        assert_eq!(root.path().join("credential.sealed").exists(),status==200);
        assert_eq!(network.asked.lock().unwrap()[0],("https://api.discogs.com/oauth/identity".into(),Some("Discogs token=MARKER_TOKEN".into())));
    }
}
#[test]
fn status_never_waits_for_the_account_lock_or_the_keyring() {
    let root=tempfile::tempdir().unwrap();let account=Account::start_with(Arc::new(Network::default()),root.path().into(),derived);
    let shared=account.shared.clone();let _held=shared.status.lock().unwrap();let began=Instant::now();
    let _=account.status();assert!(began.elapsed()<Duration::from_millis(50));
}
#[test]
fn quota_is_presented_once_until_a_success_rearms_it_and_old_account_results_are_ignored() {
    let root=tempfile::tempdir().unwrap();let account=Account::start_with(Arc::new(Network::default()),root.path().into(),derived);ready(&account);
    let mut api=crate::Chilpa::new();api.discogs_account=Some(account);
    api.discogs_result(0,Some(&chilpa_meta::Failure::Quota(60)));assert!(api.take_discogs_quota());assert!(!api.take_discogs_quota());
    api.discogs_result(0,None);api.discogs_result(0,Some(&chilpa_meta::Failure::Quota(60)));assert!(api.take_discogs_quota());
    api.discogs_result(0,None);api.discogs_account.as_mut().unwrap().shared.epoch.store(1,Ordering::Release);
    api.discogs_result(0,Some(&chilpa_meta::Failure::Quota(60)));assert!(!api.discogs_quota_pending());
}
#[test]
fn callback_addresses_bind_the_verifier_to_the_exact_pending_request() {
    let pending=wire::Begin {token:"request".into(),secret:"MARKER_SECRET".into(),url:String::new()};
    assert_eq!(wire::verifier(" abc123 ",&pending),Some("abc123".into()));
    assert_eq!(wire::verifier(&format!("{}?oauth_token=request&oauth_verifier=abc123",wire::CALLBACK),&pending),Some("abc123".into()));
    for address in ["https://api.chilpa.app/discogs/oauth/done.evil?oauth_token=request&oauth_verifier=abc123",
        "https://api.chilpa.app/discogs/oauth/done?oauth_token=other&oauth_verifier=abc123",
        "https://api.chilpa.app/discogs/oauth/done?oauth_token=request&oauth_verifier=a&oauth_verifier=b","<script>"] {
        assert!(wire::verifier(address,&pending).is_none());
    }
}
#[test]
fn oauth_finish_sends_the_request_secret_only_in_the_authorization_header() {
    let network=Network::default().reply(200,r#"{"consumer_key":"consumer","token":"access","signature":"client&access-secret"}"#);
    let pending=wire::Begin {token:"request".into(),secret:"MARKER_SECRET".into(),url:String::new()};
    let credential=wire::finish(&network,&pending,"abc123").unwrap();
    assert!(credential.header(1,"nonce").unwrap().contains("client%26access-secret"));
    let asked=network.asked.lock().unwrap();assert!(!asked[0].0.contains("MARKER_SECRET"));
    assert_eq!(asked[0].1.as_deref(),Some("Chilpa MARKER_SECRET"));
}
#[test]
fn oauth_begin_finish_and_identity_keep_signing_material_out_of_urls() {
    let network=Network::default()
        .reply(200,r#"{"request_token":"request","request_secret":"MARKER_SECRET","authorize_url":"https://www.discogs.com/oauth/authorize?oauth_token=request"}"#)
        .reply(200,r#"{"consumer_key":"consumer","token":"access","signature":"client&access-secret"}"#)
        .reply(200,r#"{"username":"tester"}"#);
    let pending=wire::begin(&network).unwrap();
    let credential=wire::finish(&network,&pending,&format!("{}?oauth_token=request&oauth_verifier=code",wire::CALLBACK)).unwrap();
    assert_eq!(wire::identity(&network,&credential).unwrap(),"tester");
    let asked=network.asked.lock().unwrap();assert_eq!(asked.len(),3);
    assert!(asked.iter().all(|(url,_)|!url.contains("MARKER_SECRET") && !url.contains("access-secret")));
    assert!(asked[2].1.as_ref().unwrap().contains("client%26access-secret"));
}
fn gated_secrets(root:&Path)->Result<chilpa_secrets::SecretBox,()> {
    std::fs::write(root.join("keystore-entered"),b"ready").map_err(|_|())?;
    let began=Instant::now();
    while !root.join("keystore-release").exists() {
        if began.elapsed()>Duration::from_secs(12) {return Err(());}
        std::thread::sleep(Duration::from_millis(10));
    }
    derived(root)
}
#[test]
fn disconnect_during_a_slow_keystore_cannot_publish_or_restore_a_late_credential() {
    let root=tempfile::tempdir().unwrap();let network=Arc::new(Network::default().reply(200,r#"{"username":"tester"}"#));
    let mut account=Account::start_with(network,root.path().into(),gated_secrets);ready(&account);
    account.ask(Order::Token("MARKER_TOKEN".into()));
    let began=Instant::now();
    while !root.path().join("keystore-entered").exists() {
        assert!(began.elapsed()<Duration::from_secs(12));std::thread::sleep(Duration::from_millis(10));
    }
    account.ask(Order::Disconnect);
    assert_eq!(account.provider()(),Credential::Anonymous);
    std::fs::write(root.path().join("keystore-release"),b"continue").unwrap();
    assert!(ready(&account).user.is_empty());
    assert!(!root.path().join("credential.sealed").exists());
    assert_eq!(account.provider()(),Credential::Anonymous);
}
#[test]
fn account_rows_cover_connection_states_and_hide_browser_code_when_proxy_access_is_off() {
    fn row(key:&str)->crate::SettingsRow {crate::SettingsRow {key:key.into(),control:"button".into(),action:chilpa_i18n::t("Connect"),..Default::default()}}
    for status in [DiscogsStatus::default(),DiscogsStatus {checking:true,..Default::default()},
        DiscogsStatus {user:"tester".into(),..Default::default()},
        DiscogsStatus {checking:true,needs_code:true,link:"https://www.discogs.com/oauth/authorize?oauth_token=request".into(),..Default::default()}] {
        let mut account=row("integrations.discogs.account");fill_account_row(&mut account,status.clone(),true);
        if status.needs_code {assert_eq!(account.control,"external");assert_eq!(account.value,status.link);}
        else {assert_eq!(account.control,"button");assert_eq!(account.action,if status.checking {chilpa_i18n::t("Cancel")} else if status.user.is_empty() {chilpa_i18n::t("Connect")} else {chilpa_i18n::t("Disconnect")});}
        let mut code=row("integrations.discogs.code");code.control="secret".into();
        fill_account_row(&mut code,status.clone(),true);assert_eq!(code.control.is_empty(),!status.needs_code);
        fill_account_row(&mut code,status,false);assert!(code.control.is_empty());
    }
}
#[test]
fn verified_usernames_accept_unicode_but_not_control_characters_or_markup() {
    let key=Credential::Token("marker".into());
    let network=Network::default().reply(200,r#"{"username":"José"}"#);
    assert_eq!(wire::identity(&network,&key).unwrap(),"José");
    let network=Network::default().reply(200,r#"{"username":"<img>"}"#);
    assert!(wire::identity(&network,&key).is_err());
}
