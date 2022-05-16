use std::io::{Error, ErrorKind};

/// Header Struct
///
/// # Parameters
/// * `nes_header_const` - ASCII letters 'NES' followed by 0x1A(EOF)
/// * `program_size` - プログラムROMサイズ
/// * `char_size` - キャラクターROMサイズ
#[derive(Debug, PartialEq)]
pub struct Header {
    pub nes_header_const: [u8; 4],
    pub program_size: u32,
    pub char_size: u32,
}

impl Header {
    pub fn new(buf: &Vec<u8>) -> Result<Self, Error> {
        // <iNES file format header>
        // 0-3: Constant $4E $45 $53 $1A ("NES" followed by MS-DOS end-of-file)
        // 4: Size of PRG ROM in 16 KB units
        // 5: Size of CHR ROM in 8 KB units (Value 0 means the board uses CHR RAM)
        // refer: https://wiki.nesdev.com/w/index.php/INES

        let headers = *array_ref!(buf, 0, 4);
        match headers {
            [78, 69, 83, 26] => Ok(Header {
                nes_header_const: headers,
                //allocates a buffer of 16KiB. 0x4000 means 4000 in hexadecimal, which is 16384 in decimal.
                program_size: (buf[4] as u32) * 0x4000,
                //allocates a buffer of 8KiB. 0x2000 means 2000 in hexadecimal, which is 8192 in decimal.
                char_size: (buf[5] as u32) * 0x2000,
            }),
            _ => {
                return Err(std::io::Error::new(
                    ErrorKind::Other,
                    format!("Invalid file header. {:?}", headers),
                ))
            }
        }
    }
}

#[cfg(test)]
mod header_test {

    use super::*;

    #[test]
    fn new_success() {
        // "N" "E" "S" "\x1A" "5" "3"
        let rom_bytes = [78, 69, 83, 26, 53, 51];
        assert_eq!(rom_bytes, *"NES\x1A53".as_bytes());

        let header = Header::new(&rom_bytes.to_vec()).unwrap();
        assert_eq!(
            header,
            Header {
                nes_header_const: [rom_bytes[0], rom_bytes[1], rom_bytes[2], rom_bytes[3],],
                program_size: (rom_bytes[4] as u32) * 0x4000,
                char_size: (rom_bytes[5] as u32) * 0x2000,
            }
        );
    }

    #[test]
    fn new_format_error() {
        // "N" "X" "S" "\x1A" "5" "3"
        let rom_bytes = [78, 88, 83, 26, 53, 51];
        assert_eq!(rom_bytes, *"NXS\x1A53".as_bytes());

        let ines_header = Header::new(&rom_bytes.to_vec());
        assert!(match ines_header {
            Err(_error) => true,
            _ => false,
        });
    }
}
