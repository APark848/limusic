//! Explicit placeholder until the mobile PoToken provider is implemented.
//!
//! Keeping the desktop BotGuard module out of this target also keeps deno_core/rusty_v8 out of
//! the mobile dependency graph. The caller already degrades when minting fails.

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("BotGuard is not implemented on this platform")]
    Fatal,
    #[error("BotGuard is not implemented on this platform")]
    Transient,
}

pub struct Minter;

pub struct Bootstrap {
    pub minter: Minter,
    pub session_token: String,
    pub lifetime_secs: u64,
}

impl Minter {
    pub async fn spawn(_user_agent: String, _session_ident: String) -> Result<Bootstrap, Error> {
        Err(Error::Fatal)
    }

    pub async fn mint(&self, _ident: &str) -> Result<String, Error> {
        Err(Error::Transient)
    }
}
