
// NB: woefully incomplete, add here as needed

pub const EXIF_IFD_POINTER: u16 = 0x8769;
pub const GPS_INFO_IFD_POINTER: u16 = 0x8825;
pub const INTEROPERABILITY_IFD_POINTER: u16 = 0xa005;
pub const JPEG_THUMBNAIL_LENGTH: u16 = 0x0202;
pub const JPEG_THUMBNAIL_OFFSET: u16 = 0x0201;

pub const IMG_DESCRIPTION: u16 = 0x010e;
pub const ORIENTATION: u16 = 0x0112;

pub const GPS_INFO: u16 = 0x8825;
pub const MODIFY_DATE: u16 = 0x0132;

pub mod gps {
    pub const LATITUDE_REF: u16 = 0x0001;
    pub const LATITUDE: u16 = 0x0002;
    pub const LONGITUDE_REF: u16 = 0x0003;
    pub const LONGITUDE: u16 = 0x0004;
    pub const ALTITUDE_REF: u16 = 0x0005;
    pub const ALTITUDE: u16 = 0x0006;
}
