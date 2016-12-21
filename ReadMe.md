
# imagemeta-rs

[![Build Status](https://travis-ci.org/liamstask/imagemeta-rs.svg)](https://travis-ci.org/liamstask/imagemeta-rs)
[![Crates.io](https://img.shields.io/crates/v/imagemeta.svg?maxAge=2592000)](https://crates.io/crates/imagemeta)
[![Docs.rs](https://docs.rs/imagemeta/badge.svg)](https://docs.rs/imagemeta)

Basic image metadata handling library in Rust.

### status

New, minimal, and not extensively tested.

**exif**: Basic read/write of exif entities works. Not much in the way of vendor-specific support, but should hopefully provide a basis upon which to build.

**xmp**: would be nice.

### references/notes

* http://www.exiv2.org/Exif2-2.PDF
* https://www.media.mit.edu/pia/Research/deepview/exif.html
* http://www.sno.phy.queensu.ca/~phil/exiftool/TagNames/EXIF.html

## License

Licensed under either of
 * Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE.md) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)
at your option.
