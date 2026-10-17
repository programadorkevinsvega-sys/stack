use chrono::{DateTime, Utc};

/// Abstracción del reloj para tests deterministas.
///
/// En producción se usa `RealClock` que devuelve `Utc::now()`.
/// En tests se inyecta `FakeClock` con un timestamp fijo.
pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

/// Implementación real del reloj.
pub struct RealClock;

impl Clock for RealClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// Implementación fake para tests. El timestamp es fijo.
pub struct FakeClock {
    pub now: DateTime<Utc>,
}

impl Clock for FakeClock {
    fn now(&self) -> DateTime<Utc> {
        self.now
    }
}

impl FakeClock {
    pub fn new(now: DateTime<Utc>) -> Self {
        Self { now }
    }
}
