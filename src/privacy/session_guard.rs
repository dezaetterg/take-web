use std::cell::{Cell, RefCell};
use std::rc::Rc;

pub struct PrivateSessionGuard {
    password_hash: RefCell<Option<String>>,
    salt: RefCell<Option<String>>,
    is_locked: Cell<bool>,
    has_prompted_for_setup: Cell<bool>,
}

impl PrivateSessionGuard {
    pub fn new() -> Self {
        Self {
            password_hash: RefCell::new(None),
            salt: RefCell::new(None),
            is_locked: Cell::new(false),
            has_prompted_for_setup: Cell::new(false),
        }
    }

    pub fn has_password(&self) -> bool {
        self.password_hash.borrow().is_some()
    }

    pub fn is_locked(&self) -> bool {
        self.is_locked.get()
    }

    pub fn set_locked(&self, locked: bool) {
        self.is_locked.set(locked);
    }

    pub fn has_prompted(&self) -> bool {
        self.has_prompted_for_setup.get()
    }

    pub fn set_has_prompted(&self, prompted: bool) {
        self.has_prompted_for_setup.set(prompted);
    }

    pub fn set_password(&self, password: &str) {
        let trimmed = password.trim();
        if trimmed.is_empty() {
            *self.password_hash.borrow_mut() = None;
            *self.salt.borrow_mut() = None;
            return;
        }

        let salt = Self::generate_csprng_salt();
        let hash = Self::hash_password(&salt, trimmed);

        *self.salt.borrow_mut() = Some(salt);
        *self.password_hash.borrow_mut() = Some(hash);
        self.is_locked.set(false);
    }

    pub fn check_password(&self, password: &str) -> bool {
        let hash_guard = self.password_hash.borrow();
        let salt_guard = self.salt.borrow();

        match (hash_guard.as_ref(), salt_guard.as_ref()) {
            (Some(expected_hash), Some(salt)) => {
                let actual_hash = Self::hash_password(salt, password.trim());
                actual_hash == *expected_hash
            }
            (None, _) => true,
            _ => false,
        }
    }

    pub fn reset(&self) {
        *self.password_hash.borrow_mut() = None;
        *self.salt.borrow_mut() = None;
        self.is_locked.set(false);
        self.has_prompted_for_setup.set(false);
    }

    fn generate_csprng_salt() -> String {
        let mut buf = [0u8; 16];
        if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
            use std::io::Read;
            if f.read_exact(&mut buf).is_ok() {
                return buf.iter().map(|b| format!("{:02x}", b)).collect();
            }
        }
        format!("{:016x}{:016x}", glib::monotonic_time(), std::process::id())
    }

    fn hash_password(salt: &str, password: &str) -> String {
        const ITERATIONS: u32 = 50_000;
        let mut current = format!("{salt}:{password}");
        for _ in 0..ITERATIONS {
            if let Some(next) =
                glib::compute_checksum_for_string(glib::ChecksumType::Sha256, &current)
            {
                current = next.to_string();
            }
        }
        current
    }

    pub fn setup_system_lock_listener<F, W>(on_lock: F, on_wake: W)
    where
        F: Fn() + 'static,
        W: Fn() + 'static,
    {
        let lock_cb = Rc::new(on_lock);
        let wake_cb = Rc::new(on_wake);

        // 1. Subscribe to system sleep via logind on SYSTEM bus
        if let Ok(sys_bus) = gio::bus_get_sync(gio::BusType::System, gio::Cancellable::NONE) {
            let cb_lock = Rc::clone(&lock_cb);
            let cb_wake = Rc::clone(&wake_cb);
            let _ = sys_bus.subscribe_to_signal(
                Some("org.freedesktop.login1"),
                Some("org.freedesktop.login1.Manager"),
                Some("PrepareForSleep"),
                Some("/org/freedesktop/login1"),
                None,
                gio::DBusSignalFlags::NONE,
                move |signal| {
                    if let Some(is_sleeping) = signal.parameters.child_value(0).get::<bool>() {
                        if is_sleeping {
                            log::info!("System sleep detected - locking private session");
                            let cb = Rc::clone(&cb_lock);
                            glib::idle_add_local_once(move || {
                                cb();
                            });
                        } else {
                            log::info!("System wake detected - restoring theme and rendering");
                            let cb = Rc::clone(&cb_wake);
                            glib::idle_add_local_once(move || {
                                cb();
                            });
                        }
                    }
                },
            );
        }

        // 2. Subscribe to screen lock on SESSION bus
        if let Ok(ses_bus) = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE) {
            let cb_lock = Rc::clone(&lock_cb);
            let cb_wake = Rc::clone(&wake_cb);
            let _ = ses_bus.subscribe_to_signal(
                None,
                None,
                Some("ActiveChanged"),
                None,
                None,
                gio::DBusSignalFlags::NONE,
                move |signal| {
                    if let Some(is_active) = signal.parameters.child_value(0).get::<bool>() {
                        if is_active {
                            log::info!("Screen lock detected - locking private session");
                            let cb = Rc::clone(&cb_lock);
                            glib::idle_add_local_once(move || {
                                cb();
                            });
                        } else {
                            log::info!("Screen unlock detected - refreshing theme and rendering");
                            let cb = Rc::clone(&cb_wake);
                            glib::idle_add_local_once(move || {
                                cb();
                            });
                        }
                    }
                },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_guard_initial_state() {
        let guard = PrivateSessionGuard::new();
        assert!(!guard.has_password());
        assert!(!guard.is_locked());
        assert!(!guard.has_prompted());
    }

    #[test]
    fn test_guard_password_verification() {
        let guard = PrivateSessionGuard::new();
        guard.set_password("secret123");
        assert!(guard.has_password());
        assert!(guard.check_password("secret123"));
        assert!(!guard.check_password("wrong_password"));
        assert!(!guard.check_password(""));
    }

    #[test]
    fn test_guard_lock_and_reset() {
        let guard = PrivateSessionGuard::new();
        guard.set_password("mypass");
        guard.set_has_prompted(true);
        guard.set_locked(true);
        assert!(guard.is_locked());

        guard.reset();
        assert!(!guard.has_password());
        assert!(!guard.is_locked());
        assert!(!guard.has_prompted());
    }
}
