// http://vip.sugovica.hu/Sardi/kepnezo/JPEG%20File%20Layout%20and%20Format.htm
// https://www.imperialviolet.org/binary/jpeg/
// http://dev.exiv2.org/projects/exiv2/wiki/The_Metadata_in_JPEG_files

use std::io::prelude::*;
use std::io;
use std::io::SeekFrom;

use byteorder::{ReadBytesExt, BigEndian};

/// helper to extract an exif segment from a jpeg file
pub fn extract_exif<R: Read + Seek>(rdr: &mut R) -> io::Result<Vec<u8>> {
    loop {
        // find next segment marker
        if 0xFF != try!(rdr.read_u8()) {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "couldn't find segment marker"));
        }

        // is this a segment we care about?
        let seg_id = try!(rdr.read_u8());
        match seg_id {
            0x0 => {} // byte stuffing, do nothing
            0xD8 => {} // image start, do nothing
            0xE1 => {  // APP1, may be EXIF
                let len = (try!(rdr.read_u16::<BigEndian>()) - 2) as usize;

                const HDR_SZ: usize = 6;
                let mut hdr = vec![0u8; HDR_SZ];
                try!(rdr.read_exact(&mut hdr));

                if &hdr == &[b'E', b'x', b'i', b'f', 0x00, 0x00] {
                    let mut segment = vec![0u8; len - HDR_SZ];
                    try!(rdr.read_exact(&mut segment));
                    return Ok(segment);
                }
                try!(rdr.seek(SeekFrom::Current((len - HDR_SZ) as i64)));
            }
            _ => {
                // skip segment
                let len = try!(rdr.read_u16::<BigEndian>()) - 2;
                try!(rdr.seek(SeekFrom::Current(len as i64)));
            }
        }
    }
}
