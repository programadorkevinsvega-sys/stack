fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_prost_build::configure()
        .build_server(true)
        .build_client(true)
        .type_attribute(".", "#[derive(serde::Serialize, serde::Deserialize)]")
        .compile_protos(
            &[
                "proto/key_validation.proto",
                "proto/account_selection.proto",
                "proto/credential_resolution.proto",
                "proto/billing_reservation.proto",
                "proto/plan_resolution.proto",
                "proto/payment.proto",
                "proto/catalog.proto",
                "proto/auth.proto",
                "proto/usage.proto",
                "proto/invoice.proto",
                "proto/file.proto",
            ],
            &["proto/"],
        )?;
    Ok(())
}
