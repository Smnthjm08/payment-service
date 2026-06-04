#[derive(Debug, Clone)]
pub enum InvoiceState {
    Draft,
    Open,
    Processing,
    Paid,
    Void,
    Uncollectible,
}

#[derive(Debug, Clone)]
pub enum PaymentAttemptStatus {
    Pending,
    Succeeded,
    Failed,
    TimedOut,
    Error,
}
