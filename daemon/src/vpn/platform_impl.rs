use crate::platform::Platform;

#[derive(Clone)]
pub struct PlatformVpn {
    inner: Platform,
}

impl PlatformVpn {
    pub fn new() -> Self {
        Self {
            inner: Platform::new(),
        }
    }
}

impl std::ops::Deref for PlatformVpn {
    type Target = Platform;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl std::ops::DerefMut for PlatformVpn {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}
