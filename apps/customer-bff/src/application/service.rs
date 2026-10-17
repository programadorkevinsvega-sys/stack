use crate::infrastructure::clients::{
    AuthGrpcClient, CatalogGrpcClient, FileGrpcClient, InvoiceGrpcClient, KeyGrpcClient,
    PaymentGrpcClient, PlanGrpcClient, UsageGrpcClient, WalletGrpcClient,
};

pub struct BffApplication {
    pub auth: AuthGrpcClient,
    pub keys: KeyGrpcClient,
    pub plans: PlanGrpcClient,
    pub catalog: CatalogGrpcClient,
    pub wallet: WalletGrpcClient,
    pub invoices: InvoiceGrpcClient,
    pub payments: PaymentGrpcClient,
    pub usage: UsageGrpcClient,
    pub files: FileGrpcClient,
    pub cookie_secure: bool,
    pub cookie_domain: Option<String>,
}
