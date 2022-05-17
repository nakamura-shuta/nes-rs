use super::header::Header;
use std::fs::File;
use std::io;
use std::io::Read;

const NES_HEADER_SIZE: usize = 0x10;

#[derive(Debug, PartialEq, Clone)]
pub enum Mirroring {
    VERTICAL,
    HORIZONTAL,
    FOUR_SCREEN,
}

/// Rom struct
///
/// # Parameters
/// * `header` - Header struct
/// * `program` - program  rom
/// * `charrom` - charactor rom
#[derive(Debug)]
pub struct Rom {
    pub header: Header,
    pub program_data: Vec<u8>,
    pub char_data: Vec<u8>,
    pub mapper: u8,
    pub screen_mirroring: Mirroring,
}

impl Rom {
    /// load rom data
    ///
    /// # Parameters
    /// * `path` - Path of ROM file
    pub fn load(path: &str) -> Result<Self, io::Error> {
        //read Rom file
        let rom_buffer = load_file(path);

        //read Header
        let nes_header = Header::new(&rom_buffer.to_vec())?;
        println!("{:?}", nes_header);

        //read program data
        let program_data = load_program(&rom_buffer, &nes_header)?;
        //read charctor data
        let char_data = load_char(&rom_buffer, &nes_header)?;

        //mapper
        let mapper = (rom_buffer[7] & 0b1111_0000) | (rom_buffer[6] >> 4);

        //screen mirroring
        let four_screen = rom_buffer[6] & 0b1000 != 0;
        let vertical_mirroring = rom_buffer[6] & 0b1 != 0;
        let screen_mirroring = match (four_screen, vertical_mirroring) {
            (true, _) => Mirroring::FOUR_SCREEN,
            (false, true) => Mirroring::VERTICAL,
            (false, false) => Mirroring::HORIZONTAL,
        };

        Ok(Rom {
            header: nes_header,
            program_data,
            char_data,
            mapper,
            screen_mirroring,
        })
    }
}

/// read Rom file. Returns ROM buffer.
///
/// # Parameters
/// * `path` - Path of ROM file
fn load_file(path: &str) -> Vec<u8> {
    let mut file = match File::open(&path) {
        Ok(file) => file,
        Err(_) => panic!("couldn't open file"),
    };

    let filesize: u64;
    match file.metadata() {
        Ok(metadata) => {
            filesize = metadata.len();
        }
        Err(_) => panic!("couldn't resolve metadata"),
    }

    let mut buffer = vec![0; filesize as usize];
    match file.read(&mut buffer) {
        Ok(_) => println!("read rom file"),
        Err(_) => panic!("couldn't read file"),
    }
    buffer
}

///load Program data from buffer. Returns Program buffer.
///
/// # Parameters
/// * `buffer` - ROM buffer
/// * `header` - Header struct
fn load_program(buffer: &[u8], header: &Header) -> Result<Vec<u8>, std::io::Error> {
    let start: usize = NES_HEADER_SIZE;
    let end = start + header.program_size as usize;
    Ok(buffer[start..end].to_vec())
}

///load Charactor data from buffer. Returns Charactor buffer.
///
/// # Parameters
/// * `buffer` - ROM buffer
/// * `header` - Header struct
fn load_char(buffer: &[u8], header: &Header) -> Result<Vec<u8>, std::io::Error> {
    let start: usize = NES_HEADER_SIZE + header.program_size as usize;
    let end = start + header.char_size as usize;
    Ok(buffer[start..end].to_vec())
}

#[cfg(test)]
mod rom_tests {
    use super::*;

    fn img(rom: &Rom) -> Option<image::RgbaImage> {
        let num = rom.char_data.len() / 16;

        if num == 0 {
            return None;
        }

        // const UNIT: usize = 2;
        const DOT: usize = 8;

        let w = 50_usize; //put 50 splits horizontally
        let h = num / w + (num % w != 0) as usize; //put h splits vertically

        let mut img: image::RgbaImage = image::ImageBuffer::new((w * DOT) as u32, (h * DOT) as u32);

        // img.put_pixel(x: u32, y: u32, pixel: P);

        const COLOR_PALLETTE: [[u8; 4]; 4] = [
            // [0x69, 0xA2, 0xFF, 255], //transparent
            // [0xBA, 0x06, 0, 255],    //color index = 6
            // [0xFF, 0x88, 0x33, 255], //color index = 38
            // [0xC4, 0x62, 0x00, 255], //color index = 8
            [0, 0, 0, 255],       //Black
            [0, 0, 0, 255],       //Black
            [255, 0, 0, 255],     //Black
            [255, 255, 255, 255], //White
        ];

        (0..num).for_each(|sprite_index| {
            let sprite: [u8; 16] = rom
                .char_data
                .get(sprite_index * 16..(sprite_index + 1) * 16)
                .unwrap()
                .try_into()
                .unwrap();

            let cindexes = calc_cindex(sprite);

            let row = sprite_index % w;
            let col = sprite_index / w;
            let xoffset = row * 8;
            let yoffset = col * 8;

            (0..8).for_each(|y| {
                let indexes = &cindexes[y * 8..(y + 1) * 8];
                indexes.iter().enumerate().for_each(|(x, c)| {
                    let pixel = image::Rgba(COLOR_PALLETTE[*c]);
                    img.put_pixel((x + xoffset) as u32, (y + yoffset) as u32, pixel);
                });
            });
        });

        Some(img)
    }

    fn calc_cindex(sprite: [u8; 16]) -> [usize; 64] {
        let sprite1 = &sprite[0..8];
        let sprite2 = &sprite[8..16];
        let mut palette = [0usize; 64];

        sprite1
            .iter()
            .zip(sprite2)
            .enumerate()
            .for_each(|(i, (row1, row2))| {
                let odd_palette_num = (row1 & 0b0101_0101) | ((row2 & 0b0101_0101) << 1);
                let even_palette_num = ((row1 & 0b1010_1010) >> 1) | (row2 & 0b1010_1010);

                palette[i * 8 + 0] = ((even_palette_num & 0b1100_0000) >> 6) as usize;
                palette[i * 8 + 2] = ((even_palette_num & 0b0011_0000) >> 4) as usize;
                palette[i * 8 + 4] = ((even_palette_num & 0b0000_1100) >> 2) as usize;
                palette[i * 8 + 6] = ((even_palette_num & 0b0000_0011) >> 0) as usize;

                palette[i * 8 + 1] = ((odd_palette_num & 0b1100_0000) >> 6) as usize;
                palette[i * 8 + 3] = ((odd_palette_num & 0b0011_0000) >> 4) as usize;
                palette[i * 8 + 5] = ((odd_palette_num & 0b0000_1100) >> 2) as usize;
                palette[i * 8 + 7] = ((odd_palette_num & 0b0000_0011) >> 0) as usize;
            });

        palette
    }

    #[test]
    fn save_img() {
        let rom = Rom::load("./hello_world.nes").unwrap();
        img(&rom).unwrap().save("char.png").unwrap();
    }
}
