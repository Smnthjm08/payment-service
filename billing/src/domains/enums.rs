#[derive(Debug, Clone)]
pub enum InvoiceState {
    Draft,
    Open,
    Processing,
    Paid,
    Void,
    Uncollectible,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaymentAttemptStatus {
    Pending,
    Succeeded,
    Failed,
    TimedOut,
    Error,
}

impl sqlx::Type<sqlx::Postgres> for PaymentAttemptStatus {
    fn type_info() -> sqlx::postgres::PgTypeInfo {
        <&str as sqlx::Type<sqlx::Postgres>>::type_info()
    }
}

impl<'r> sqlx::Decode<'r, sqlx::Postgres> for PaymentAttemptStatus {
    fn decode(
        value: sqlx::postgres::PgValueRef<'r>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let text = <&str as sqlx::Decode<sqlx::Postgres>>::decode(value)?;
        match text {
            "Pending"   => Ok(Self::Pending),
            "Succeeded" => Ok(Self::Succeeded),
            "Failed"    => Ok(Self::Failed),
            "TimedOut"  => Ok(Self::TimedOut),
            "Error"     => Ok(Self::Error),
            other => Err(format!("Unknown PaymentAttemptStatus: {other}").into()),
        }
    }
}

impl sqlx::Encode<'_, sqlx::Postgres> for PaymentAttemptStatus {
    fn encode_by_ref(
        &self,
        buf: &mut sqlx::postgres::PgArgumentBuffer,
    ) -> Result<sqlx::encode::IsNull, Box<dyn std::error::Error + Send + Sync>> {
        let s: &str = match self {
            Self::Pending   => "Pending",
            Self::Succeeded => "Succeeded",
            Self::Failed    => "Failed",
            Self::TimedOut  => "TimedOut",
            Self::Error     => "Error",
        };
        <&str as sqlx::Encode<sqlx::Postgres>>::encode_by_ref(&s, buf)
    }
}
