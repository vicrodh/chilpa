//! I notify visible account settings only when their facade state changes.
#[cxx_qt::bridge]
pub mod qobject {
    #[auto_cxx_name]
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(i32, revision)]
        #[qproperty(bool, busy)]
        type DiscogsAccount = super::DiscogsAccountRust;
        #[qinvokable] fn refresh(self: Pin<&mut DiscogsAccount>);
    }
}
use cxx_qt::CxxQtType;
use std::pin::Pin;
pub struct DiscogsAccountRust {revision:i32,busy:bool,seen:u64}
impl Default for DiscogsAccountRust {
    fn default()->Self {Self {revision:0,busy:false,seen:u64::MAX}}
}
impl qobject::DiscogsAccount {
    pub fn refresh(mut self:Pin<&mut Self>) {
        let status=chilpa_qt_kit::app::with(|api|api.discogs());
        if status.revision==self.seen {return;}
        self.as_mut().rust_mut().seen=status.revision;
        self.as_mut().set_busy(status.checking);
        let revision=self.revision.wrapping_add(1);
        self.set_revision(revision);
    }
}
