//! I validate identity before keeping either kind of credential.
use chilpa_meta::{discogs::Credential, Failure, Http, Net, Reply};
use serde_json::Value;
use std::time::Duration;
pub(super) const CALLBACK:&str="https://api.chilpa.app/discogs/oauth/done";
const ORIGIN:&str=chilpa_meta::proxy::ORIGIN;
const TIMEOUT:Duration=Duration::from_secs(10);
const LARGEST:usize=16*1024;
pub(super) struct Begin {pub token:String,pub secret:String,pub url:String}
fn safe(value:&str)->bool {
    !value.is_empty() && value.len()<=4096 && value.bytes().all(|b|b.is_ascii_alphanumeric() || b"._~-".contains(&b))
}
fn decoded(reply:Reply)->Result<Value,Failure> {
    match reply.status {
        200=>serde_json::from_slice(&reply.body).map_err(|_|Failure::Unexpected("invalid authorization response".into())),
        401=>Err(Failure::Credential),
        429=>Err(Failure::Busy(reply.headers.get("retry-after").and_then(|v|v.parse().ok()).unwrap_or(60))),
        status=>Err(Failure::Unexpected(status.to_string())),
    }
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
    let reply=http.get(&format!("{ORIGIN}/discogs/oauth/begin?callback={}",chilpa_meta::text::encode(CALLBACK)),"application/json",TIMEOUT,LARGEST)
        .map_err(|_|Failure::Unreachable("authorization unavailable".into()))?;
    let doc=decoded(reply)?;
    let token=doc["request_token"].as_str().filter(|s|safe(s)).ok_or(Failure::Credential)?.to_string();
    let secret=doc["request_secret"].as_str().filter(|s|safe(s)).ok_or(Failure::Credential)?.to_string();
    let link=doc["authorize_url"].as_str().ok_or(Failure::Credential)?;
    let url=url::Url::parse(link).map_err(|_|Failure::Credential)?;
    if url.scheme()!="https" || url.host_str()!=Some("www.discogs.com") || url.path()!="/oauth/authorize"
        || !url.username().is_empty() || url.password().is_some() || url.port_or_known_default()!=Some(443) || url.fragment().is_some()
        || url.query_pairs().collect::<Vec<_>>()!=[("oauth_token".into(),token.clone().into())] {return Err(Failure::Credential);}
    Ok(Begin {token,secret,url:link.into()})
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
    Net::proxy(http).pace.wait();
    let address=format!("{ORIGIN}/discogs/oauth/finish?oauth_token={}&oauth_verifier={}",chilpa_meta::text::encode(&pending.token),chilpa_meta::text::encode(&code));
    let reply=http.get_as(&address,"application/json",TIMEOUT,LARGEST,&format!("Chilpa {}",pending.secret))
        .map_err(|_|Failure::Unreachable("authorization unavailable".into()))?;
    let doc=decoded(reply)?;
    let credential=Credential::OAuth {consumer_key:doc["consumer_key"].as_str().unwrap_or_default().into(),
        token:doc["token"].as_str().unwrap_or_default().into(),signature:doc["signature"].as_str().unwrap_or_default().into()};
    credential.header(1,"validation").ok_or(Failure::Credential)?;
    Ok(credential)
}
