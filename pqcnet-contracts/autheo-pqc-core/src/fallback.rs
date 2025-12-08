#![cfg(all(feature = "liboqs", not(target_arch = "wasm32")))]

use crate::dsa::{MlDsa, MlDsaKeyPair};
use crate::error::{PqcError, PqcResult};
use crate::kem::{MlKem, MlKemEncapsulation, MlKemKeyPair};
use crate::types::Bytes;
use alloc::{boxed::Box, sync::Arc};
use core::sync::atomic::{AtomicBool, Ordering};

/// Shared switch used to steer fallback engines.
#[derive(Clone, Debug)]
pub struct FallbackSwitch {
    flag: Arc<AtomicBool>,
}

impl FallbackSwitch {
    fn new(flag: Arc<AtomicBool>) -> Self {
        Self { flag }
    }

    /// Force the backup engine for all future operations.
    pub fn force_backup(&self) {
        self.flag.store(true, Ordering::SeqCst);
    }

    /// Return to the primary engine.
    pub fn use_primary(&self) {
        self.flag.store(false, Ordering::SeqCst);
    }

    /// Report whether the backup path is currently active.
    pub fn is_forcing_backup(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }
}

fn should_failover(err: &PqcError) -> bool {
    matches!(
        err,
        PqcError::IntegrationError(_) | PqcError::PrimitiveFailure(_) | PqcError::InternalError(_)
    )
}

/// ML-KEM wrapper that can fall back from Kyber to HQC.
pub struct FallbackKem {
    primary: Box<dyn MlKem>,
    backup: Box<dyn MlKem>,
    switch: Arc<AtomicBool>,
    auto_failover: bool,
}

impl FallbackKem {
    pub fn new(
        primary: Box<dyn MlKem>,
        backup: Box<dyn MlKem>,
        auto_failover: bool,
    ) -> (Self, FallbackSwitch) {
        let switch = Arc::new(AtomicBool::new(false));
        (
            Self {
                primary,
                backup,
                switch: switch.clone(),
                auto_failover,
            },
            FallbackSwitch::new(switch),
        )
    }

    fn use_backup(&self) -> bool {
        self.switch.load(Ordering::SeqCst)
    }

    fn with_engine<T>(&self, op: impl Fn(&dyn MlKem) -> PqcResult<T>) -> PqcResult<T> {
        if self.use_backup() {
            op(self.backup.as_ref())
        } else {
            match op(self.primary.as_ref()) {
                Ok(value) => Ok(value),
                Err(err) if self.auto_failover && should_failover(&err) => {
                    self.switch.store(true, Ordering::SeqCst);
                    op(self.backup.as_ref())
                }
                Err(err) => Err(err),
            }
        }
    }
}

impl MlKem for FallbackKem {
    fn level(&self) -> crate::types::SecurityLevel {
        if self.use_backup() {
            self.backup.level()
        } else {
            self.primary.level()
        }
    }

    fn keygen(&self) -> PqcResult<MlKemKeyPair> {
        self.with_engine(|engine| engine.keygen())
    }

    fn encapsulate(&self, public_key: &[u8]) -> PqcResult<MlKemEncapsulation> {
        self.with_engine(|engine| engine.encapsulate(public_key))
    }

    fn decapsulate(&self, secret_key: &[u8], ciphertext: &[u8]) -> PqcResult<Bytes> {
        self.with_engine(|engine| engine.decapsulate(secret_key, ciphertext))
    }
}

/// ML-DSA wrapper that can fall back from Dilithium to SPHINCS+.
pub struct FallbackDsa {
    primary: Box<dyn MlDsa>,
    backup: Box<dyn MlDsa>,
    switch: Arc<AtomicBool>,
    auto_failover: bool,
}

impl FallbackDsa {
    pub fn new(
        primary: Box<dyn MlDsa>,
        backup: Box<dyn MlDsa>,
        auto_failover: bool,
    ) -> (Self, FallbackSwitch) {
        let switch = Arc::new(AtomicBool::new(false));
        (
            Self {
                primary,
                backup,
                switch: switch.clone(),
                auto_failover,
            },
            FallbackSwitch::new(switch),
        )
    }

    fn use_backup(&self) -> bool {
        self.switch.load(Ordering::SeqCst)
    }

    fn with_engine<T>(&self, op: impl Fn(&dyn MlDsa) -> PqcResult<T>) -> PqcResult<T> {
        if self.use_backup() {
            op(self.backup.as_ref())
        } else {
            match op(self.primary.as_ref()) {
                Ok(value) => Ok(value),
                Err(err) if self.auto_failover && should_failover(&err) => {
                    self.switch.store(true, Ordering::SeqCst);
                    op(self.backup.as_ref())
                }
                Err(err) => Err(err),
            }
        }
    }
}

impl MlDsa for FallbackDsa {
    fn level(&self) -> crate::types::SecurityLevel {
        if self.use_backup() {
            self.backup.level()
        } else {
            self.primary.level()
        }
    }

    fn keygen(&self) -> PqcResult<MlDsaKeyPair> {
        self.with_engine(|engine| engine.keygen())
    }

    fn sign(&self, sk: &[u8], message: &[u8]) -> PqcResult<Bytes> {
        self.with_engine(|engine| engine.sign(sk, message))
    }

    fn verify(&self, pk: &[u8], message: &[u8], signature: &[u8]) -> PqcResult<()> {
        self.with_engine(|engine| engine.verify(pk, message, signature))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::sync::Arc;
    use core::sync::atomic::{AtomicUsize, Ordering};

    enum KemBehavior {
        Success,
        InvalidInput,
    }

    struct MockKem {
        behavior: KemBehavior,
        calls: Arc<AtomicUsize>,
    }

    impl MockKem {
        fn new(behavior: KemBehavior, calls: Arc<AtomicUsize>) -> Self {
            Self { behavior, calls }
        }
    }

    impl MlKem for MockKem {
        fn level(&self) -> crate::types::SecurityLevel {
            crate::types::SecurityLevel::MlKem128
        }

        fn keygen(&self) -> PqcResult<MlKemKeyPair> {
            Err(PqcError::InternalError("unused"))
        }

        fn encapsulate(&self, _public_key: &[u8]) -> PqcResult<MlKemEncapsulation> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            match self.behavior {
                KemBehavior::Success => Ok(MlKemEncapsulation {
                    ciphertext: vec![0x42],
                    shared_secret: vec![0x24],
                }),
                KemBehavior::InvalidInput => Err(PqcError::InvalidInput("bad key")),
            }
        }

        fn decapsulate(&self, _secret_key: &[u8], _ciphertext: &[u8]) -> PqcResult<Bytes> {
            Err(PqcError::InternalError("unused"))
        }
    }

    enum DsaBehavior {
        Success,
        InvalidInput,
    }

    struct MockDsa {
        behavior: DsaBehavior,
        calls: Arc<AtomicUsize>,
    }

    impl MockDsa {
        fn new(behavior: DsaBehavior, calls: Arc<AtomicUsize>) -> Self {
            Self { behavior, calls }
        }
    }

    impl MlDsa for MockDsa {
        fn level(&self) -> crate::types::SecurityLevel {
            crate::types::SecurityLevel::MlDsa128
        }

        fn keygen(&self) -> PqcResult<MlDsaKeyPair> {
            Err(PqcError::InternalError("unused"))
        }

        fn sign(&self, _sk: &[u8], _message: &[u8]) -> PqcResult<Bytes> {
            Err(PqcError::InternalError("unused"))
        }

        fn verify(&self, _pk: &[u8], _message: &[u8], _signature: &[u8]) -> PqcResult<()> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            match self.behavior {
                DsaBehavior::Success => Ok(()),
                DsaBehavior::InvalidInput => Err(PqcError::InvalidInput("bad key")),
            }
        }
    }

    #[test]
    fn kem_forced_backup_does_not_retry_primary_on_invalid() {
        let primary_calls = Arc::new(AtomicUsize::new(0));
        let backup_calls = Arc::new(AtomicUsize::new(0));

        let (kem, switch) = FallbackKem::new(
            Box::new(MockKem::new(KemBehavior::Success, primary_calls.clone())),
            Box::new(MockKem::new(KemBehavior::InvalidInput, backup_calls.clone())),
            false,
        );

        switch.force_backup();

        let err = kem.encapsulate(b"pk").unwrap_err();
        assert!(matches!(err, PqcError::InvalidInput(_)));
        assert_eq!(backup_calls.load(Ordering::SeqCst), 1);
        assert_eq!(primary_calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn dsa_forced_backup_does_not_retry_primary_on_invalid() {
        let primary_calls = Arc::new(AtomicUsize::new(0));
        let backup_calls = Arc::new(AtomicUsize::new(0));

        let (dsa, switch) = FallbackDsa::new(
            Box::new(MockDsa::new(DsaBehavior::Success, primary_calls.clone())),
            Box::new(MockDsa::new(
                DsaBehavior::InvalidInput,
                backup_calls.clone(),
            )),
            false,
        );

        switch.force_backup();

        let err = dsa.verify(b"pk", b"msg", b"sig").unwrap_err();
        assert!(matches!(err, PqcError::InvalidInput(_)));
        assert_eq!(backup_calls.load(Ordering::SeqCst), 1);
        assert_eq!(primary_calls.load(Ordering::SeqCst), 0);
    }
}
