//! I query the direct API first and keep the anonymous backup bounded.
use super::{Credential, TIMEOUT, LARGEST};
use crate::{Failure, Http, Net, Reply, http::retry_after};
use rand::RngExt;
use serde_json::Value;
use std::{sync::{Mutex, atomic::{AtomicUsize, Ordering}}, time::{Duration, Instant, SystemTime, UNIX_EPOCH}};

pub type CredentialProvider = std::sync::Arc<dyn Fn() -> Credential + Send + Sync>;
static ANONYMOUS_WAIT: Mutex<Option<Instant>> = Mutex::new(None);
static AUTHENTICATED_WAIT: Mutex<Option<Instant>> = Mutex::new(None);
static BACKUP_WAIT: Mutex<Option<Instant>> = Mutex::new(None);

fn remaining(wait:&Mutex<Option<Instant>>)->u64 {
    wait.lock().unwrap_or_else(|e|e.into_inner()).and_then(|at|at.checked_duration_since(Instant::now()))
        .map_or(0,|duration|duration.as_secs().saturating_add(1))
}
fn remember(wait:&Mutex<Option<Instant>>,reply:&Reply) {
    if reply.status == 429 || reply.headers.get("x-discogs-ratelimit-remaining").is_some_and(|n|n.trim()=="0") {
        *wait.lock().unwrap_or_else(|e|e.into_inner())=Some(Instant::now()+Duration::from_secs(retry_after(reply).unwrap_or(60).clamp(1,3600)));
    }
}

pub struct Session<'a> {
    http: &'a dyn Http,
    credential: &'a (dyn Fn() -> Credential + Send + Sync),
    backup_allowed: bool,
    paced: bool,
    /// Four API requests per operation, including retries and backup calls.
    remaining: AtomicUsize,
}
fn anonymous() -> Credential { Credential::Anonymous }
impl<'a> Session<'a> {
    pub fn new(http: &'a dyn Http, credential: &'a (dyn Fn() -> Credential + Send + Sync), backup_allowed: bool) -> Self {
        Self {http,credential,backup_allowed,paced:true,remaining:AtomicUsize::new(4)}
    }
    /// A deterministic anonymous session for injected transports and tests.
    pub fn free(http: &'a dyn Http) -> Self {
        Self {http,credential:&anonymous,backup_allowed:true,paced:false,remaining:AtomicUsize::new(4)}
    }
    pub fn with_credential(mut self, credential: &'a (dyn Fn() -> Credential + Send + Sync)) -> Self { self.credential=credential; self }
    pub fn without_backup(mut self) -> Self {self.backup_allowed=false;self}
    fn take(&self) -> Result<(),Failure> {
        self.remaining.fetch_update(Ordering::AcqRel,Ordering::Acquire,|left|left.checked_sub(1))
            .map(|_|()).map_err(|_|Failure::Busy(1))
    }
    fn waiting(&self, authenticated: bool) -> u64 {
        if !self.paced {return 0;}
        let wait=if authenticated {&AUTHENTICATED_WAIT} else {&ANONYMOUS_WAIT};
        remaining(wait)
    }
    fn remember(&self, authenticated: bool, reply: &Reply) {
        if !self.paced {return;}
        remember(if authenticated {&AUTHENTICATED_WAIT} else {&ANONYMOUS_WAIT},reply);
    }
    fn request(&self, url: &str, credential: &Credential, backup: bool) -> Result<Reply,String> {
        self.take().map_err(|_|"request budget exhausted".to_string())?;
        if self.paced {
            if backup {Net::proxy(self.http).pace.wait();}
            else {Net::discogs_direct(self.http,credential.authenticated()).pace.wait();}
        }
        if backup || !credential.authenticated() {return self.http.get(url,"application/json",TIMEOUT,LARGEST);}
        let now=SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        let nonce=format!("{:032x}",rand::rng().random::<u128>());
        let authorization=credential.header(now,&nonce).ok_or("invalid credential")?;
        self.http.get_as(url,"application/json",TIMEOUT,LARGEST,&authorization)
    }
    pub(super) fn get(&self, direct: &str, backup: &str) -> Result<Value,Failure> {
        let credential=(self.credential)();
        let authenticated=credential.authenticated();
        if authenticated && credential.header(1,"validation").is_none() {return Err(Failure::Credential);}
        let waiting=self.waiting(authenticated);
        let reply=if waiting > 0 {Ok(Reply {status:429,headers:[("retry-after".into(),waiting.to_string())].into(),body:Vec::new()})}
            else {self.request(direct,&credential,false)};
        let mut quota=None;
        match reply {
            Ok(reply) => {
                self.remember(authenticated,&reply);
                match reply.status {
                    200 => return decode(&reply),
                    401 if authenticated => return Err(Failure::Credential),
                    429 if authenticated => return Err(Failure::Busy(retry_after(&reply).unwrap_or(60))),
                    429 => quota=Some(retry_after(&reply).unwrap_or(60)),
                    status => return Err(Failure::Unexpected(status.to_string())),
                }
            }
            Err(_) if authenticated => return Err(Failure::Unreachable("direct API unavailable".into())),
            Err(_) => {}
        }
        if (self.credential)() != credential {return Err(Failure::Busy(1));}
        if !self.backup_allowed {return Err(quota.map_or_else(||Failure::Unreachable("direct API unavailable".into()),Failure::Quota));}
        if self.paced {
            let wait=remaining(&BACKUP_WAIT);if wait>0 {return Err(Failure::Quota(wait));}
        }
        let reply=self.request(backup,&Credential::Anonymous,true);
        if self.paced {if let Ok(reply)=&reply {remember(&BACKUP_WAIT,reply);}}
        match reply {
            Ok(reply) if reply.status == 200 => decode(&reply),
            Ok(reply) if reply.status == 429 => Err(Failure::Quota(retry_after(&reply).unwrap_or(quota.unwrap_or(60)))),
            _ if quota.is_some() => Err(Failure::Quota(quota.unwrap())),
            Ok(reply) => Err(Failure::Refused(format!("backup status {}",reply.status))),
            Err(_) => Err(Failure::Unreachable("direct API and backup unavailable".into())),
        }
    }
}
fn decode(reply: &Reply) -> Result<Value,Failure> {
    serde_json::from_slice(&reply.body).map_err(|_|Failure::Unexpected("invalid JSON response".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::testing::Canned;
    const DIRECT:&str="https://api.discogs.com/releases/1";
    const BACKUP:&str="https://api.chilpa.app/discogs/release/1";
    #[test]
    fn anonymous_success_uses_the_direct_api_without_authorization() {
        let http=Canned::new().json("{}");
        Session::free(&http).get(DIRECT,BACKUP).unwrap();
        assert_eq!(http.asked(),[DIRECT]);assert_eq!(http.authorization(),[None]);
    }
    #[test]
    fn anonymous_quota_or_network_failure_can_use_the_backup() {
        for direct in [429,0] {
            let http=if direct == 0 {Canned::new().silence()} else {Canned::new().status(direct,&[],b"")};
            let http=http.json("{}");
            Session::free(&http).get(DIRECT,BACKUP).unwrap();
            assert_eq!(http.asked(),[DIRECT,BACKUP]);assert_eq!(http.authorization(),[None,None]);
        }
    }
    #[test]
    fn exhausted_anonymous_routes_report_quota_with_or_without_backup() {
        let http=Canned::new().status(429,&[],b"").status(429,&[("retry-after","7")],b"");
        assert_eq!(Session::free(&http).get(DIRECT,BACKUP),Err(Failure::Quota(7)));
        let http=Canned::new().status(429,&[("retry-after","8")],b"");
        assert_eq!(Session::free(&http).without_backup().get(DIRECT,BACKUP),Err(Failure::Quota(8)));
        assert_eq!(http.asked(),[DIRECT]);
    }
    #[test]
    fn two_unreachable_routes_remain_a_connection_failure() {
        let http=Canned::new().silence().silence();
        assert!(matches!(Session::free(&http).get(DIRECT,BACKUP),Err(Failure::Unreachable(_))));
        assert_eq!(http.asked().len(),2);
    }
    #[test]
    fn signed_requests_never_fall_back_to_anonymous_after_rejection_or_quota() {
        let credential=||Credential::Token("MARKER_TOKEN".into());
        for (status,expected) in [(401,Failure::Credential),(429,Failure::Busy(9))] {
            let http=Canned::new().status(status,&[("retry-after","9")],b"MARKER_TOKEN");
            assert_eq!(Session::free(&http).with_credential(&credential).get(DIRECT,BACKUP),Err(expected));
            assert_eq!(http.asked(),[DIRECT]);
            assert_eq!(http.authorization(),[Some("Discogs token=MARKER_TOKEN".into())]);
        }
    }
    #[test]
    fn unexpected_statuses_are_not_retried_and_never_echo_the_body() {
        for status in [401,403,404,500] {
            let http=Canned::new().status(status,&[],b"REFLECTED_SECRET");
            assert_eq!(Session::free(&http).get(DIRECT,BACKUP),Err(Failure::Unexpected(status.to_string())));
            assert_eq!(http.asked().len(),1);
        }
    }
    #[test]
    fn every_request_reads_the_current_credential_without_restarting_the_session() {
        let current=Mutex::new(Credential::Anonymous);
        let read=||current.lock().unwrap().clone();
        let http=Canned::new().json("{}").json("{}").json("{}");
        let session=Session::free(&http).with_credential(&read);
        session.get(DIRECT,BACKUP).unwrap();
        *current.lock().unwrap()=Credential::Token("connected".into());
        session.get(DIRECT,BACKUP).unwrap();
        *current.lock().unwrap()=Credential::Anonymous;
        session.get(DIRECT,BACKUP).unwrap();
        assert_eq!(http.authorization(),[None,Some("Discogs token=connected".into()),None]);
    }
    #[test]
    fn a_broken_credential_is_refused_before_opening_the_network() {
        let credential=||Credential::Token("bad\nheader".into());
        let http=Canned::new();
        assert_eq!(Session::free(&http).with_credential(&credential).get(DIRECT,BACKUP),Err(Failure::Credential));
        assert!(http.asked().is_empty());
    }
}

/// Images never receive a user credential, including when the API is signed.
pub fn fetch_image(http: &dyn Http, image: &str, backup_allowed: bool, timeout: Duration, largest: usize) -> Result<Reply,Failure> {
    if !super::is_discogs(image) {return Err(Failure::BadId);}
    if let Ok(reply)=http.get(image,"image/*",timeout,largest) {
        if reply.status == 200 {return Ok(reply);}
    }
    if !backup_allowed {return Err(Failure::Unreachable("image unavailable".into()));}
    let wait=remaining(&BACKUP_WAIT);if wait>0 {return Err(Failure::Quota(wait));}
    Net::proxy(http).pace.wait();
    let reply=http.get(&super::image_url(image)?,"image/*",timeout,largest).map_err(|_|Failure::Unreachable("image backup unavailable".into()))?;
    remember(&BACKUP_WAIT,&reply);Ok(reply)
}
