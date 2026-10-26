//! I wait without polling and retain one temporary OAuth context for five minutes.
use super::*;
use chilpa_secrets::SecretBox;
use std::{path::PathBuf,time::{Instant,Duration},sync::mpsc::Receiver};
const CONNECTING:Duration=Duration::from_secs(300);
pub(super) type OpenSecrets=fn(&std::path::Path)->Result<SecretBox,()>;
pub(super) fn run(shared:Arc<Shared>,http:Arc<dyn chilpa_meta::Http>,root:PathBuf,receiver:Receiver<()>,open:OpenSecrets) {
    let mut worker=Worker {shared,http,root,secrets:None,user:String::new(),pending:None,open,disk_error:false};
    worker.restore();let _=worker.shared.ready.set(());
    loop {
        let wait=worker.pending.as_ref().map(|(_,at)|at.saturating_duration_since(Instant::now()));
        let received=match wait {Some(wait)=>receiver.recv_timeout(wait),None=>receiver.recv().map_err(|_|mpsc::RecvTimeoutError::Disconnected)};
        match received {
            Err(mpsc::RecvTimeoutError::Disconnected)=>break,
            Err(mpsc::RecvTimeoutError::Timeout)=>{
                worker.pending=None;
                let epoch=worker.shared.epoch.load(Ordering::Acquire);
                worker.done(epoch,Some(chilpa_i18n::t("Discogs sign-in expired. Connect again.")));
            }
            Ok(())=>{
                let order=worker.shared.pending.lock().unwrap_or_else(|e|e.into_inner()).take();
                if let Some((epoch,order))=order {worker.handle(epoch,order);}
            }
        }
    }
}
struct Worker {
    shared:Arc<Shared>,http:Arc<dyn chilpa_meta::Http>,root:PathBuf,secrets:Option<SecretBox>,
    user:String,pending:Option<(wire::Begin,Instant)>,
    open:OpenSecrets,
    disk_error:bool,
}
impl Worker {
    fn current(&self,epoch:u64)->bool {self.shared.epoch.load(Ordering::Acquire)==epoch}
    fn secrets(&mut self)->Result<&SecretBox,()> {
        if self.secrets.is_none() {self.secrets=Some((self.open)(&self.root)?);}
        self.secrets.as_ref().ok_or(())
    }
    fn restore(&mut self) {
        if self.root.join("credential.sealed").try_exists().unwrap_or(true) {
            let root=self.root.clone();
            let restored=self.secrets().and_then(|secrets|store::load(&root,secrets));
            match restored {
                Ok(Some(saved)) if self.current(0)=>{
                    let mut live=self.shared.credential.write().unwrap_or_else(|e|e.into_inner());
                    if self.current(0) {self.user=saved.username;*live=saved.credential;}
                }
                Err(())=>return self.done(0,Some(chilpa_i18n::t("The saved Discogs account could not be opened. Connect again."))),
                _=>{}
            }
        }
        self.done(0,None);
    }
    fn done(&self,epoch:u64,error:Option<String>) {
        let mut error=error.unwrap_or_default();
        if self.disk_error {if !error.is_empty() {error.push('\n');}error.push_str(&Self::disk_warning());}
        self.shared.publish(epoch,|s|{s.user=self.user.clone();s.checking=false;s.needs_code=false;s.link.clear();s.error=error;});
    }
    fn disk_warning()->String {chilpa_i18n::t("The saved Discogs account could not be restored after cancellation. Review the account before restarting.")}
    fn restore_previous(&mut self,previous:&store::Saved) {
        let anonymous=matches!(*self.shared.credential.read().unwrap_or_else(|e|e.into_inner()),Credential::Anonymous);
        let root=self.root.clone();
        let restored=if anonymous {store::forget(&root)} else {self.secrets().and_then(|secrets|store::save(&root,secrets,previous))};
        self.disk_error=restored.is_err();
        if self.disk_error {
            let epoch=self.shared.epoch.load(Ordering::Acquire);
            self.shared.publish(epoch,|s|s.error=Self::disk_warning());
        }
    }
    fn failed(&self,epoch:u64,failure:chilpa_meta::Failure) {
        self.done(epoch,Some(Self::message(failure)));
    }
    fn message(failure:chilpa_meta::Failure)->String {
        match failure {
            chilpa_meta::Failure::Credential=>chilpa_i18n::t("Discogs did not accept the token or authorization code. Connect again."),
            chilpa_meta::Failure::Busy(seconds)=>chilpa_i18n::t_args("{} is busy; try again in {} seconds",&["Discogs",&seconds.to_string()]),
            _=>chilpa_i18n::t("Discogs could not be reached. Try connecting again."),
        }
    }
    fn accept(&mut self,epoch:u64,credential:Credential) {
        if !self.current(epoch) {return;}
        let user=match wire::identity(self.http.as_ref(),&credential) {Ok(user)=>user,Err(failure)=>return self.failed(epoch,failure)};
        if !self.current(epoch) {return;}
        let root=self.root.clone();
        let previous=store::Saved {credential:self.shared.credential.read().unwrap_or_else(|e|e.into_inner()).clone(),username:self.user.clone()};
        let saved=store::Saved {credential:credential.clone(),username:user.clone()};
        if self.secrets().and_then(|secrets|store::save(&root,secrets,&saved)).is_err() {
            return self.done(epoch,Some(chilpa_i18n::t("The Discogs account could not be saved. The connection was not changed.")));
        }
        let committed={
            let mut live=self.shared.credential.write().unwrap_or_else(|e|e.into_inner());
            if self.current(epoch) {*live=credential;true} else {false}
        };
        if !committed {self.restore_previous(&previous);return;}
        self.disk_error=false;self.user=user;self.done(epoch,None);
    }
    fn handle(&mut self,epoch:u64,order:Order) {
        if !self.current(epoch) {return;}
        if !matches!(order,Order::Finish(_)) {self.pending=None;}
        match order {
            Order::Token(token)=>match Credential::personal(&token) {
                Some(credential)=>self.accept(epoch,credential),None=>self.failed(epoch,chilpa_meta::Failure::Credential),
            },
            Order::Disconnect=>{
                *self.shared.credential.write().unwrap_or_else(|e|e.into_inner())=Credential::Anonymous;
                self.user.clear();
                let removed=store::forget(&self.root);
                if removed.is_ok() {self.disk_error=false;}
                let error=removed.err().map(|_|chilpa_i18n::t("Discogs is disconnected for now, but its saved account could not be removed. It may reconnect after restarting."));
                self.done(epoch,error);
            }
            Order::Cancel=>self.done(epoch,None),
            Order::Finish(code)=>{
                let Some((pending,until))=self.pending.take().filter(|(_,at)|*at>Instant::now()) else {
                    return self.done(epoch,Some(chilpa_i18n::t("Discogs sign-in expired. Connect again.")));
                };
                match wire::finish(self.http.as_ref(),&pending,&code) {
                    Ok(credential)=>self.accept(epoch,credential),
                    Err(failure)=>{
                        self.shared.publish(epoch,|s|{s.error=Self::message(failure);s.needs_code=true;s.checking=true;s.link=pending.url.clone();});
                        self.pending=Some((pending,until));
                    }
                }
            }
            Order::OAuth=>{
                let begin=match wire::begin(self.http.as_ref()) {Ok(begin)=>begin,Err(failure)=>return self.failed(epoch,failure)};
                if !self.current(epoch) {return;}
                if let Some(window)=crate::web_sign_in::executable() {
                    let capture=crate::web_sign_in::Capture {url:begin.url.clone(),until:wire::CALLBACK.into(),action:"finish".into(),field:"redirect_url".into(),user_agent:None};
                    let outcome=crate::web_sign_in::run_controlled(&window,&capture,"Discogs",CONNECTING,
                        ||!self.current(epoch) || !chilpa_core::network::current().allows(chilpa_meta::proxy::ORIGIN));
                    if !self.current(epoch) {return;}
                    match outcome {
                        crate::web_sign_in::Outcome::Reached(address)=>match wire::finish(self.http.as_ref(),&begin,&address) {
                            Ok(credential)=>self.accept(epoch,credential),Err(failure)=>self.failed(epoch,failure),
                        },
                        crate::web_sign_in::Outcome::Closed=>self.done(epoch,Some(chilpa_i18n::t("Discogs sign-in was cancelled."))),
                        crate::web_sign_in::Outcome::Failed(_)=>self.browser(epoch,begin),
                    }
                } else {self.browser(epoch,begin);}
            }
        }
    }
    fn browser(&mut self,epoch:u64,begin:wire::Begin) {
        self.shared.publish(epoch,|s|{s.link=begin.url.clone();s.needs_code=true;s.checking=true;});
        self.pending=Some((begin,Instant::now()+CONNECTING));
    }
}
