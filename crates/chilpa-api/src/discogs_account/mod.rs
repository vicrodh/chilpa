//! I own one replaceable account command and keep keyring work off the window.
mod store;
mod wire;
mod worker;
#[cfg(test)] mod tests;
use chilpa_meta::discogs::{Credential,CredentialProvider};
use serde::Serialize;
use std::{cell::RefCell,sync::{Arc,Mutex,RwLock,OnceLock,mpsc,atomic::{AtomicU64,Ordering}}};

#[derive(Clone,Default,Debug,PartialEq,Eq,Serialize)]
pub struct DiscogsStatus {
    pub revision:u64,pub user:String,pub checking:bool,pub needs_code:bool,pub link:String,pub error:String,
}
enum Order {Token(String),OAuth,Finish(String),Disconnect,Cancel}
struct Shared {
    status:Mutex<DiscogsStatus>,credential:RwLock<Credential>,ready:OnceLock<()>,
    epoch:Arc<AtomicU64>,pending:Mutex<Option<(u64,Order)>>,
}
impl Shared {
    fn publish(&self,epoch:u64,update:impl FnOnce(&mut DiscogsStatus)) {
        let mut status=self.status.lock().unwrap_or_else(|e|e.into_inner());
        if self.epoch.load(Ordering::Acquire)!=epoch {return;}
        update(&mut status);status.revision=status.revision.wrapping_add(1);
    }
}
pub(crate) struct Account {
    shared:Arc<Shared>,wake:mpsc::SyncSender<()>,last:RefCell<DiscogsStatus>,
    quota:bool,quota_presented:bool,
}
impl Drop for Account {
    fn drop(&mut self) {self.shared.epoch.fetch_add(1,Ordering::AcqRel);}
}
impl Account {
    fn start(http:Arc<dyn chilpa_meta::Http>,root:std::path::PathBuf)->Self {
        Self::start_with(http,root,|root|chilpa_secrets::SecretBox::open("chilpa",root).map_err(|_|()))
    }
    fn start_with(http:Arc<dyn chilpa_meta::Http>,root:std::path::PathBuf,open:worker::OpenSecrets)->Self {
        let status=DiscogsStatus {checking:true,..Default::default()};
        let shared=Arc::new(Shared {status:Mutex::new(status.clone()),credential:RwLock::new(Credential::Anonymous),
            ready:OnceLock::new(),epoch:Arc::new(AtomicU64::new(0)),pending:Mutex::new(None)});
        let (wake,receiver)=mpsc::sync_channel(1);
        let owned=shared.clone();
        if std::thread::Builder::new().name("chilpa-discogs-account".into()).spawn(move||worker::run(owned,http,root,receiver,open)).is_err() {
            shared.publish(0,|s|{s.checking=false;s.error=chilpa_i18n::t("Discogs account setup could not start.");});
            let _=shared.ready.set(());
        }
        Self {shared,wake,last:RefCell::new(status),quota:false,quota_presented:false}
    }
    fn status(&self)->DiscogsStatus {
        if let Ok(status)=self.shared.status.try_lock() {*self.last.borrow_mut()=status.clone();}
        self.last.borrow().clone()
    }
    fn ask(&mut self,order:Order) {
        let epoch=self.shared.epoch.fetch_add(1,Ordering::AcqRel).wrapping_add(1);
        if matches!(order,Order::Disconnect) {*self.shared.credential.write().unwrap_or_else(|e|e.into_inner())=Credential::Anonymous;}
        self.shared.publish(epoch,|s|{s.checking=true;s.needs_code=false;s.link.clear();s.error.clear();});
        *self.shared.pending.lock().unwrap_or_else(|e|e.into_inner())=Some((epoch,order));
        if matches!(self.wake.try_send(()),Err(mpsc::TrySendError::Disconnected(_))) {
            self.shared.publish(epoch,|s|{s.checking=false;s.error=chilpa_i18n::t("Discogs account setup could not start.");});
        }
    }
    fn provider(&self)->CredentialProvider {
        let shared=self.shared.clone();
        Arc::new(move||{shared.ready.wait();shared.credential.read().unwrap_or_else(|e|e.into_inner()).clone()})
    }
}

impl crate::Chilpa {
    fn ensure_discogs(&mut self)->&mut Account {
        self.discogs_account.get_or_insert_with(||Account::start(Arc::new(crate::metadata::worker::Network::new()),chilpa_dirs::data().join("discogs")))
    }
    pub fn discogs(&mut self)->DiscogsStatus {
        let account=self.ensure_discogs();let status=account.status();
        if !status.user.is_empty() && !status.checking && status.error.is_empty() {account.quota=false;account.quota_presented=false;}
        status
    }
    pub(crate) fn discogs_provider(&mut self)->(CredentialProvider,Arc<AtomicU64>) {
        let account=self.ensure_discogs();(account.provider(),account.shared.epoch.clone())
    }
    pub fn connect_discogs_token(&mut self,token:&str) {
        if self.offline() {return;}
        if token.len()>4096 {return;}
        self.ensure_discogs().ask(Order::Token(token.into()));
    }
    pub fn connect_discogs_oauth(&mut self) {
        if !chilpa_core::network::current().allows(chilpa_meta::proxy::ORIGIN) {return;}
        self.ensure_discogs().ask(Order::OAuth);
    }
    pub fn finish_discogs_oauth(&mut self,code:&str) {
        if !self.offline() && code.len()<=8192 {self.ensure_discogs().ask(Order::Finish(code.into()));}
    }
    pub fn disconnect_discogs(&mut self) {self.ensure_discogs().ask(Order::Disconnect);}
    pub(crate) fn discogs_setting(&mut self,key:&str,value:&str)->bool {
        match key {
            "integrations.discogs.token"=>self.connect_discogs_token(value),
            "integrations.discogs.code"=>self.finish_discogs_oauth(value),
            _=>return false,
        }
        true
    }
    pub(crate) fn discogs_maintenance(&mut self,key:&str)->Option<String> {
        if key!="integrations.discogs.account" {return None;}
        let status=self.discogs();
        if status.checking {self.ensure_discogs().ask(Order::Cancel);}
        else if !status.user.is_empty() {self.disconnect_discogs();}
        else {self.connect_discogs_oauth();}
        Some(String::new())
    }
    pub(crate) fn discogs_row(&mut self,row:&mut crate::SettingsRow)->bool {
        if !row.key.starts_with("integrations.discogs.") {return false;}
        fill_account_row(row,self.discogs(),chilpa_core::network::current().proxy);
        true
    }
    pub fn discogs_quota_pending(&self)->bool {self.discogs_account.as_ref().is_some_and(|a|a.quota && !a.quota_presented)}
    pub fn take_discogs_quota(&mut self)->bool {
        let Some(account)=self.discogs_account.as_mut() else {return false;};
        if !account.quota || account.quota_presented {return false;}
        account.quota_presented=true;true
    }
    pub(crate) fn discogs_result(&mut self,epoch:u64,failure:Option<&chilpa_meta::Failure>)->bool {
        let Some(account)=self.discogs_account.as_mut() else {return epoch==0;};
        if account.shared.epoch.load(Ordering::Acquire)!=epoch {return false;}
        match failure {
            Some(chilpa_meta::Failure::Quota(_))=>account.quota=true,
            Some(chilpa_meta::Failure::Credential)=>account.shared.publish(epoch,|s|s.error=chilpa_i18n::t("Discogs did not accept your account. Connect it again in Settings > Integrations.")),
            None=>{account.quota=false;account.quota_presented=false;}
            _=>{}
        }
        true
    }
}

fn fill_account_row(row:&mut crate::SettingsRow,status:DiscogsStatus,proxy_enabled:bool) {
        match row.key.as_str() {
            "integrations.discogs.account"=>{
                if status.checking && status.needs_code {
                    row.control="external".into();row.action=chilpa_i18n::t("Open Discogs");row.value=status.link;
                    row.description=chilpa_i18n::t("Allow Chilpa on Discogs, then paste the code below.");
                } else if status.checking {
                    row.control="button".into();row.action=chilpa_i18n::t("Cancel");
                    row.description=chilpa_i18n::t("Checking the account…");
                } else if !status.user.is_empty() {
                    row.title=chilpa_i18n::t_args("Connected as {}",&[&status.user]);row.action=chilpa_i18n::t("Disconnect");
                    row.description=chilpa_i18n::t("Disconnecting forgets this account here. Revoke access at discogs.com/settings/applications.");
                }
                if !status.error.is_empty() {row.description=status.error;}
                if !proxy_enabled && status.user.is_empty() {
                    row.control="none".into();row.description=chilpa_i18n::t("Enable Chilpa proxy for browser sign-in, or connect with a personal token below.");
                }
            }
            "integrations.discogs.code" if !status.needs_code || !proxy_enabled=>row.control.clear(),
            "integrations.discogs.token" if !status.error.is_empty()=>row.description=status.error,
            _=>{}
        }
}
