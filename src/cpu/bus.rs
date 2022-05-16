use crate::ppu::ppu::Ppu;
use crate::ppu::ppu::TPpu;
use crate::{rom::rom::Rom, Memory};

const RAM: u16 = 0x0000;
const RAM_MIRRORS_END: u16 = 0x1FFF;
const PPU_REGISTERS: u16 = 0x2000;
const PPU_REGISTERS_MIRRORS_END: u16 = 0x3FFF;

/// Bus Struct
/// RAMに直接アクセスできるモジュール
pub struct Bus<'call> {
    cpu_vram: [u8; 2048],
    program_data: Vec<u8>,
    ppu: Ppu,
    cycles: usize,
    gameloop_callback: Box<dyn FnMut(&Ppu) + 'call>,
}

impl<'a> Bus<'a> {
    /// Busコンストラクタ
    ///
    /// # Parameters
    /// * `rom` - Rom
    /// * `gameloop_callback` - ループ処理用コールバック
    pub fn new<'call, F>(rom: Rom, gameloop_callback: F) -> Bus<'call>
    where
        F: FnMut(&Ppu) + 'call,
    {
        //PPU作成
        let ppu = Ppu::new_ppu(rom.char_data, rom.screen_mirroring);

        Bus {
            cpu_vram: [0; 2048],
            program_data: rom.program_data,
            ppu,
            cycles: 0,
            gameloop_callback: Box::from(gameloop_callback),
        }
    }

    fn read_program_data(&self, mut addr: u16) -> u8 {
        addr -= 0x8000;
        if self.program_data.len() == 0x4000 && addr >= 0x4000 {
            addr %= 0x4000;
        }
        self.program_data[addr as usize]
    }

    pub fn tick(&mut self, cycles: u8) {
        self.cycles += cycles as usize;
        let new_frame = self.ppu.tick(cycles * 3);
        if new_frame {
            (self.gameloop_callback)(&self.ppu);
        }
    }

    pub fn poll_nmi_status(&mut self) -> Option<u8> {
        self.ppu.nmi_interrupt.take()
    }
}

impl Memory for Bus<'_> {
    fn mem_read(&mut self, addr: u16) -> u8 {
        match addr {
            RAM..=RAM_MIRRORS_END => {
                let mirror_down_addr = addr & 0b0000_0111_1111_1111;
                self.cpu_vram[mirror_down_addr as usize]
            }
            0x2000 | 0x2001 | 0x2003 | 0x2005 | 0x2006 | 0x4014 => 0,
            0x2002 => self.ppu.read_status(),
            0x2004 => self.ppu.read_oam_data(),
            0x2007 => self.ppu.read_data(),
            0x4000..=0x4015 => {
                //ignore APU
                0
            }

            0x4016 => {
                // ignore joypad 1;
                0
            }

            0x4017 => {
                // ignore joypad 2
                0
            }
            0x2008..=PPU_REGISTERS_MIRRORS_END => {
                let mirror_down_addr = addr & 0b0010_0000_0000_0111;
                self.mem_read(mirror_down_addr)
            }
            0x8000..=0xFFFF => self.read_program_data(addr),

            _ => {
                println!("Ignoring mem access at {}", addr);
                0
            }
        }
    }

    fn mem_write(&mut self, addr: u16, data: u8) {
        match addr {
            RAM..=RAM_MIRRORS_END => {
                let mirror_down_addr = addr & 0b11111111111;
                self.cpu_vram[mirror_down_addr as usize] = data;
            }
            0x2000 => {
                self.ppu.write_to_ctrl(data);
            }
            0x2001 => {
                self.ppu.write_to_mask(data);
            }
            0x2002 => panic!("attempt to write to PPU status register"),

            0x2003 => {
                self.ppu.write_to_oam_addr(data);
            }
            0x2004 => {
                self.ppu.write_to_oam_data(data);
            }
            0x2005 => {
                self.ppu.write_to_scroll(data);
            }
            0x2006 => {
                self.ppu.write_to_ppu_addr(data);
            }
            0x2007 => {
                self.ppu.write_to_data(data);
            }
            0x4000..=0x4013 | 0x4015 => {
                //ignore APU
            }

            0x4016 => {
                // ignore joypad 1;
            }

            0x4017 => {
                // ignore joypad 2
            }

            // https://wiki.nesdev.com/w/index.php/PPU_programmer_reference#OAM_DMA_.28.244014.29_.3E_write
            0x4014 => {
                let mut buffer: [u8; 256] = [0; 256];
                let hi: u16 = (data as u16) << 8;
                for i in 0..256u16 {
                    buffer[i as usize] = self.mem_read(hi + i);
                }

                self.ppu.write_oam_dma(&buffer);

                // todo: handle this eventually
                // let add_cycles: u16 = if self.cycles % 2 == 1 { 514 } else { 513 };
                // self.tick(add_cycles); //todo this will cause weird effects as PPU will have 513/514 * 3 ticks
            }

            0x2008..=PPU_REGISTERS_MIRRORS_END => {
                let mirror_down_addr = addr & 0b0010_0000_0000_0111;
                self.mem_write(mirror_down_addr, data);
            }
            0x8000..=0xFFFF => panic!("Attempt to write to Cartridge ROM space: {:x}", addr),

            _ => {
                println!("Ignoring mem write-access at {}", addr);
            }
        }
    }
}
