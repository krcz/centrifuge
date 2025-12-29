use polyepoxide_core::{oxide, Bond, ByteString};

/// EXIF value types as defined in the EXIF standard
#[oxide]
pub enum ExifValue {
    Byte(u8),
    Ascii(String),
    Short(u16),
    Long(u32),
    Rational { num: u32, denom: u32 },
    Undefined(Bond<ByteString>),
    SLong(i32),
    SRational { num: i32, denom: i32 },
}

/// A single EXIF tag with its values
#[oxide]
pub struct ExifTag {
    pub id: u16,
    pub values: Vec<ExifValue>,
}

/// EXIF metadata extracted from photos
#[oxide]
pub struct ExifData {
    /// All EXIF tags with typed values
    pub tags: Vec<ExifTag>,

    // Common fields for convenience (may duplicate data in tags)
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub date_taken: Option<String>, // ISO 8601 format
    pub gps_latitude: Option<f64>,
    pub gps_longitude: Option<f64>,
}

/// Photo with full metadata and inline content
#[oxide]
pub struct Photo {
    pub filename: String,
    pub mime_type: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub exif: Option<Bond<ExifData>>,
    pub thumbnails: Vec<Bond<Photo>>,
    pub content: Bond<ByteString>,
}
