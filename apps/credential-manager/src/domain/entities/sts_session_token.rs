use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;
use zeroize::Zeroize;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StsSessionToken {
    pub account_id: Uuid,
    pub access_key_id: String,
    pub secret_access_key: String,
    pub session_token: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub region: String,
}

impl StsSessionToken {
    pub fn is_expiring_within(&self, refresh_buffer_seconds: i64) -> bool {
        let Some(expires_at) = self.expires_at else {
            return false;
        };
        let buffer = Duration::seconds(refresh_buffer_seconds.max(0));
        Utc::now() + buffer >= expires_at
    }

    pub fn lock_memory(&self) {
        unsafe {
            if !self.access_key_id.is_empty() {
                libc::mlock(
                    self.access_key_id.as_ptr() as *const libc::c_void,
                    self.access_key_id.capacity(),
                );
            }
            if !self.secret_access_key.is_empty() {
                libc::mlock(
                    self.secret_access_key.as_ptr() as *const libc::c_void,
                    self.secret_access_key.capacity(),
                );
            }
            if let Some(ref t) = self.session_token {
                if !t.is_empty() {
                    libc::mlock(t.as_ptr() as *const libc::c_void, t.capacity());
                }
            }
        }
    }
}

impl Zeroize for StsSessionToken {
    fn zeroize(&mut self) {
        let ak_ptr = self.access_key_id.as_ptr();
        let ak_cap = self.access_key_id.capacity();
        let sk_ptr = self.secret_access_key.as_ptr();
        let sk_cap = self.secret_access_key.capacity();
        let (st_ptr, st_cap) = self
            .session_token
            .as_ref()
            .map(|s| (s.as_ptr(), s.capacity()))
            .unzip();

        self.access_key_id.zeroize();
        self.secret_access_key.zeroize();
        if let Some(ref mut t) = self.session_token {
            t.zeroize();
        }

        unsafe {
            if ak_cap > 0 {
                libc::munlock(ak_ptr as *const libc::c_void, ak_cap);
            }
            if sk_cap > 0 {
                libc::munlock(sk_ptr as *const libc::c_void, sk_cap);
            }
            if let Some((ptr, cap)) = st_ptr.zip(st_cap) {
                if cap > 0 {
                    libc::munlock(ptr as *const libc::c_void, cap);
                }
            }
        }
    }
}
