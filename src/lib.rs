extern crate byteorder;

pub mod tag;
pub mod jpeg;

use std::io::prelude::*;
use std::io;
use std::io::SeekFrom;
use byteorder::{ReadBytesExt, WriteBytesExt, BigEndian, LittleEndian, ByteOrder};

/// top level data structure representing an entire exif document
#[derive(Clone, Debug)]
pub struct Exif {
    pub ifds: Vec<Ifd>,
}

// why doesn't io::Cursor implement any of these for anything other than [u8]? :(
struct PosWriter<T> {
    inner: T,
    pos: u64,
}

impl<T: Write + Seek> PosWriter<T> {
    fn new(inner: T) -> Self {
        PosWriter{ inner: inner, pos: 0 }
    }
    fn position(&self) -> u64 { self.pos }
}

impl<T: Write> Write for PosWriter<T> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = try!(self.inner.write(buf));
        self.pos += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

impl<T: Seek> Seek for PosWriter<T> {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let n = try!(self.inner.seek(pos));
        self.pos = n;
        Ok(n)
    }
}

impl Exif {
    /// extract Exif from the given reader
    pub fn new<R: Read + Seek>(rdr: &mut R) -> io::Result<Self> {
        let mut header = vec![0; 8];
        try!(rdr.read_exact(&mut header));

        let big_endian = match (header[0], header[1]) {
            (b'M', b'M') => true,
            (b'I', b'I') => false,
            _ => return Err(io::Error::new(io::ErrorKind::InvalidData, "invalid endianness marker")),
        };

        let offset_to_ifd = if big_endian {
            BigEndian::read_u32(&header[4..])
        } else {
            LittleEndian::read_u32(&header[4..])
        } as usize;

        try!(rdr.seek(SeekFrom::Start(offset_to_ifd as u64)));

        let mut ifds = vec![];
        for i in 0.. {
            let (ifd, offset) = {
                if big_endian { try!(Ifd::new::<_, BigEndian>(rdr, i)) }
                else          { try!(Ifd::new::<_, LittleEndian>(rdr, i)) }
            };
            ifds.push(ifd);
            if offset == 0 { break; } // last IFD
            try!(rdr.seek(SeekFrom::Start(offset as u64)));
        }

        Ok(Exif{ ifds: ifds })
    }

    /// Write an existing Exif to the given writer
    pub fn encode<W: Write + Seek>(&self, w: &mut W) -> io::Result<()> {
        const HEADER: [u8; 8] = [b'I', b'I', 0x2A, 0x00, 0x08, 0x00, 0x00, 0x00];

        let mut pw = PosWriter::new(w);
        try!(pw.write_all(&HEADER));

        for (n, ifd) in self.ifds.iter().enumerate() {
            try!(ifd.encode::<_, LittleEndian>(&mut pw, n == self.ifds.len() - 1));
        }

        Ok(())
    }
}

/// Image file directory - container for a collection of Entries
#[derive(Clone, Debug)]
pub struct Ifd {
    pub id: u16,
    pub entries: Vec<Entry>,
    pub children: Vec<Ifd>,
}

impl Ifd {
    fn new<R: Read + Seek, B: ByteOrder>(rdr: &mut R, id: u16) -> io::Result<(Self, usize)> {
        let num_headers = try!(rdr.read_u16::<B>());

        let mut children = vec![];
        let mut entries = vec![];

        // headers are continguous, followed by offset_to_next_ifd and entry data
        let mut hdrs = vec![];
        for _ in 0..num_headers {
            hdrs.push(try!(EntryHeader::decode::<_, B>(rdr)));
        }

        let offset_to_next_ifd = try!(rdr.read_u32::<B>()) as usize;

        // XXX: plumb this back up to the Exif struct
        // let mut thumbnail = JpegThumbnail::new();

        for h in &hdrs {
            match h.tag {
                // follow known pointers to generate SubIFDs
                tag::EXIF_IFD_POINTER | tag::GPS_INFO_IFD_POINTER | tag::INTEROPERABILITY_IFD_POINTER => {
                    if let OffsetValue::Value(ref v) = h.offset_val {
                        let off = B::read_u32(v);
                        try!(rdr.seek(SeekFrom::Start(off as u64)));
                        let (ifd, _) = try!(Ifd::new::<_, B>(rdr, h.tag));
                        children.push(ifd);
                    }
                    // XXX: provide invalid format feedback
                },
                /*
                // only handle jpeg thumbnails at the moment
                tag::JPEG_THUMBNAIL_LENGTH => {
                    if let OffsetValue::Value(ref v) = h.offset_val {
                        try!(thumbnail.set_length(rdr, B::read_u32(v) as usize));
                    }
                },
                tag::JPEG_THUMBNAIL_OFFSET => {
                    if let OffsetValue::Value(ref v) = h.offset_val {
                        try!(thumbnail.set_offset(rdr, B::read_u32(v) as usize));
                    }
                },
                */
                _ => {
                    let e = try!(Entry::from_header::<_, B>(rdr, h));
                    entries.push(e);
                },
            }
        }

        Ok((Ifd{
            id: id,
            entries: entries,
            children: children,
        }, offset_to_next_ifd))
    }

    /// similar to Entry::encode_header(), but the offset to the subIFD
    /// is encoded in the ULong immediate
    fn encode_subifd_header<W: Write, B: ByteOrder>(&self, pw: &mut PosWriter<W>, data_offset: usize) -> io::Result<()> {
        try!(pw.write_u16::<B>(self.id));
        try!(pw.write_u16::<B>(4));  // ULong
        try!(pw.write_u32::<B>(1));
        pw.write_u32::<B>(data_offset as u32)
    }

    fn encode<W: Write + Seek, B: ByteOrder>(&self, pw: &mut PosWriter<W>, last: bool) -> io::Result<()> {
        const IFD_HEADER_LEN: usize = 12;
        const NEXT_IFD_PTR_LEN: usize = 4;

        let num_headers = self.entries.len() + self.children.len();
        try!(pw.write_u16::<B>(num_headers as u16));

        // data segment for this IFD starts here
        let mut data_offset = pw.position() as usize + num_headers * IFD_HEADER_LEN + NEXT_IFD_PTR_LEN;

        // track which EntryData instances must be written to this IFD's data segment
        let mut offset_data_entries = vec![];

        // write headers for both entries and children...
        for e in &self.entries {
            if try!(e.encode_header::<_, B>(pw, data_offset)) {
                data_offset += e.data.total_sz();
                offset_data_entries.push(e);
            }
        }

        for c in &self.children {
            try!(c.encode_subifd_header::<_, B>(pw, data_offset));
        }

        // write offset to next IFD or 0 if this is the last
        // we'll need to seek back here once we know the size of the written data segment and any subIFDs
        let next_offset_pos = pw.position();
        try!(pw.write_u32::<B>(0u32)); // placeholder

        // ...followed by data for both entries and children
        for e in offset_data_entries {
            try!(e.data.encode_offset_data::<_, B>(pw));
        }

        for c in &self.children {
            // at the moment, we do not descend into child hierarchies
            try!(c.encode::<_, B>(pw, true));
        }

        // seek back to next_offset location and write it, now that we know it
        if !last {
            let cur_pos = pw.position();
            try!(pw.seek(SeekFrom::Start(next_offset_pos)));
            try!(pw.write_u32::<B>(cur_pos as u32));
            try!(pw.seek(SeekFrom::Start(cur_pos)));
        }

        Ok(())
    }
}

/*
#[derive(Debug)]
pub struct JpegThumbnail {
    offset: usize,
    length: usize,
    pub data: Option<Vec<u8>>,
}

impl JpegThumbnail {
    fn new() -> Self {
        JpegThumbnail{ offset: 0, length: 0, data: None }
    }

    fn set_length<R: Read + Seek>(&mut self, rdr: &mut R, length: usize) -> io::Result<()> {
        self.length = length;
        self.extract_data(rdr)
    }

    fn set_offset<R: Read + Seek>(&mut self, rdr: &mut R, offset: usize) -> io::Result<()> {
        self.offset = offset;
        self.extract_data(rdr)
    }

    fn extract_data<R: Read + Seek>(&mut self, rdr: &mut R, ) -> io::Result<()> {
        if self.length != 0 && self.offset != 0 {
            let mut buf = vec![0u8; self.length as usize];
            try!(rdr.seek(SeekFrom::Start(self.offset as u64)));
            try!(rdr.read_exact(&mut buf));
            self.data = Some(buf);
        }
        Ok(())
    }
}
*/

#[derive(Debug)]
enum OffsetValue {
    Offset(u32),
    Value(Vec<u8>),
}

#[derive(Debug)]
struct EntryHeader {
    tag: u16,
    format: u16,
    count: u32,
    offset_val: OffsetValue,
}

impl EntryHeader {
    fn decode<R: Read, B: ByteOrder>(rdr: &mut R) -> io::Result<Self> {
        let tag = try!(rdr.read_u16::<B>());
        let fmt = try!(rdr.read_u16::<B>());
        let n = try!(rdr.read_u32::<B>());

        // if all the data fits into 4 bytes, expect an OffsetValue::Value encoded immediately,
        // otherwise expect an OffsetValue::Offset to the data
        let ov = if Self::datatype_sz(fmt) * n as usize <= 4 {
            let mut buf = vec![0u8; 4];
            try!(rdr.read_exact(&mut buf));
            OffsetValue::Value(buf)
        } else {
            OffsetValue::Offset(try!(rdr.read_u32::<B>()))
        };

        Ok(EntryHeader{
            tag: tag,
            format: fmt,
            count: n,
            offset_val: ov,
        })
    }

    fn datatype_sz(dt: u16) -> usize {
        match dt {
            1 /*Byte*/ | 2 /*Ascii*/ | 6 /*SignedByte*/ | 7 /*Undef*/ => 1,
            3 /*UShort*/ | 8 /*SShort*/ => 2,
            4 /*ULong*/ | 9 /*SLong*/ | 11 /*Float32*/ => 4,
            5 /*URational*/ | 10 /*SRational*/ | 12 /*Float64*/ => 8,
            _ => 0,
        }
    }

    fn sz(&self) -> usize {
        Self::datatype_sz(self.format)
    }

    fn data_sz(&self) -> usize {
        self.sz() * self.count as usize
    }
}

/// individual entry within an IFD
#[derive(Clone, Debug)]
pub struct Entry {
    pub tag: u16,
    pub data: EntryData,
}

impl Entry {
    fn from_header<R: Read + Seek, B: ByteOrder>(rdr: &mut R, h: &EntryHeader) -> io::Result<Self> {
        Ok(Entry{
            tag: h.tag,
            data: try!(EntryData::from_header::<_, B>(rdr, h)),
        })
    }

    /// encode the header portion of this enty,
    /// return true if data must subsequently be written into the data segment
    fn encode_header<W: Write, B: ByteOrder>(&self, w: &mut W, data_offset: usize) -> io::Result<bool> {
        try!(w.write_u16::<B>(self.tag));
        try!(w.write_u16::<B>(self.data.format_code()));
        try!(w.write_u32::<B>(self.data.len() as u32));

        if self.data.total_sz() > 4 {
            try!(w.write_u32::<B>(data_offset as u32));
            Ok(true)
        } else {
            try!(self.data.encode_offset_data::<_, B>(w));
            // immediate data must be padded to 4 bytes
            let pad = 4isize - self.data.total_sz() as isize;
            if pad > 0 {
                let pbuf = [0u8; 4];
                try!(w.write_all(&mut &pbuf[..pad as usize]));
            }
            Ok(false)
        }
    }
}

/// Data associated with an Entry
#[derive(Clone, Debug)]
pub enum EntryData {
    Byte(Vec<u8>),
    Ascii(String),
    UShort(Vec<u16>),
    ULong(Vec<u32>),
    URational(Vec<u64>),
    SignedByte(Vec<i8>),
    Undef(Vec<u8>),  // or vendor specific
    SShort(Vec<i16>),
    SLong(Vec<i32>),
    SRational(Vec<i64>),
    Float32(Vec<f32>),
    Float64(Vec<f64>),
}

impl EntryData {
    fn item_sz(&self) -> usize {
        use self::EntryData::*;
        match *self {
            Byte(_) | Ascii(_) | SignedByte(_) | Undef(_) => 1,
            UShort(_) | SShort(_) => 2,
            ULong(_) | SLong(_) | Float32(_) => 4,
            URational(_) | SRational(_) | Float64(_) => 8,
        }
    }

    fn len(&self) -> usize {
        use self::EntryData::*;
        // better way to implement this?
        match *self {
            Byte(ref v) => v.len(),
            Ascii(ref v) => v.len() + 1,
            UShort(ref v) => v.len(),
            ULong(ref v) => v.len(),
            URational(ref v) => v.len(),
            SignedByte(ref v) => v.len(),
            Undef(ref v) => v.len(),
            SShort(ref v) => v.len(),
            SLong(ref v) => v.len(),
            SRational(ref v) => v.len(),
            Float32(ref v) => v.len(),
            Float64(ref v) => v.len(),
        }
    }

    fn total_sz(&self) -> usize {
        self.item_sz() * self.len()
    }

    fn format_code(&self) -> u16 {
        use self::EntryData::*;
        match *self {
            Byte(_) => 1,
            Ascii(_) => 2,
            UShort(_) => 3,
            ULong(_) => 4,
            URational(_) => 5,
            SignedByte(_) => 6,
            Undef(_) => 7,
            SShort(_) => 8,
            SLong(_) => 9,
            SRational(_) => 10,
            Float32(_) => 11,
            Float64(_) => 12,
        }
    }

    fn from_header<R: Read + Seek, B: ByteOrder>(rdr: &mut R, h: &EntryHeader) -> io::Result<Self> {
        let d = match h.offset_val {
            OffsetValue::Value(ref v) => v.to_owned(),
            OffsetValue::Offset(o) => {
                let mut v = vec![0u8; h.data_sz()];
                try!(rdr.seek(SeekFrom::Start(o as u64)));
                try!(rdr.read_exact(&mut v));
                v
            }
        };

        match h.format {
            1 => Ok(EntryData::Byte(d)),
            2 => {
                let null_term = try!(d.iter().position(|&c| c == 0)
                                        .ok_or(io::Error::new(io::ErrorKind::InvalidData, "invalid ascii data, no null terminator")));
                Ok(EntryData::Ascii(String::from_utf8_lossy(&d[..null_term]).to_string()))
            },
            3 => {
                let mut v = Vec::with_capacity(h.count as usize);
                let mut c = io::Cursor::new(d);
                for _ in 0..h.count { v.push(try!(c.read_u16::<B>())); }
                Ok(EntryData::UShort(v))
            },
            4 => {
                let mut v = Vec::with_capacity(h.count as usize);
                let mut c = io::Cursor::new(d);
                for _ in 0..h.count { v.push(try!(c.read_u32::<B>())); }
                Ok(EntryData::ULong(v))
            },
            5 => {
                let mut v = Vec::with_capacity(h.count as usize);
                let mut c = io::Cursor::new(d);
                for _ in 0..h.count { v.push(try!(c.read_u64::<B>())); }
                Ok(EntryData::URational(v))
            },
            // XXX: better way to convert?
            6 => Ok(EntryData::SignedByte(d.iter().map(|&b| b as i8).collect())),
            7 => Ok(EntryData::Undef(d)),
            8 => {
                let mut v = Vec::with_capacity(h.count as usize);
                let mut c = io::Cursor::new(d);
                for _ in 0..h.count { v.push(try!(c.read_u16::<B>()) as i16); }
                Ok(EntryData::SShort(v))
            },
            9 => {
                let mut v = Vec::with_capacity(h.count as usize);
                let mut c = io::Cursor::new(d);
                for _ in 0..h.count { v.push(try!(c.read_u32::<B>()) as i32); }
                Ok(EntryData::SLong(v))
            },
            10 => {
                let mut v = Vec::with_capacity(h.count as usize);
                let mut c = io::Cursor::new(d);
                for _ in 0..h.count { v.push(try!(c.read_i64::<B>())); }
                Ok(EntryData::SRational(v))
            },
            11 => {
                let mut v = Vec::with_capacity(h.count as usize);
                let mut c = io::Cursor::new(d);
                for _ in 0..h.count { v.push(try!(c.read_f32::<B>())); }
                Ok(EntryData::Float32(v))
            },
            12 => {
                let mut v = Vec::with_capacity(h.count as usize);
                let mut c = io::Cursor::new(d);
                for _ in 0..h.count { v.push(try!(c.read_f64::<B>())); }
                Ok(EntryData::Float64(v))
            },
            v => Err(io::Error::new(io::ErrorKind::InvalidData, format!("invalid entry header format: 0x{:x}", v)))
        }
    }

    // header portion has already been written, just write data segment
    fn encode_offset_data<W: Write, B: ByteOrder>(&self, w: &mut W) -> io::Result<()> {

        use self::EntryData::*;
        match *self {
            Byte(ref v) | Undef(ref v) => try!(w.write_all(v)),
            Ascii(ref v) => {
                try!(w.write_all(v.as_bytes()));
                try!(w.write_all(b"\0"));
            },
            UShort(ref v) =>    for d in v { try!(w.write_u16::<B>(*d)); },
            ULong(ref v) =>     for d in v { try!(w.write_u32::<B>(*d)); },
            URational(ref v) => for d in v { try!(w.write_u64::<B>(*d)); },
            SignedByte(ref v) => {
                // XXX: better way to convert?
                let vb = v.iter().map(|&b| b as u8).collect::<Vec<u8>>();
                try!(w.write_all(&vb))
            },
            SShort(ref v) =>    for d in v { try!(w.write_u16::<B>(*d as u16)); },
            SLong(ref v) =>     for d in v { try!(w.write_u32::<B>(*d as u32)); },
            SRational(ref v) => for d in v { try!(w.write_u64::<B>(*d as u64)); },
            Float32(ref v) =>   for d in v { try!(w.write_f32::<B>(*d)); },
            Float64(ref v) =>   for d in v { try!(w.write_f64::<B>(*d)); },
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::{BufReader, Cursor};

    #[test]
    fn basic_decode_jpeg() {
        let mut f = File::open("src/fixtures/IMG_2222.JPG").expect("couldn't open file");
        let segment = jpeg::extract_exif(&mut f).expect("extract exif");

        let e = Exif::new(&mut Cursor::new(segment)).expect("extract exif");
        dump_exif(&e);
    }

    #[test]
    fn basic_decode_bin() {
        let fe = File::open("src/fixtures/exif-sony-1.bin").expect("couldn't open file");
        let e = Exif::new(&mut BufReader::new(fe)).expect("extract exif");
        dump_exif(&e);
    }

    #[test]
    fn basic_roundtrip() {
        let fe = File::open("src/fixtures/exif-sony-1.bin").expect("open file");
        let e = Exif::new(&mut BufReader::new(fe)).expect("extract exif");

        let mut buf = Cursor::new(vec![]);
        e.encode(&mut buf).expect("encode exif file");

        buf.set_position(0);
        Exif::new(&mut buf).expect("extract exif 2");

        // dump_exif(&e);
    }

    fn dump_exif(e: &Exif) {
        for ifd in &e.ifds {
            println!("ifd 0x{:x}, {} entries, {} children", ifd.id, ifd.entries.len(), ifd.children.len());
            for e in &ifd.entries {
                println!("    {:?}", e)
            }
            for subifd in &ifd.children {
                println!("    SUBifd 0x{:x}, {} entries, {} children", subifd.id, subifd.entries.len(), subifd.children.len());
                for e in &subifd.entries {
                    println!("        {:?}", e)
                }
            }
        }
    }
}
