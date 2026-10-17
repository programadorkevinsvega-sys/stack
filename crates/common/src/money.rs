use rust_decimal::Decimal;
use std::ops::{Add, Sub};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MoneyError {
    #[error("currency must be a 3-letter ISO 4217 code")]
    InvalidCurrencyCode,
    #[error("amount cannot be negative")]
    NegativeAmount,
    #[error("currency mismatch: {left} vs {right}")]
    CurrencyMismatch { left: String, right: String },
    #[error("cannot subtract {attempted} from {available} {currency}")]
    InsufficientFunds {
        attempted: Decimal,
        available: Decimal,
        currency: String,
    },
}

/// Representación segura de dinero. Nunca usa floats para aritmética.
///
/// La moneda se almacena como código ISO 4217 (3 letras mayúsculas).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Money {
    pub amount: Decimal,
    pub currency: String,
}

impl Money {
    /// Crea un nuevo Money validando que el monto no sea negativo y la moneda sea un código válido.
    pub fn new(amount: Decimal, currency: impl Into<String>) -> Result<Self, MoneyError> {
        let currency = currency.into().to_uppercase();
        if currency.len() != 3 || !currency.chars().all(|c| c.is_ascii_uppercase()) {
            return Err(MoneyError::InvalidCurrencyCode);
        }
        if amount < Decimal::ZERO {
            return Err(MoneyError::NegativeAmount);
        }
        Ok(Self { amount, currency })
    }

    /// Crea Money con valor cero en la moneda indicada.
    pub fn zero(currency: impl Into<String>) -> Result<Self, MoneyError> {
        Self::new(Decimal::ZERO, currency)
    }

    /// Crea Money en USD (caso común).
    pub fn usd(amount: Decimal) -> Result<Self, MoneyError> {
        Self::new(amount, "USD")
    }

    /// Verifica que dos Money tengan la misma moneda. Si no, devuelve error.
    pub fn same_currency(&self, other: &Self) -> Result<(), MoneyError> {
        if self.currency == other.currency {
            Ok(())
        } else {
            Err(MoneyError::CurrencyMismatch {
                left: self.currency.clone(),
                right: other.currency.clone(),
            })
        }
    }

    pub fn is_zero(&self) -> bool {
        self.amount == Decimal::ZERO
    }

    pub fn checked_add(&self, other: &Self) -> Result<Self, MoneyError> {
        self.same_currency(other)?;
        Self::new(self.amount + other.amount, self.currency.clone())
    }

    pub fn checked_sub(&self, other: &Self) -> Result<Self, MoneyError> {
        self.same_currency(other)?;
        if self.amount < other.amount {
            return Err(MoneyError::InsufficientFunds {
                attempted: other.amount,
                available: self.amount,
                currency: self.currency.clone(),
            });
        }
        Self::new(self.amount - other.amount, self.currency.clone())
    }

    pub fn checked_mul(&self, factor: Decimal) -> Result<Self, MoneyError> {
        Self::new(self.amount * factor, self.currency.clone())
    }

    pub fn checked_div(&self, divisor: Decimal) -> Result<Self, MoneyError> {
        if divisor == Decimal::ZERO {
            return Err(MoneyError::NegativeAmount);
        }
        Self::new(self.amount / divisor, self.currency.clone())
    }

    pub fn percentage(&self, percent: Decimal) -> Result<Self, MoneyError> {
        self.checked_mul(percent / Decimal::from(100))
    }
}

impl Add for Money {
    type Output = Result<Self, MoneyError>;

    fn add(self, rhs: Self) -> Self::Output {
        self.checked_add(&rhs)
    }
}

impl Sub for Money {
    type Output = Result<Self, MoneyError>;

    fn sub(self, rhs: Self) -> Self::Output {
        self.checked_sub(&rhs)
    }
}

impl PartialOrd for Money {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        if self.currency != other.currency {
            return None;
        }
        self.amount.partial_cmp(&other.amount)
    }
}

impl std::fmt::Display for Money {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.amount, self.currency)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;

    #[test]
    fn test_money_new_valid() {
        let m = Money::new(Decimal::new(1000, 2), "USD").expect("valid money should be created"); // 10.00
        assert_eq!(m.amount, Decimal::new(1000, 2));
        assert_eq!(m.currency, "USD");
    }

    #[test]
    fn test_money_new_negative_amount() {
        assert!(Money::new(Decimal::new(-1, 0), "USD").is_err());
    }

    #[test]
    fn test_money_new_invalid_currency() {
        assert!(Money::new(Decimal::ZERO, "US").is_err());
        assert!(Money::new(Decimal::ZERO, "USDD").is_err());
    }

    #[test]
    fn test_money_add_same_currency() {
        let a = Money::usd(Decimal::new(500, 2)).expect("valid usd amount"); // 5.00
        let b = Money::usd(Decimal::new(300, 2)).expect("valid usd amount"); // 3.00
        let result = (a + b).expect("same currency addition should work");
        assert_eq!(result.amount, Decimal::new(800, 2)); // 8.00
        assert_eq!(result.currency, "USD");
    }

    #[test]
    fn test_money_add_different_currencies_returns_error() {
        let a = Money::usd(Decimal::ONE).expect("valid usd amount");
        let b = Money::new(Decimal::ONE, "EUR").expect("valid eur amount");
        let result = a + b;
        assert!(matches!(result, Err(MoneyError::CurrencyMismatch { .. })));
    }

    #[test]
    fn test_money_zero() {
        let z = Money::zero("usd").expect("valid zero money");
        assert!(z.is_zero());
        assert_eq!(z.currency, "USD");
    }

    #[test]
    fn test_money_sub_insufficient_funds_returns_error() {
        let a = Money::usd(Decimal::new(100, 2)).expect("valid usd amount"); // 1.00
        let b = Money::usd(Decimal::new(200, 2)).expect("valid usd amount"); // 2.00
        let result = a - b;
        assert!(matches!(result, Err(MoneyError::InsufficientFunds { .. })));
    }
}
