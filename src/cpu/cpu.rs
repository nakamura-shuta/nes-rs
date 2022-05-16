use super::opcodes;
use crate::Bus;
use std::collections::HashMap;

bitflags! {
    /// # Status Register (P) http://wiki.nesdev.com/w/index.php/Status_flags
    ///
    /// ステータスレジスタはCPU状態を示すフラグ郡。
    /// bit5は常に1で、bit3はNESでは未実装です。
    /// IRQは割り込み、BRKはソフトウエア割り込みです。
    ///
    /// | bit |  name | detail |contents |
    /// |---------|---------|---------|---------|
    /// |bit7	|N|	ネガティブ|	演算結果のbit7が1の時にセット|
    /// |bit6	|V|	オーバーフローv	P演算結果がオーバーフローを起こした時にセット|
    /// |bit5	|R|	予約済み|	常にセットされている|
    /// |bit4	|B|	ブレークモード|	BRK発生時にセット、IRQ発生時にクリア|
    /// |bit3	|D|	デシマルモード|	0:デフォルト、1:BCDモード (未実装)|
    /// |bit2	|I|	IRQ禁止|	0:IRQ許可、1:IRQ禁止|
    /// |bit1	|Z|	ゼロ|	演算結果が0の時にセット|
    /// |bit0	|C|	キャリー|	キャリー発生時にセット|
    ///
    ///  7 6 5 4 3 2 1 0
    ///  N V _ B D I Z C
    ///  | |   | | | | +--- Carry Flag
    ///  | |   | | | +----- Zero Flag
    ///  | |   | | +------- Interrupt Disable
    ///  | |   | +--------- Decimal Mode (not used on NES)
    ///  | |   +----------- Break Command
    ///  | +--------------- Overflow Flag
    ///  +----------------- Negative Flag
    ///
    pub struct CpuFlags: u8 {
        const CARRY             = 0b00000001;
        const ZERO              = 0b00000010;
        const INTERRUPT_DISABLE = 0b00000100;
        const DECIMAL_MODE      = 0b00001000;
        const BREAK             = 0b00010000;
        const BREAK2            = 0b00100000;
        const OVERFLOW          = 0b01000000;
        const NEGATIV           = 0b10000000;
    }
}

const STACK: u16 = 0x0100;
const STACK_RESET: u8 = 0xfd;

/// # Cpu Struct.
///
/// レジスタ一覧。上位8bitは0x01に固定。
/// スタックは256バイトが使用可能で、WRAMのうち0x0100～0x01FF。
/// スタックポインタレジスタが0xA0の場合、スタックポインタは0x01A0になる。
/// 演算はAレジスタで行われ、X,Yレジスタはインデックスに使用される。
///
/// | name |  size | detail |
/// |---------|---------|---------|
/// |A	 |8bit |	アキュムレータ|
/// |X	 |8bit |	インデックスレジスタ|
/// |Y	 |8bit |	インデックスレジスタ|
/// |S	 |8bit |	スタックポインタ|
/// |P	 |8bit |	ステータスレジスタ|
/// |PC	|16bit|	  プログラムカウンタ|
pub struct Cpu<'a> {
    pub reg_a: u8,
    pub reg_x: u8,
    pub reg_y: u8,
    pub reg_sp: u8,
    pub status: CpuFlags,
    pub reg_pc: u16,
    //pub memory: [u8; 0xFFFF],
    pub bus: Bus<'a>,
}

/// Addressing Mode
/// CPUが命令ストリームの次の1バイト or 2バイトを
/// どのように解釈するか定義する
#[derive(Debug)]
#[allow(non_camel_case_types)]
pub enum AddressingMode {
    Immediate,
    ZeroPage,
    ZeroPage_X,
    ZeroPage_Y,
    Absolute,
    Absolute_X,
    Absolute_Y,
    Indirect_X,
    Indirect_Y,
    NoneAddressing,
}

/// Memory Trait
/// メモリ操作に関する定義,
pub trait Memory {
    fn mem_read(&mut self, addr: u16) -> u8;

    fn mem_write(&mut self, addr: u16, data: u8);

    fn mem_read_u16(&mut self, pos: u16) -> u16 {
        let lo = self.mem_read(pos) as u16;
        let hi = self.mem_read(pos + 1) as u16;
        (hi << 8) | (lo as u16)
    }

    fn mem_write_u16(&mut self, pos: u16, data: u16) {
        let hi = (data >> 8) as u8;
        let lo = (data & 0xff) as u8;
        self.mem_write(pos, lo);
        self.mem_write(pos + 1, hi);
    }
}

impl Memory for Cpu<'_> {
    fn mem_read(&mut self, addr: u16) -> u8 {
        self.bus.mem_read(addr)
    }

    fn mem_write(&mut self, addr: u16, data: u8) {
        self.bus.mem_write(addr, data)
    }
    fn mem_read_u16(&mut self, addr: u16) -> u16 {
        self.bus.mem_read_u16(addr)
    }

    fn mem_write_u16(&mut self, addr: u16, data: u16) {
        self.bus.mem_write_u16(addr, data)
    }
}

mod interrupt {
    #[derive(PartialEq, Eq)]
    pub enum InterruptType {
        NMI,
    }

    #[derive(PartialEq, Eq)]
    pub(super) struct Interrupt {
        pub(super) itype: InterruptType,
        pub(super) vector_addr: u16,
        pub(super) b_flag_mask: u8,
        pub(super) cpu_cycles: u8,
    }
    pub(super) const NMI: Interrupt = Interrupt {
        itype: InterruptType::NMI,
        vector_addr: 0xfffA,
        b_flag_mask: 0b00100000,
        cpu_cycles: 2,
    };
}

impl<'a> Cpu<'a> {
    ///Cpuコンストラクタ
    ///
    /// # Parameters
    /// * `bus` - Bus
    pub fn new<'b>(bus: Bus<'b>) -> Cpu<'b> {
        Cpu {
            reg_a: 0,
            reg_x: 0,
            reg_y: 0,
            reg_sp: STACK_RESET,
            reg_pc: 0,
            status: CpuFlags::from_bits_truncate(0b100100),
            bus,
        }
    }

    ///AddressingModeによって読み出すメモリのアドレスを算出.
    ///
    /// # Parameters
    /// * `mode` - AddressingMode
    /// # Reference
    /// * https://zenn.dev/szktty/articles/nes-addressingmode
    fn get_operand_address(&mut self, mode: &AddressingMode) -> u16 {
        match mode {
            AddressingMode::Immediate => self.reg_pc,

            AddressingMode::ZeroPage => self.mem_read(self.reg_pc) as u16,

            AddressingMode::Absolute => self.mem_read_u16(self.reg_pc),

            AddressingMode::ZeroPage_X => {
                let pos = self.mem_read(self.reg_pc);

                pos.wrapping_add(self.reg_x) as u16
            }
            AddressingMode::ZeroPage_Y => {
                let pos = self.mem_read(self.reg_pc);

                pos.wrapping_add(self.reg_y) as u16
            }

            AddressingMode::Absolute_X => {
                let base = self.mem_read_u16(self.reg_pc);

                base.wrapping_add(self.reg_x as u16)
            }
            AddressingMode::Absolute_Y => {
                let base = self.mem_read_u16(self.reg_pc);

                base.wrapping_add(self.reg_y as u16)
            }

            AddressingMode::Indirect_X => {
                let base = self.mem_read(self.reg_pc);

                let ptr: u8 = (base as u8).wrapping_add(self.reg_x);
                let lo = self.mem_read(ptr as u16);
                let hi = self.mem_read(ptr.wrapping_add(1) as u16);
                (hi as u16) << 8 | (lo as u16)
            }
            AddressingMode::Indirect_Y => {
                let base = self.mem_read(self.reg_pc);

                let lo = self.mem_read(base as u16);
                let hi = self.mem_read((base as u8).wrapping_add(1) as u16);
                let deref_base = (hi as u16) << 8 | (lo as u16);

                deref_base.wrapping_add(self.reg_y as u16)
            }
            AddressingMode::NoneAddressing => {
                panic!("mode {:?} is not supported", mode);
            }
        }
    }

    fn ldy(&mut self, mode: &AddressingMode) {
        let addr = self.get_operand_address(mode);
        let data = self.mem_read(addr);
        self.reg_y = data;
        self.update_zero_and_negative_flags(self.reg_y);
    }

    fn ldx(&mut self, mode: &AddressingMode) {
        let addr = self.get_operand_address(mode);
        let data = self.mem_read(addr);
        self.reg_x = data;
        self.update_zero_and_negative_flags(self.reg_x);
    }

    fn lda(&mut self, mode: &AddressingMode) {
        let addr = self.get_operand_address(mode);
        let value = self.mem_read(addr);
        self.set_reg_a(value);
    }

    fn sta(&mut self, mode: &AddressingMode) {
        let addr = self.get_operand_address(mode);
        self.mem_write(addr, self.reg_a);
    }

    fn set_reg_a(&mut self, value: u8) {
        self.reg_a = value;
        self.update_zero_and_negative_flags(self.reg_a);
    }

    fn and(&mut self, mode: &AddressingMode) {
        let addr = self.get_operand_address(mode);
        let data = self.mem_read(addr);
        self.set_reg_a(data & self.reg_a);
    }

    fn eor(&mut self, mode: &AddressingMode) {
        let addr = self.get_operand_address(mode);
        let data = self.mem_read(addr);
        self.set_reg_a(data ^ self.reg_a);
    }

    fn ora(&mut self, mode: &AddressingMode) {
        let addr = self.get_operand_address(mode);
        let data = self.mem_read(addr);
        self.set_reg_a(data | self.reg_a);
    }

    fn tax(&mut self) {
        self.reg_x = self.reg_a;
        self.update_zero_and_negative_flags(self.reg_x);
    }

    fn update_zero_and_negative_flags(&mut self, result: u8) {
        if result == 0 {
            self.status.insert(CpuFlags::ZERO);
        } else {
            self.status.remove(CpuFlags::ZERO);
        }

        if result >> 7 == 1 {
            self.status.insert(CpuFlags::NEGATIV);
        } else {
            self.status.remove(CpuFlags::NEGATIV);
        }
    }

    fn update_negative_flags(&mut self, result: u8) {
        if result >> 7 == 1 {
            self.status.insert(CpuFlags::NEGATIV)
        } else {
            self.status.remove(CpuFlags::NEGATIV)
        }
    }

    fn inx(&mut self) {
        self.reg_x = self.reg_x.wrapping_add(1);
        self.update_zero_and_negative_flags(self.reg_x);
    }

    fn iny(&mut self) {
        self.reg_y = self.reg_y.wrapping_add(1);
        self.update_zero_and_negative_flags(self.reg_y);
    }

    pub fn reset(&mut self) {
        self.reg_a = 0;
        self.reg_x = 0;
        self.reg_y = 0;
        self.reg_sp = STACK_RESET;
        self.status = CpuFlags::from_bits_truncate(0b100100);
        //self.memory = [0; 0xFFFF];
        self.reg_pc = self.mem_read_u16(0xFFFC);
    }

    fn set_carry_flag(&mut self) {
        self.status.insert(CpuFlags::CARRY)
    }

    fn clear_carry_flag(&mut self) {
        self.status.remove(CpuFlags::CARRY)
    }

    fn add_to_reg_a(&mut self, data: u8) {
        let sum = self.reg_a as u16
            + data as u16
            + (if self.status.contains(CpuFlags::CARRY) {
                1
            } else {
                0
            }) as u16;

        let carry = sum > 0xff;

        if carry {
            self.status.insert(CpuFlags::CARRY);
        } else {
            self.status.remove(CpuFlags::CARRY);
        }

        let result = sum as u8;

        if (data ^ result) & (result ^ self.reg_a) & 0x80 != 0 {
            self.status.insert(CpuFlags::OVERFLOW);
        } else {
            self.status.remove(CpuFlags::OVERFLOW)
        }

        self.set_reg_a(result);
    }

    fn sub_from_reg_a(&mut self, data: u8) {
        self.add_to_reg_a(((data as i8).wrapping_neg().wrapping_sub(1)) as u8);
    }

    fn and_with_reg_a(&mut self, data: u8) {
        self.set_reg_a(data & self.reg_a);
    }

    fn xor_with_reg_a(&mut self, data: u8) {
        self.set_reg_a(data ^ self.reg_a);
    }

    fn or_with_reg_a(&mut self, data: u8) {
        self.set_reg_a(data | self.reg_a);
    }

    fn sbc(&mut self, mode: &AddressingMode) {
        let addr = self.get_operand_address(mode);
        let data = self.mem_read(addr);
        self.add_to_reg_a(((data as i8).wrapping_neg().wrapping_sub(1)) as u8);
    }

    fn adc(&mut self, mode: &AddressingMode) {
        let addr = self.get_operand_address(mode);
        let value = self.mem_read(addr);
        self.add_to_reg_a(value);
    }

    fn stack_pop(&mut self) -> u8 {
        self.reg_sp = self.reg_sp.wrapping_add(1);
        self.mem_read((STACK as u16) + self.reg_sp as u16)
    }

    fn stack_push(&mut self, data: u8) {
        self.mem_write((STACK as u16) + self.reg_sp as u16, data);
        self.reg_sp = self.reg_sp.wrapping_sub(1)
    }

    fn stack_push_u16(&mut self, data: u16) {
        let hi = (data >> 8) as u8;
        let lo = (data & 0xff) as u8;
        self.stack_push(hi);
        self.stack_push(lo);
    }

    fn stack_pop_u16(&mut self) -> u16 {
        let lo = self.stack_pop() as u16;
        let hi = self.stack_pop() as u16;
        hi << 8 | lo
    }

    fn asl_accumulator(&mut self) {
        let mut data = self.reg_a;
        if data >> 7 == 1 {
            self.set_carry_flag();
        } else {
            self.clear_carry_flag();
        }
        data <<= 1;
        self.set_reg_a(data)
    }

    fn asl(&mut self, mode: &AddressingMode) -> u8 {
        let addr = self.get_operand_address(mode);
        let mut data = self.mem_read(addr);
        if data >> 7 == 1 {
            self.set_carry_flag();
        } else {
            self.clear_carry_flag();
        }
        data <<= 1;
        self.mem_write(addr, data);
        self.update_zero_and_negative_flags(data);
        data
    }

    fn lsr_accumulator(&mut self) {
        let mut data = self.reg_a;
        if data & 1 == 1 {
            self.set_carry_flag();
        } else {
            self.clear_carry_flag();
        }
        data >>= 1;
        self.set_reg_a(data)
    }

    fn lsr(&mut self, mode: &AddressingMode) -> u8 {
        let addr = self.get_operand_address(mode);
        let mut data = self.mem_read(addr);
        if data & 1 == 1 {
            self.set_carry_flag();
        } else {
            self.clear_carry_flag();
        }
        data >>= 1;
        self.mem_write(addr, data);
        self.update_zero_and_negative_flags(data);
        data
    }

    fn rol(&mut self, mode: &AddressingMode) -> u8 {
        let addr = self.get_operand_address(mode);
        let mut data = self.mem_read(addr);
        let old_carry = self.status.contains(CpuFlags::CARRY);

        if data >> 7 == 1 {
            self.set_carry_flag();
        } else {
            self.clear_carry_flag();
        }
        data <<= 1;
        if old_carry {
            data |= 1;
        }
        self.mem_write(addr, data);
        self.update_negative_flags(data);
        data
    }

    fn rol_accumulator(&mut self) {
        let mut data = self.reg_a;
        let old_carry = self.status.contains(CpuFlags::CARRY);

        if data >> 7 == 1 {
            self.set_carry_flag();
        } else {
            self.clear_carry_flag();
        }
        data <<= 1;
        if old_carry {
            data |= 1;
        }
        self.set_reg_a(data);
    }

    fn ror(&mut self, mode: &AddressingMode) -> u8 {
        let addr = self.get_operand_address(mode);
        let mut data = self.mem_read(addr);
        let old_carry = self.status.contains(CpuFlags::CARRY);

        if data & 1 == 1 {
            self.set_carry_flag();
        } else {
            self.clear_carry_flag();
        }
        data >>= 1;
        if old_carry {
            data |= 0b10000000;
        }
        self.mem_write(addr, data);
        self.update_negative_flags(data);
        data
    }

    fn ror_accumulator(&mut self) {
        let mut data = self.reg_a;
        let old_carry = self.status.contains(CpuFlags::CARRY);

        if data & 1 == 1 {
            self.set_carry_flag();
        } else {
            self.clear_carry_flag();
        }
        data >>= 1;
        if old_carry {
            data |= 0b10000000;
        }
        self.set_reg_a(data);
    }

    fn inc(&mut self, mode: &AddressingMode) -> u8 {
        let addr = self.get_operand_address(mode);
        let mut data = self.mem_read(addr);
        data = data.wrapping_add(1);
        self.mem_write(addr, data);
        self.update_zero_and_negative_flags(data);
        data
    }

    fn dey(&mut self) {
        self.reg_y = self.reg_y.wrapping_sub(1);
        self.update_zero_and_negative_flags(self.reg_y);
    }

    fn dex(&mut self) {
        self.reg_x = self.reg_x.wrapping_sub(1);
        self.update_zero_and_negative_flags(self.reg_x);
    }

    fn dec(&mut self, mode: &AddressingMode) -> u8 {
        let addr = self.get_operand_address(mode);
        let mut data = self.mem_read(addr);
        data = data.wrapping_sub(1);
        self.mem_write(addr, data);
        self.update_zero_and_negative_flags(data);
        data
    }

    fn pla(&mut self) {
        let data = self.stack_pop();
        self.set_reg_a(data);
    }

    fn plp(&mut self) {
        self.status.bits = self.stack_pop();
        self.status.remove(CpuFlags::BREAK);
        self.status.insert(CpuFlags::BREAK2);
    }

    fn php(&mut self) {
        //http://wiki.nesdev.com/w/index.php/CPU_status_flag_behavior
        let mut flags = self.status;
        flags.insert(CpuFlags::BREAK);
        flags.insert(CpuFlags::BREAK2);
        self.stack_push(flags.bits());
    }

    fn bit(&mut self, mode: &AddressingMode) {
        let addr = self.get_operand_address(mode);
        let data = self.mem_read(addr);
        let and = self.reg_a & data;
        if and == 0 {
            self.status.insert(CpuFlags::ZERO);
        } else {
            self.status.remove(CpuFlags::ZERO);
        }

        self.status.set(CpuFlags::NEGATIV, data & 0b10000000 > 0);
        self.status.set(CpuFlags::OVERFLOW, data & 0b01000000 > 0);
    }

    fn compare(&mut self, mode: &AddressingMode, compare_with: u8) {
        let addr = self.get_operand_address(mode);
        let data = self.mem_read(addr);
        if data <= compare_with {
            self.status.insert(CpuFlags::CARRY);
        } else {
            self.status.remove(CpuFlags::CARRY);
        }

        self.update_zero_and_negative_flags(compare_with.wrapping_sub(data));
    }

    fn branch(&mut self, condition: bool) {
        if condition {
            let jump: i8 = self.mem_read(self.reg_pc) as i8;
            let jump_addr = self.reg_pc.wrapping_add(1).wrapping_add(jump as u16);

            self.reg_pc = jump_addr;
        }
    }

    fn interrupt(&mut self, interrupt: interrupt::Interrupt) {
        self.stack_push_u16(self.reg_pc);
        let mut flag = self.status;
        flag.set(CpuFlags::BREAK, interrupt.b_flag_mask & 0b010000 == 1);
        flag.set(CpuFlags::BREAK2, interrupt.b_flag_mask & 0b100000 == 1);

        self.stack_push(flag.bits);
        self.status.insert(CpuFlags::INTERRUPT_DISABLE);

        self.bus.tick(interrupt.cpu_cycles);
        self.reg_pc = self.mem_read_u16(interrupt.vector_addr);
    }

    ///CPU実行
    pub fn run(&mut self) {
        self.run_with_callback(|_| {});
    }

    ///CPU実行
    ///
    /// # Parameters
    /// * `callback` - Cpuを引数にとるクロージャ
    pub fn run_with_callback<F>(&mut self, mut callback: F)
    where
        F: FnMut(&mut Cpu),
    {
        let opcodes: &HashMap<u8, &'static opcodes::OpCode> = &(*opcodes::OPCODES_MAP);

        loop {
            if let Some(_nmi) = self.bus.poll_nmi_status() {
                self.interrupt(interrupt::NMI);
            }

            callback(self);

            let code = self.mem_read(self.reg_pc);
            self.reg_pc += 1;
            let program_counter_state = self.reg_pc;

            //OpCode取得
            let opcode = opcodes
                .get(&code)
                .unwrap_or_else(|| panic!("OpCode {:x} is not recognized", code));

            match code {
                0xa9 | 0xa5 | 0xb5 | 0xad | 0xbd | 0xb9 | 0xa1 | 0xb1 => {
                    self.lda(&opcode.mode);
                }

                0xAA => self.tax(),
                0xe8 => self.inx(),
                0x00 => return,

                /* CLD */ 0xd8 => self.status.remove(CpuFlags::DECIMAL_MODE),

                /* CLI */ 0x58 => self.status.remove(CpuFlags::INTERRUPT_DISABLE),

                /* CLV */ 0xb8 => self.status.remove(CpuFlags::OVERFLOW),

                /* CLC */ 0x18 => self.clear_carry_flag(),

                /* SEC */ 0x38 => self.set_carry_flag(),

                /* SEI */ 0x78 => self.status.insert(CpuFlags::INTERRUPT_DISABLE),

                /* SED */ 0xf8 => self.status.insert(CpuFlags::DECIMAL_MODE),

                /* PHA */ 0x48 => self.stack_push(self.reg_a),

                /* PLA */
                0x68 => {
                    self.pla();
                }

                /* PHP */
                0x08 => {
                    self.php();
                }

                /* PLP */
                0x28 => {
                    self.plp();
                }

                /* ADC */
                0x69 | 0x65 | 0x75 | 0x6d | 0x7d | 0x79 | 0x61 | 0x71 => {
                    self.adc(&opcode.mode);
                }

                /* SBC */
                0xe9 | 0xe5 | 0xf5 | 0xed | 0xfd | 0xf9 | 0xe1 | 0xf1 => {
                    self.sbc(&opcode.mode);
                }

                /* AND */
                0x29 | 0x25 | 0x35 | 0x2d | 0x3d | 0x39 | 0x21 | 0x31 => {
                    self.and(&opcode.mode);
                }

                /* EOR */
                0x49 | 0x45 | 0x55 | 0x4d | 0x5d | 0x59 | 0x41 | 0x51 => {
                    self.eor(&opcode.mode);
                }

                /* ORA */
                0x09 | 0x05 | 0x15 | 0x0d | 0x1d | 0x19 | 0x01 | 0x11 => {
                    self.ora(&opcode.mode);
                }

                /* LSR */ 0x4a => self.lsr_accumulator(),

                /* LSR */
                0x46 | 0x56 | 0x4e | 0x5e => {
                    self.lsr(&opcode.mode);
                }

                /*ASL*/ 0x0a => self.asl_accumulator(),

                /* ASL */
                0x06 | 0x16 | 0x0e | 0x1e => {
                    self.asl(&opcode.mode);
                }

                /*ROL*/ 0x2a => self.rol_accumulator(),

                /* ROL */
                0x26 | 0x36 | 0x2e | 0x3e => {
                    self.rol(&opcode.mode);
                }

                /* ROR */ 0x6a => self.ror_accumulator(),

                /* ROR */
                0x66 | 0x76 | 0x6e | 0x7e => {
                    self.ror(&opcode.mode);
                }

                /* INC */
                0xe6 | 0xf6 | 0xee | 0xfe => {
                    self.inc(&opcode.mode);
                }

                /* INY */
                0xc8 => self.iny(),

                /* DEC */
                0xc6 | 0xd6 | 0xce | 0xde => {
                    self.dec(&opcode.mode);
                }

                /* DEX */
                0xca => {
                    self.dex();
                }

                /* DEY */
                0x88 => {
                    self.dey();
                }

                /* CMP */
                0xc9 | 0xc5 | 0xd5 | 0xcd | 0xdd | 0xd9 | 0xc1 | 0xd1 => {
                    self.compare(&opcode.mode, self.reg_a);
                }

                /* CPY */
                0xc0 | 0xc4 | 0xcc => {
                    self.compare(&opcode.mode, self.reg_y);
                }

                /* CPX */
                0xe0 | 0xe4 | 0xec => self.compare(&opcode.mode, self.reg_x),

                /* JMP Absolute */
                0x4c => {
                    let mem_address = self.mem_read_u16(self.reg_pc);
                    self.reg_pc = mem_address;
                }

                /* JMP Indirect */
                0x6c => {
                    let mem_address = self.mem_read_u16(self.reg_pc);
                    let indirect_ref = if mem_address & 0x00FF == 0x00FF {
                        let lo = self.mem_read(mem_address);
                        let hi = self.mem_read(mem_address & 0xFF00);
                        (hi as u16) << 8 | (lo as u16)
                    } else {
                        self.mem_read_u16(mem_address)
                    };

                    self.reg_pc = indirect_ref;
                }

                /* JSR */
                0x20 => {
                    self.stack_push_u16(self.reg_pc + 2 - 1);
                    let target_address = self.mem_read_u16(self.reg_pc);
                    self.reg_pc = target_address
                }

                /* RTS */
                0x60 => {
                    self.reg_pc = self.stack_pop_u16() + 1;
                }

                /* RTI */
                0x40 => {
                    self.status.bits = self.stack_pop();
                    self.status.remove(CpuFlags::BREAK);
                    self.status.insert(CpuFlags::BREAK2);

                    self.reg_pc = self.stack_pop_u16();
                }

                /* BNE */
                0xd0 => {
                    self.branch(!self.status.contains(CpuFlags::ZERO));
                }

                /* BVS */
                0x70 => {
                    self.branch(self.status.contains(CpuFlags::OVERFLOW));
                }

                /* BVC */
                0x50 => {
                    self.branch(!self.status.contains(CpuFlags::OVERFLOW));
                }

                /* BPL */
                0x10 => {
                    self.branch(!self.status.contains(CpuFlags::NEGATIV));
                }

                /* BMI */
                0x30 => {
                    self.branch(self.status.contains(CpuFlags::NEGATIV));
                }

                /* BEQ */
                0xf0 => {
                    self.branch(self.status.contains(CpuFlags::ZERO));
                }

                /* BCS */
                0xb0 => {
                    self.branch(self.status.contains(CpuFlags::CARRY));
                }

                /* BCC */
                0x90 => {
                    self.branch(!self.status.contains(CpuFlags::CARRY));
                }

                /* BIT */
                0x24 | 0x2c => {
                    self.bit(&opcode.mode);
                }

                /* STA */
                0x85 | 0x95 | 0x8d | 0x9d | 0x99 | 0x81 | 0x91 => {
                    self.sta(&opcode.mode);
                }

                /* STX */
                0x86 | 0x96 | 0x8e => {
                    let addr = self.get_operand_address(&opcode.mode);
                    self.mem_write(addr, self.reg_x);
                }

                /* STY */
                0x84 | 0x94 | 0x8c => {
                    let addr = self.get_operand_address(&opcode.mode);
                    self.mem_write(addr, self.reg_y);
                }

                /* LDX */
                0xa2 | 0xa6 | 0xb6 | 0xae | 0xbe => {
                    self.ldx(&opcode.mode);
                }

                /* LDY */
                0xa0 | 0xa4 | 0xb4 | 0xac | 0xbc => {
                    self.ldy(&opcode.mode);
                }

                /* NOP */
                0xea => {
                    //do nothing
                }

                /* TAY */
                0xa8 => {
                    self.reg_y = self.reg_a;
                    self.update_zero_and_negative_flags(self.reg_y);
                }

                /* TSX */
                0xba => {
                    self.reg_x = self.reg_sp;
                    self.update_zero_and_negative_flags(self.reg_x);
                }

                /* TXA */
                0x8a => {
                    self.reg_a = self.reg_x;
                    self.update_zero_and_negative_flags(self.reg_a);
                }

                /* TXS */
                0x9a => {
                    self.reg_sp = self.reg_x;
                }

                /* TYA */
                0x98 => {
                    self.reg_a = self.reg_y;
                    self.update_zero_and_negative_flags(self.reg_a);
                }

                /* unofficial */

                /* DCP */
                0xc7 | 0xd7 | 0xCF | 0xdF | 0xdb | 0xd3 | 0xc3 => {
                    let addr = self.get_operand_address(&opcode.mode);
                    let mut data = self.mem_read(addr);
                    data = data.wrapping_sub(1);
                    self.mem_write(addr, data);
                    // self._update_zero_and_negative_flags(data);
                    if data <= self.reg_a {
                        self.status.insert(CpuFlags::CARRY);
                    }

                    self.update_zero_and_negative_flags(self.reg_a.wrapping_sub(data));
                }

                /* RLA */
                0x27 | 0x37 | 0x2F | 0x3F | 0x3b | 0x33 | 0x23 => {
                    let data = self.rol(&opcode.mode);
                    self.and_with_reg_a(data);
                }

                /* SLO */
                0x07 | 0x17 | 0x0F | 0x1f | 0x1b | 0x03 | 0x13 => {
                    let data = self.asl(&opcode.mode);
                    self.or_with_reg_a(data);
                }

                /* SRE */
                0x47 | 0x57 | 0x4F | 0x5f | 0x5b | 0x43 | 0x53 => {
                    let data = self.lsr(&opcode.mode);
                    self.xor_with_reg_a(data);
                }

                /* SKB */
                0x80 | 0x82 | 0x89 | 0xc2 | 0xe2 => {
                    /* 2 byte NOP (immidiate ) */
                    // todo: might be worth doing the read
                }

                /* AXS */
                0xCB => {
                    let addr = self.get_operand_address(&opcode.mode);
                    let data = self.mem_read(addr);
                    let x_and_a = self.reg_x & self.reg_a;
                    let result = x_and_a.wrapping_sub(data);

                    if data <= x_and_a {
                        self.status.insert(CpuFlags::CARRY);
                    }
                    self.update_zero_and_negative_flags(result);

                    self.reg_x = result;
                }

                /* ARR */
                0x6B => {
                    let addr = self.get_operand_address(&opcode.mode);
                    let data = self.mem_read(addr);
                    self.and_with_reg_a(data);
                    self.ror_accumulator();
                    //todo: registers
                    let result = self.reg_a;
                    let bit_5 = (result >> 5) & 1;
                    let bit_6 = (result >> 6) & 1;

                    if bit_6 == 1 {
                        self.status.insert(CpuFlags::CARRY)
                    } else {
                        self.status.remove(CpuFlags::CARRY)
                    }

                    if bit_5 ^ bit_6 == 1 {
                        self.status.insert(CpuFlags::OVERFLOW);
                    } else {
                        self.status.remove(CpuFlags::OVERFLOW);
                    }

                    self.update_zero_and_negative_flags(result);
                }

                /* unofficial SBC */
                0xeb => {
                    let addr = self.get_operand_address(&opcode.mode);
                    let data = self.mem_read(addr);
                    self.sub_from_reg_a(data);
                }

                /* ANC */
                0x0b | 0x2b => {
                    let addr = self.get_operand_address(&opcode.mode);
                    let data = self.mem_read(addr);
                    self.and_with_reg_a(data);
                    if self.status.contains(CpuFlags::NEGATIV) {
                        self.status.insert(CpuFlags::CARRY);
                    } else {
                        self.status.remove(CpuFlags::CARRY);
                    }
                }

                /* ALR */
                0x4b => {
                    let addr = self.get_operand_address(&opcode.mode);
                    let data = self.mem_read(addr);
                    self.and_with_reg_a(data);
                    self.lsr_accumulator();
                }

                /* NOP read */
                0x04 | 0x44 | 0x64 | 0x14 | 0x34 | 0x54 | 0x74 | 0xd4 | 0xf4 | 0x0c | 0x1c
                | 0x3c | 0x5c | 0x7c | 0xdc | 0xfc => {
                    let addr = self.get_operand_address(&opcode.mode);
                    let _data = self.mem_read(addr);
                    /* do nothing */
                }

                /* RRA */
                0x67 | 0x77 | 0x6f | 0x7f | 0x7b | 0x63 | 0x73 => {
                    let data = self.ror(&opcode.mode);
                    self.add_to_reg_a(data);
                }

                /* ISB */
                0xe7 | 0xf7 | 0xef | 0xff | 0xfb | 0xe3 | 0xf3 => {
                    let data = self.inc(&opcode.mode);
                    self.sub_from_reg_a(data);
                }

                /* NOPs */
                0x02 | 0x12 | 0x22 | 0x32 | 0x42 | 0x52 | 0x62 | 0x72 | 0x92 | 0xb2 | 0xd2
                | 0xf2 => { /* do nothing */ }

                0x1a | 0x3a | 0x5a | 0x7a | 0xda | 0xfa => { /* do nothing */ }

                /* LAX */
                0xa7 | 0xb7 | 0xaf | 0xbf | 0xa3 | 0xb3 => {
                    let addr = self.get_operand_address(&opcode.mode);
                    let data = self.mem_read(addr);
                    self.set_reg_a(data);
                    self.reg_x = self.reg_a;
                }

                /* SAX */
                0x87 | 0x97 | 0x8f | 0x83 => {
                    let data = self.reg_a & self.reg_x;
                    let addr = self.get_operand_address(&opcode.mode);
                    self.mem_write(addr, data);
                }

                /* LXA */
                0xab => {
                    self.lda(&opcode.mode);
                    self.tax();
                }

                /* XAA */
                0x8b => {
                    self.reg_a = self.reg_x;
                    self.update_zero_and_negative_flags(self.reg_a);
                    let addr = self.get_operand_address(&opcode.mode);
                    let data = self.mem_read(addr);
                    self.and_with_reg_a(data);
                }

                /* LAS */
                0xbb => {
                    let addr = self.get_operand_address(&opcode.mode);
                    let mut data = self.mem_read(addr);
                    data &= self.reg_sp;
                    self.reg_a = data;
                    self.reg_x = data;
                    self.reg_sp = data;
                    self.update_zero_and_negative_flags(data);
                }

                /* TAS */
                0x9b => {
                    let data = self.reg_a & self.reg_x;
                    self.reg_sp = data;
                    let mem_address = self.mem_read_u16(self.reg_pc) + self.reg_y as u16;

                    let data = ((mem_address >> 8) as u8 + 1) & self.reg_sp;
                    self.mem_write(mem_address, data)
                }

                /* AHX  Indirect Y */
                0x93 => {
                    let pos: u8 = self.mem_read(self.reg_pc);
                    let mem_address = self.mem_read_u16(pos as u16) + self.reg_y as u16;
                    let data = self.reg_a & self.reg_x & (mem_address >> 8) as u8;
                    self.mem_write(mem_address, data)
                }

                /* AHX Absolute Y*/
                0x9f => {
                    let mem_address = self.mem_read_u16(self.reg_pc) + self.reg_y as u16;

                    let data = self.reg_a & self.reg_x & (mem_address >> 8) as u8;
                    self.mem_write(mem_address, data)
                }

                /* SHX */
                0x9e => {
                    let mem_address = self.mem_read_u16(self.reg_pc) + self.reg_y as u16;

                    // todo if cross page boundry {
                    //     mem_address &= (self.x as u16) << 8;
                    // }
                    let data = self.reg_x & ((mem_address >> 8) as u8 + 1);
                    self.mem_write(mem_address, data)
                }

                /* SHY */
                0x9c => {
                    let mem_address = self.mem_read_u16(self.reg_pc) + self.reg_x as u16;
                    let data = self.reg_y & ((mem_address >> 8) as u8 + 1);
                    self.mem_write(mem_address, data)
                }

                _ => todo!(),
            }

            //busのcyclesを進める
            self.bus.tick(opcode.cycles);

            //program counterを進める
            if program_counter_state == self.reg_pc {
                self.reg_pc += (opcode.len - 1) as u16;
            }

            callback(self);
        }
    }
}
