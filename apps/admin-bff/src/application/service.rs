use crate::infrastructure::clients::{
    AdminAuthGrpcClient, AdminCatalogGrpcClient, AdminCredentialGrpcClient, AdminFileGrpcClient,
    AdminInvoiceGrpcClient, AdminJetStreamClient, AdminPlanGrpcClient, AdminRouterGrpcClient,
    AdminUsageGrpcClient, AdminWalletGrpcClient,
};

pub struct BffApplication {
    pub auth: AdminAuthGrpcClient,
    pub catalog: AdminCatalogGrpcClient,
    pub credentials: AdminCredentialGrpcClient,
    pub plans: AdminPlanGrpcClient,
    pub wallet: AdminWalletGrpcClient,
    pub invoices: AdminInvoiceGrpcClient,
    pub files: AdminFileGrpcClient,
    pub usage: AdminUsageGrpcClient,
    pub router: AdminRouterGrpcClient,
    pub jetstream: AdminJetStreamClient,
    pub cookie_secure: bool,
    pub cookie_domain: Option<String>,
}
