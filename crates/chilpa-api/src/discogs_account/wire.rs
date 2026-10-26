//! I verify identity and exchange OAuth credentials from the client's own IP.
use chilpa_meta::{discogs::Credential, Failure, Http, Net, Reply};
use serde_json::Value;
use std::time::Duration;
pub(super) const CALLBACK:&str="https://api.chilpa.app/discogs/oauth/done";
const ORIGIN:&str=chilpa_meta::proxy::ORIGIN;
const TIMEOUT:Duration=Duration::from_secs(10);
const LARGEST:usize=16*1024;
pub(super) struct Begin {pub token:String,pub secret:String,pub url:String,pub consumer:String,pub signing:String}
fn safe(value:&str)->bool {
    !value.is_empty() && value.len()<=4096 && value.bytes().all(|b|b.is_ascii_alphanumeric() || b"._~-".contains(&b))
}
fn accepted(reply:Reply)->Result<Reply,Failure> {
    match reply.status {
        200=>Ok(reply),
        401=>Err(Failure::Credential),
        429=>Err(Failure::Busy(reply.headers.get("retry-after").and_then(|v|v.parse().ok()).unwrap_or(60))),
        status=>Err(Failure::Unexpected(status.to_string())),
    }
}
fn decoded(reply:Reply)->Result<Value,Failure> {
    serde_json::from_slice(&accepted(reply)?.body).map_err(|_|Failure::Unexpected("invalid authorization response".into()))
}
fn form(reply:Reply)->Result<std::collections::BTreeMap<String,String>,Failure> {
    let body=accepted(reply)?.body;
    if body.len()>LARGEST {return Err(Failure::Credential);}
    let mut values=std::collections::BTreeMap::new();
    for (key,value) in url::form_urlencoded::parse(&body) {
        if values.insert(key.into_owned(),value.into_owned()).is_some() {return Err(Failure::Credential);}
    }
    Ok(values)
}
fn signing_header(consumer:&str,token:Option<&str>,signature:&str,extra:Option<(&str,&str)>)->Result<String,Failure> {
    let now=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
    use rand::RngExt;
    let nonce=format!("{:032x}",rand::rng().random::<u128>());
    let mut header=Credential::sign_oauth(consumer,token,signature,now,&nonce).ok_or(Failure::Credential)?;
    if let Some((name,value))=extra {header.push_str(&format!(", {name}=\"{}\"",chilpa_meta::text::encode(value)));}
    Ok(header)
}
pub(super) fn identity(http:&dyn Http,credential:&Credential)->Result<String,Failure> {
    if !credential.authenticated() {return Err(Failure::Credential);}
    Net::discogs_direct(http,true).pace.wait();
    let now=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
    use rand::RngExt;
    let header=credential.header(now,&format!("{:032x}",rand::rng().random::<u128>())).ok_or(Failure::Credential)?;
    let reply=http.get_as("https://api.discogs.com/oauth/identity","application/json",TIMEOUT,LARGEST,&header)
        .map_err(|_|Failure::Unreachable("identity unavailable".into()))?;
    let doc=decoded(reply)?;
    let user=doc["username"].as_str().filter(|u|!u.is_empty() && u.len()<=128 && !u.chars().any(|c|c.is_control() || c=='<' || c=='>')).ok_or(Failure::Credential)?;
    Ok(user.into())
}
pub(super) fn begin(http:&dyn Http)->Result<Begin,Failure> {
    Net::proxy(http).pace.wait();
    let reply=http.get(&format!("{ORIGIN}/discogs/oauth/setup?callback={}",chilpa_meta::text::encode(CALLBACK)),"application/json",TIMEOUT,LARGEST)
        .map_err(|_|Failure::Unreachable("authorization unavailable".into()))?;
    let setup=decoded(reply)?;
    let consumer=setup["consumer_key"].as_str().filter(|s|safe(s)).ok_or(Failure::Credential)?.to_string();
    let signing=setup["signature"].as_str().filter(|s|s.ends_with('&') && s.matches('&').count()==1).ok_or(Failure::Credential)?.to_string();
    let header=signing_header(&consumer,None,&signing,Some(("oauth_callback",CALLBACK)))?;
    Net::discogs_direct(http,true).pace.wait();
    let reply=http.post_as("https://api.discogs.com/oauth/request_token","application/x-www-form-urlencoded",TIMEOUT,LARGEST,&header)
        .map_err(|_|Failure::Unreachable("authorization unavailable".into()))?;
    let values=form(reply)?;
    let token=values.get("oauth_token").filter(|s|safe(s)).ok_or(Failure::Credential)?.clone();
    let secret=values.get("oauth_token_secret").filter(|s|safe(s)).ok_or(Failure::Credential)?.clone();
    if values.get("oauth_callback_confirmed").map(String::as_str)!=Some("true") {return Err(Failure::Credential);}
    let url=format!("https://www.discogs.com/oauth/authorize?oauth_token={}",chilpa_meta::text::encode(&token));
    Ok(Begin {token,secret,url,consumer,signing})
}
pub(super) fn verifier(input:&str,pending:&Begin)->Option<String> {
    let input=input.trim();
    if input.len()>8192 {return None;}
    let code=if input.starts_with("https://") {
        let url=url::Url::parse(input).ok()?;
        if url.origin().ascii_serialization()!="https://api.chilpa.app" || url.path()!="/discogs/oauth/done"
            || !url.username().is_empty() || url.password().is_some() || url.fragment().is_some() {return None;}
        let pairs:Vec<_>=url.query_pairs().collect();
        let tokens:Vec<_>=pairs.iter().filter(|(k,_)|k=="oauth_token").collect();
        let codes:Vec<_>=pairs.iter().filter(|(k,_)|k=="oauth_verifier").collect();
        if tokens.len()!=1 || tokens[0].1!=pending.token || codes.len()!=1 {return None;}
        codes[0].1.to_string()
    } else {input.into()};
    (!code.is_empty() && code.len()<=128 && code.bytes().all(|b|b.is_ascii_alphanumeric())).then_some(code)
}
pub(super) fn finish(http:&dyn Http,pending:&Begin,input:&str)->Result<Credential,Failure> {
    let code=verifier(input,pending).ok_or(Failure::Credential)?;
    let signature=format!("{}{}",pending.signing,chilpa_meta::text::encode(&pending.secret));
    let header=signing_header(&pending.consumer,Some(&pending.token),&signature,Some(("oauth_verifier",&code)))?;
    Net::discogs_direct(http,true).pace.wait();
    let reply=http.post_as("https://api.discogs.com/oauth/access_token","application/x-www-form-urlencoded",TIMEOUT,LARGEST,&header)
        .map_err(|_|Failure::Unreachable("authorization unavailable".into()))?;
    let values=form(reply)?;
    let token=values.get("oauth_token").filter(|s|safe(s)).ok_or(Failure::Credential)?.clone();
    let secret=values.get("oauth_token_secret").filter(|s|safe(s)).ok_or(Failure::Credential)?;
    let credential=Credential::OAuth {consumer_key:pending.consumer.clone(),token,
        signature:format!("{}{}",pending.signing,chilpa_meta::text::encode(secret))};
    credential.header(1,"validation").ok_or(Failure::Credential)?;
    Ok(credential)
}
