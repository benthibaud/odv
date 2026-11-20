use crate::ast::{CURRENT_SCHEMA_VERSION, DocumentAst};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::{Cursor, Read, Write};
use std::path::Path;
use thiserror::Error;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

#[derive(Debug, Error)]
pub enum FormatError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("ZIP archive error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Missing required archive entry: {0}")]
    MissingEntry(&'static str),
    #[error("Unsupported schema version {found} (max supported: {max_supported})")]
    UnsupportedSchemaVersion { found: u32, max_supported: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OdvMetadata {
    pub schema_version: u32,
    pub title: String,
    pub language: String,
    pub created_with: String,
}

/// Complete in-memory representation of a `.odv` ZIP container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OdvPackage {
    pub meta: OdvMetadata,
    pub document: DocumentAst,
    pub styles: BTreeMap<String, String>,
    pub medias: BTreeMap<String, Vec<u8>>,
}

impl OdvPackage {
    pub fn new(document: DocumentAst) -> Self {
        let meta = OdvMetadata {
            schema_version: CURRENT_SCHEMA_VERSION,
            title: document.title.clone(),
            language: document.language.clone(),
            created_with: concat!("odv-core/", env!("CARGO_PKG_VERSION")).to_string(),
        };
        Self {
            meta,
            document,
            styles: BTreeMap::new(),
            medias: BTreeMap::new(),
        }
    }

    pub fn write_to_bytes(&self) -> Result<Vec<u8>, FormatError> {
        let mut cursor = Cursor::new(Vec::new());
        {
            let mut zip = ZipWriter::new(&mut cursor);
            let options =
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

            zip.start_file("meta.json", options)?;
            let meta_json = serde_json::to_vec_pretty(&self.meta)?;
            zip.write_all(&meta_json)?;

            zip.start_file("document.json", options)?;
            let doc_json = serde_json::to_vec_pretty(&self.document)?;
            zip.write_all(&doc_json)?;

            for (name, css) in &self.styles {
                let entry_name = format!("styles/{name}");
                zip.start_file(entry_name, options)?;
                zip.write_all(css.as_bytes())?;
            }

            for (name, bytes) in &self.medias {
                let entry_name = format!("medias/{name}");
                zip.start_file(entry_name, options)?;
                zip.write_all(bytes)?;
            }

            zip.finish()?;
        }
        Ok(cursor.into_inner())
    }

    pub fn read_from_bytes(bytes: &[u8]) -> Result<Self, FormatError> {
        let cursor = Cursor::new(bytes);
        let mut archive = ZipArchive::new(cursor)?;

        let meta: OdvMetadata = {
            let mut file = archive
                .by_name("meta.json")
                .map_err(|_| FormatError::MissingEntry("meta.json"))?;
            let mut buf = Vec::new();
            file.read_to_end(&mut buf)?;
            serde_json::from_slice(&buf)?
        };

        if meta.schema_version > CURRENT_SCHEMA_VERSION {
            return Err(FormatError::UnsupportedSchemaVersion {
                found: meta.schema_version,
                max_supported: CURRENT_SCHEMA_VERSION,
            });
        }

        let document: DocumentAst = {
            let mut file = archive
                .by_name("document.json")
                .map_err(|_| FormatError::MissingEntry("document.json"))?;
            let mut buf = Vec::new();
            file.read_to_end(&mut buf)?;
            serde_json::from_slice(&buf)?
        };

        if document.schema_version > CURRENT_SCHEMA_VERSION {
            return Err(FormatError::UnsupportedSchemaVersion {
                found: document.schema_version,
                max_supported: CURRENT_SCHEMA_VERSION,
            });
        }

        let mut styles = BTreeMap::new();
        let mut medias = BTreeMap::new();

        for idx in 0..archive.len() {
            let mut entry = archive.by_index(idx)?;
            if entry.is_dir() {
                continue;
            }
            let name = entry.name().to_string();
            if let Some(style_name) = name.strip_prefix("styles/") {
                let mut css = String::new();
                entry.read_to_string(&mut css)?;
                styles.insert(style_name.to_string(), css);
            } else if let Some(media_name) = name.strip_prefix("medias/") {
                let mut data = Vec::new();
                entry.read_to_end(&mut data)?;
                medias.insert(media_name.to_string(), data);
            }
        }

        Ok(Self {
            meta,
            document,
            styles,
            medias,
        })
    }

    pub fn save_to_file(&self, path: &Path) -> Result<(), FormatError> {
        let bytes = self.write_to_bytes()?;
        std::fs::write(path, bytes)?;
        Ok(())
    }

    pub fn load_from_file(path: &Path) -> Result<Self, FormatError> {
        let bytes = std::fs::read(path)?;
        Self::read_from_bytes(&bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Visibility;

    #[test]
    fn contract_odv_zip_roundtrip_and_schema_version_check() {
        let mut doc = DocumentAst::new("Archive Contract", "fr");
        doc.push_heading(1, "Introduction", Some("Slide bullet".into()));
        doc.push_paragraph("Explanatory text.", Visibility::All);

        let mut pkg = OdvPackage::new(doc);
        pkg.styles
            .insert("tokens.css".into(), ":root { --scale: 1.25; }".into());
        pkg.medias.insert("figure.avif".into(), vec![0, 1, 2, 3]);

        let bytes = pkg.write_to_bytes().expect("ZIP serialization must succeed");
        let restored = OdvPackage::read_from_bytes(&bytes).expect("ZIP deserialization must succeed");

        assert_eq!(restored.meta.schema_version, CURRENT_SCHEMA_VERSION);
        assert_eq!(restored.document, pkg.document);
        assert_eq!(restored.styles, pkg.styles);
        assert_eq!(restored.medias, pkg.medias);

        // Reject future schema versions cleanly
        let mut future_pkg = pkg;
        future_pkg.meta.schema_version = CURRENT_SCHEMA_VERSION + 99;
        let future_bytes = future_pkg.write_to_bytes().unwrap();
        let err = OdvPackage::read_from_bytes(&future_bytes).unwrap_err();
        assert!(matches!(
            err,
            FormatError::UnsupportedSchemaVersion {
                found: 100,
                max_supported: 1
            }
        ));
    }
}
