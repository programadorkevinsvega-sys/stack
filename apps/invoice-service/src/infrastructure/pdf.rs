use crate::domain::entities::{Invoice, InvoiceDocument};
use crate::domain::errors::InvoiceError;
use async_trait::async_trait;
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::Client as S3Client;
use rust_decimal::Decimal;

#[async_trait]
pub trait PdfGenerator: Send + Sync {
    async fn generate_invoice_pdf(
        &self,
        document: &InvoiceDocument,
    ) -> Result<Vec<u8>, InvoiceError>;
}

#[async_trait]
pub trait PdfStorage: Send + Sync {
    async fn store_invoice_pdf(
        &self,
        invoice: &Invoice,
        bytes: Vec<u8>,
    ) -> Result<String, InvoiceError>;

    async fn retrieve_invoice_pdf(&self, pdf_url: &str) -> Result<Vec<u8>, InvoiceError>;
}

#[derive(Clone, Default)]
pub struct MinimalPdfGenerator;

fn decimal_str(d: Decimal) -> String {
    format!("{:.2}", d.round_dp(2))
}

#[async_trait]
impl PdfGenerator for MinimalPdfGenerator {
    async fn generate_invoice_pdf(
        &self,
        document: &InvoiceDocument,
    ) -> Result<Vec<u8>, InvoiceError> {
        let content = build_pdf_content(document);
        let content_bytes = content.as_bytes();
        let stream_len = content_bytes.len();

        let objects = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>".to_string(),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
            format!("<< /Length {stream_len} >>\nstream\n{content}\nendstream"),
        ];

        let mut pdf = b"%PDF-1.4\n%\xAA\xBB\xCC\xDD\n".to_vec();
        let mut offsets = Vec::with_capacity(objects.len());
        for (index, object) in objects.iter().enumerate() {
            offsets.push(pdf.len());
            pdf.extend_from_slice(format!("{} 0 obj\n{}\nendobj\n", index + 1, object).as_bytes());
        }
        let xref_offset = pdf.len();
        pdf.extend_from_slice(
            format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
        );
        for offset in offsets {
            pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        pdf.extend_from_slice(
            format!(
                "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
                objects.len() + 1,
                xref_offset
            )
            .as_bytes(),
        );
        Ok(pdf)
    }
}

fn build_pdf_content(document: &InvoiceDocument) -> String {
    let inv = &document.invoice;
    let y_start = 750i32;
    let line_height = 14i32;
    let left_margin = 50i32;

    let mut y = y_start;
    let mut ops = String::new();

    let put = |y: &mut i32, ops: &mut String, text: &str, size: i32| {
        ops.push_str(&format!(
            "BT\n/F1 {size} Tf\n{left_margin} {y} Td\n({}) Tj\nET\n",
            escape_pdf_text(text)
        ));
        *y -= line_height * 2;
    };

    put(
        &mut y,
        &mut ops,
        &format!("AI Gateway — Invoice #{}", inv.invoice_number),
        14,
    );
    put(&mut y, &mut ops, &format!("User: {}", inv.user_id), 10);
    put(
        &mut y,
        &mut ops,
        &format!(
            "Period: {} — {}",
            inv.period_start.format("%Y-%m-%d"),
            inv.period_end.format("%Y-%m-%d")
        ),
        10,
    );
    let due = inv
        .due_date
        .map_or("N/A".to_string(), |d| d.format("%Y-%m-%d").to_string());
    put(
        &mut y,
        &mut ops,
        &format!("Status: {}   Due: {}", inv.status, due),
        10,
    );
    put(&mut y, &mut ops, "Line items:", 10);

    for item in &document.line_items {
        put(
            &mut y,
            &mut ops,
            &format!(
                "{}  |  qty {}  |  unit {}  |  amount {}",
                item.description,
                item.quantity,
                decimal_str(item.unit_price),
                decimal_str(item.amount)
            ),
            10,
        );
    }

    put(
        &mut y,
        &mut ops,
        &format!("Subtotal: {} {}", decimal_str(inv.subtotal), inv.currency),
        10,
    );
    put(
        &mut y,
        &mut ops,
        &format!("Tax: {} {}", decimal_str(inv.tax), inv.currency),
        10,
    );
    put(
        &mut y,
        &mut ops,
        &format!("Total: {} {}", decimal_str(inv.total), inv.currency),
        10,
    );

    ops
}

fn escape_pdf_text(value: &str) -> String {
    value
        .chars()
        .flat_map(|ch| match ch {
            '(' => vec!['\\', '('],
            ')' => vec!['\\', ')'],
            '\\' => vec!['\\', '\\'],
            _ => {
                let mut buf = [0u8; 4];
                let encoded = ch.encode_utf8(&mut buf);
                encoded.chars().collect::<Vec<_>>()
            }
        })
        .collect()
}

#[derive(Clone)]
pub struct R2PdfStorage {
    pub client: S3Client,
    pub bucket: String,
    pub prefix: String,
}

#[async_trait]
impl PdfStorage for R2PdfStorage {
    async fn store_invoice_pdf(
        &self,
        invoice: &Invoice,
        bytes: Vec<u8>,
    ) -> Result<String, InvoiceError> {
        let key = format!(
            "{}/user_id={}/{}.pdf",
            self.prefix.trim_matches('/'),
            invoice.user_id,
            invoice.invoice_number
        );
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(&key)
            .content_type("application/pdf")
            .body(ByteStream::from(bytes))
            .send()
            .await
            .map_err(|e| InvoiceError::PdfStorageFailed(e.to_string()))?;
        Ok(format!("r2://{}/{}", self.bucket, key))
    }

    async fn retrieve_invoice_pdf(&self, pdf_url: &str) -> Result<Vec<u8>, InvoiceError> {
        let key = pdf_url
            .strip_prefix(&format!("r2://{}/", self.bucket))
            .ok_or_else(|| InvoiceError::PdfStorageFailed("pdf_url bucket mismatch".into()))?;
        let output = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| InvoiceError::PdfStorageFailed(e.to_string()))?;
        let bytes = output
            .body
            .collect()
            .await
            .map_err(|e| InvoiceError::PdfStorageFailed(e.to_string()))?
            .into_bytes()
            .to_vec();
        Ok(bytes)
    }
}
