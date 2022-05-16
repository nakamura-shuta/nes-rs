use crate::ppu::addr::AddrRegister;
use crate::ppu::control::ControlRegister;
use crate::ppu::mask::MaskRegister;
use crate::ppu::scroll::ScrollRegister;
use crate::ppu::status::StatusRegister;
use crate::rom::rom::Mirroring;

/// PPU struct
/// PPUのレジスタはCPUから見て0x2000~0x2007
///
/// | address |  size | purpose |
/// |---------|---------|---------|
/// |0x0000～0x07FF| 0x0800| WRAM|
/// |0x0800～0x1FFF| - |WRAMのミラー|
/// |0x2000～0x2007| 0x0008| PPU レジスタ|
/// |0x2008～0x3FFF| - |PPUレジスタのミラー|
/// |0x4000～0x401F| 0x0020| APU(Audio Processing Unit) I/O、PAD|
/// |0x4020～0x5FFF| 0x1FE0| 拡張ROM|
/// |0x6000～0x7FFF| 0x2000| バッテリーバックアップRAM|
/// |0x8000～0xBFFF| 0x4000| PRG-ROM(LOW)|
///
/// PPU Register
/// | address |  short name |  R/W name | contents |
/// |---------|---------|---------|---------|
/// |0x2000| PPUCTRL| W| コントロールレジスタ1| 割り込みなどPPUの設定|
/// |0x2001| PPUMASK| W| コントロールレジスタ2| 背景イネーブルなどのPPU設定|
/// |0x2002| PPUSTATUS| R| PPUステータス| PPUのステータス|
/// |0x2003| OAMADDR| W| スプライトメモリデータ| 書き込むスプライト領域のアドレス|
/// |0x2004| OAMDATA| RW| デシマルモード| スプライト領域のデータv
/// |0x2005| PPUSCROLL| W| 背景スクロールオフセット| 背景スクロール値|
/// |0x2006| PPUADDR| W| PPUメモリアドレス| 書き込むPPUメモリ領域のアドレス|
/// |0x2007| PPUDATA| RW| PPUメモリデータ| PPUメモリ領域のデータ|
#[derive(Debug)]
pub struct Ppu {
    ///ROMに保存されているゲームのビジュアル
    pub char_data: Vec<u8>,
    ///画面で使用されるパレットテーブルを保持するための内部メモリ
    pub palette_table: [u8; 32],
    ///背景情報を保持するための2KiBのスペースバンク
    pub vram: [u8; 2048],
    ///スプライトの状態を保持するための内部メモリ
    pub oam_data: [u8; 256],
    ///ミラーリング
    pub mirroring: Mirroring,
    /// Address Register
    pub addr: AddrRegister,
    // Control Rregister
    pub ctrl: ControlRegister,

    /// Aask Register
    pub mask: MaskRegister,
    /// Status Register
    pub status: StatusRegister,
    /// Scroll Register
    pub scroll: ScrollRegister,

    pub oam_addr: u8,
    internal_data_buf: u8,

    ///ライン
    scanline: u16,
    ///PPUサイクル
    cycles: usize,
    ///NMI
    pub nmi_interrupt: Option<u8>,
}

pub trait TPpu {
    fn write_to_ctrl(&mut self, value: u8);
    fn write_to_mask(&mut self, value: u8);
    fn read_status(&mut self) -> u8;
    fn write_to_oam_addr(&mut self, value: u8);
    fn write_to_oam_data(&mut self, value: u8);
    fn read_oam_data(&self) -> u8;
    fn write_to_scroll(&mut self, value: u8);
    fn write_to_ppu_addr(&mut self, value: u8);
    fn write_to_data(&mut self, value: u8);
    fn read_data(&mut self) -> u8;
    fn write_oam_dma(&mut self, value: &[u8; 256]);
}

impl Ppu {
    ///PPUコンストラクタ
    ///
    /// # Parameters
    /// * `char_data` - キャラクターデータ
    /// * `mirroring` - ミラーリング
    pub fn new_ppu(char_data: Vec<u8>, mirroring: Mirroring) -> Self {
        Ppu {
            char_data,
            mirroring,
            ctrl: ControlRegister::new(),
            mask: MaskRegister::new(),
            status: StatusRegister::new(),
            oam_addr: 0,
            scroll: ScrollRegister::new(),
            addr: AddrRegister::new(),
            vram: [0; 2048],
            oam_data: [0; 64 * 4],
            palette_table: [0; 32],
            internal_data_buf: 0,
            cycles: 0,
            scanline: 0,
            nmi_interrupt: None,
        }
    }

    fn increment_vram_addr(&mut self) {
        self.addr.increment(self.ctrl.vram_addr_increment());
    }

    /// PPUのサイクルを進める.
    /// CPU が 1 サイクル動作する毎に PPUは3 サイクル分動作する.
    ///
    /// # Parameters
    /// * `cycles` - サイクル
    pub fn tick(&mut self, cycles: u8) -> bool {
        //NES の解像度 = 256*240 *1.
        //内部的には 341*262.
        //1 PPU サイクルで 1 dot 処理される.
        //341*262 = 89342 PPU サイクルが 1 フレーム
        self.cycles += cycles as usize;
        if self.cycles >= 341 {
            self.cycles -= 341;
            self.scanline += 1;

            //line 241でVBLANKフラグ=trueになり
            //NMI 割り込みが発生
            if self.scanline == 241 {
                self.status.set_vblank_status(true);
                self.status.set_sprite_zero_hit(false);
                if self.ctrl.generate_vblank_nmi() {
                    self.nmi_interrupt = Some(1);
                }
            }

            //1scanline処理おわり
            if self.scanline >= 262 {
                self.scanline = 0;
                self.nmi_interrupt = None;
                self.status.set_sprite_zero_hit(false);
                self.status.reset_vblank_status();
                return true;
            }
        }
        false
    }

    // fn poll_nmi_interrupt(&mut self) -> Option<u8> {
    //     self.nmi_interrupt.take()
    // }

    // Horizontal:
    //   [ A ] [ a ]
    //   [ B ] [ b ]

    // Vertical:
    //   [ A ] [ B ]
    //   [ a ] [ b ]
    pub fn mirror_vram_addr(&self, addr: u16) -> u16 {
        let mirrored_vram = addr & 0b10111111111111; // mirror down 0x3000-0x3eff to 0x2000 - 0x2eff
        let vram_index = mirrored_vram - 0x2000; // to vram vector
        let name_table = vram_index / 0x400; // to the name table index
        match (&self.mirroring, name_table) {
            (Mirroring::VERTICAL, 2) | (Mirroring::VERTICAL, 3) => vram_index - 0x800,
            (Mirroring::HORIZONTAL, 2) => vram_index - 0x400,
            (Mirroring::HORIZONTAL, 1) => vram_index - 0x400,
            (Mirroring::HORIZONTAL, 3) => vram_index - 0x800,
            _ => vram_index,
        }
    }
}

impl TPpu for Ppu {
    fn write_to_ctrl(&mut self, value: u8) {
        let _before_nmi_status = self.ctrl.generate_vblank_nmi();
        self.ctrl.update(value);
    }

    fn write_to_mask(&mut self, value: u8) {
        self.mask.update(value);
    }

    fn read_status(&mut self) -> u8 {
        let data = self.status.snapshot();
        self.status.reset_vblank_status();
        self.addr.reset_latch();
        self.scroll.reset_latch();
        data
    }

    fn write_to_oam_addr(&mut self, value: u8) {
        self.oam_addr = value;
    }

    fn write_to_oam_data(&mut self, value: u8) {
        self.oam_data[self.oam_addr as usize] = value;
        self.oam_addr = self.oam_addr.wrapping_add(1);
    }

    fn read_oam_data(&self) -> u8 {
        self.oam_data[self.oam_addr as usize]
    }

    fn write_to_scroll(&mut self, value: u8) {
        self.scroll.write(value);
    }

    fn write_to_ppu_addr(&mut self, value: u8) {
        self.addr.update(value);
    }

    fn write_to_data(&mut self, value: u8) {
        let addr = self.addr.get();
        match addr {
            0..=0x1fff => println!("attempt to write to chr rom space {}", addr),
            0x2000..=0x2fff => {
                self.vram[self.mirror_vram_addr(addr) as usize] = value;
            }
            0x3000..=0x3eff => unimplemented!("addr {} shouldn't be used in reallity", addr),

            //Addresses $3F10/$3F14/$3F18/$3F1C are mirrors of $3F00/$3F04/$3F08/$3F0C
            0x3f10 | 0x3f14 | 0x3f18 | 0x3f1c => {
                let add_mirror = addr - 0x10;
                self.palette_table[(add_mirror - 0x3f00) as usize] = value;
            }
            0x3f00..=0x3fff => {
                self.palette_table[(addr - 0x3f00) as usize] = value;
            }
            _ => panic!("unexpected access to mirrored space {}", addr),
        }
        self.increment_vram_addr();
    }

    fn read_data(&mut self) -> u8 {
        let addr = self.addr.get();

        self.increment_vram_addr();

        match addr {
            0..=0x1fff => {
                let result = self.internal_data_buf;
                self.internal_data_buf = self.char_data[addr as usize];
                result
            }
            0x2000..=0x2fff => {
                let result = self.internal_data_buf;
                self.internal_data_buf = self.vram[self.mirror_vram_addr(addr) as usize];
                result
            }
            0x3000..=0x3eff => unimplemented!("addr {} shouldn't be used in reallity", addr),

            //Addresses $3F10/$3F14/$3F18/$3F1C are mirrors of $3F00/$3F04/$3F08/$3F0C
            0x3f10 | 0x3f14 | 0x3f18 | 0x3f1c => {
                let add_mirror = addr - 0x10;
                self.palette_table[(add_mirror - 0x3f00) as usize]
            }

            0x3f00..=0x3fff => self.palette_table[(addr - 0x3f00) as usize],
            _ => panic!("unexpected access to mirrored space {}", addr),
        }
    }

    fn write_oam_dma(&mut self, data: &[u8; 256]) {
        for x in data.iter() {
            self.oam_data[self.oam_addr as usize] = *x;
            self.oam_addr = self.oam_addr.wrapping_add(1);
        }
    }
}
