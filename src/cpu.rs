// Reference: https://www.nesdev.org/obelisk-6502-guide/reference.html

use crate::opcodes::CPU_OPS_CODES;

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

bitflags! {
        // Status flags -- https://www.nesdev.org/wiki/Status_flags
    // 7654 3210
    // NV0B DIZC
    // |||| ||||
    // |||| |||+- Carry
    // |||| ||+-- Zero
    // |||| |+--- Interrupt Disable
    // |||| +---- Decimal
    // |||+------ (No CPU effect; see: the B flag)
    // ||+------- (No CPU effect; always pushed as 0)
    // |+-------- Overflow
    // +--------- Negative
    pub struct CPUFlags: u8 {
        const CARRY             = 0b00000001;
        const ZERO              = 0b00000010;
        const INTERRUPT_DISABLE = 0b00000100;
        const DECIMAL_MODE      = 0b00001000;
        const BREAK             = 0b00010000;
        const BREAK2            = 0b00100000; // not used
        const OVERFLOW          = 0b01000000;
        const NEGATIVE          = 0b10000000;
    }
}
pub struct CPU {
    pub register_a: u8,
    pub status: CPUFlags,
    pub register_x: u8,
    pub register_y: u8,
    pub program_counter: u16,
    pub stack_pointer: u8,
    memory: [u8; 0xFFFF]
}

// Stack occupied 0x0100 -> 0x01FF
const STACK: u16 = 0x0100;
// STACK + STACK_RESET is "top" of stack
const STACK_RESET: u8 = 0xfd;

impl CPU {
    pub fn new() -> Self {
        CPU {
            register_a: 0,
            register_x: 0,
            register_y: 0,
            program_counter: 0,
            stack_pointer: 0,
            // interrupt distable and negative initialized
            status: CPUFlags::from_bits_truncate(0b100100),
            memory: [0; 0xFFFF],
        }
    }

    fn get_operand_address(&mut self, mode: &AddressingMode) -> u16 {
        match mode {
            AddressingMode::Immediate => self.program_counter,
            AddressingMode::ZeroPage => self.mem_read(self.program_counter) as u16,
            AddressingMode::Absolute => self.mem_read_u16(self.program_counter),
            AddressingMode::ZeroPage_X => self.mem_read(self.program_counter).wrapping_add(self.register_x) as u16,
            AddressingMode::ZeroPage_Y => self.mem_read(self.program_counter).wrapping_add(self.register_y) as u16,
            AddressingMode::Absolute_X => self.mem_read_u16(self.program_counter).wrapping_add(self.register_x as u16),
            AddressingMode::Absolute_Y => self.mem_read_u16(self.program_counter).wrapping_add(self.register_y as u16),
            AddressingMode::Indirect_X => {
                let base = self.mem_read(self.program_counter);
 
                let ptr: u8 = (base as u8).wrapping_add(self.register_x);
                let lo = self.mem_read(ptr as u16);
                let hi = self.mem_read(ptr.wrapping_add(1) as u16);
                (hi as u16) << 8 | (lo as u16)
            }
            AddressingMode::Indirect_Y => {
                let base = self.mem_read(self.program_counter);
 
                let lo = self.mem_read(base as u16);
                let hi = self.mem_read((base as u8).wrapping_add(1) as u16);
                let deref_base = (hi as u16) << 8 | (lo as u16);
                let deref = deref_base.wrapping_add(self.register_y as u16);
                deref
            }
            AddressingMode::NoneAddressing => {
                panic!("mode {:?} is not supported", mode);
            }
        }
    }

    // Reads 8 bits.
    fn mem_read(&self, addr: u16) -> u8 {
        self.memory[addr as usize]
    }

    fn mem_write(&mut self, addr: u16, data: u8) {
        self.memory[addr as usize] = data;
    }

    // Converts little-endian (used by NES) to big-endian
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
    
    pub fn reset(&mut self) {
        self.register_a = 0;
        self.register_x = 0;
        self.register_y = 0;
        self.stack_pointer = STACK_RESET;
        self.status = CPUFlags::from_bits_truncate(0b100100);
 
        self.program_counter = self.mem_read_u16(0xFFFC);
    }

    pub fn load(&mut self, program: Vec<u8>) {
        // 0x8000 to 0xFFFF stores program ROM
       self.memory[0x8000 .. (0x8000 + program.len())].copy_from_slice(&program[..]);
       self.mem_write_u16(0xFFFC, 0x8000);
    }

    pub fn load_and_run(&mut self, program: Vec<u8>) {
       self.load(program);
       self.reset();
       self.run();
    }

    fn stack_pop(&mut self) -> u8 {
        self.stack_pointer = self.stack_pointer.wrapping_add(1);
        self.mem_read((STACK as u16) + self.stack_pointer as u16)
    }

    fn stack_push(&mut self, data: u8) {
        self.mem_write((STACK as u16) + self.stack_pointer as u16, data);
        self.stack_pointer = self.stack_pointer.wrapping_sub(1)
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

    fn and(&mut self, mode: &AddressingMode) {
        let addr = self.get_operand_address(mode);
        self.register_a &= self.mem_read(addr);
        self.update_zero_and_negative_flags(self.register_a); // Unsure... documentation is too vague
    }

    fn asl(&mut self, mode: &AddressingMode) {
        let mut data;
        let addr = self.get_operand_address(mode);
        // AddressingNone => Accumulator
        match mode {
            AddressingMode::NoneAddressing => data = self.register_a,
            _ => data = self.mem_read(addr),
        }
        if data >> 7 == 1 {
            self.status.insert(CPUFlags::CARRY);
        } else {
            self.status.remove(CPUFlags::CARRY);
        }
        data <<= 1;
        match mode {
            AddressingMode::NoneAddressing => self.register_a = data,
            _ => self.mem_write(addr, data),
        }
        self.update_zero_and_negative_flags(data);
    }

    fn eor(&mut self, mode: &AddressingMode) {
        let addr = self.get_operand_address(mode);
        self.register_a ^= self.mem_read(addr);
        self.update_zero_and_negative_flags(self.register_a); // Unsure... documentation is too vague
    }

    fn dec(&mut self, mode: &AddressingMode){
        let addr = self.get_operand_address(mode);
        let val = self.mem_read(addr).wrapping_sub(1);

        self.mem_write(addr, val);
        self.update_zero_and_negative_flags(val);
    }

    fn dex(&mut self) {
        self.register_x = self.register_x.wrapping_sub(1);
        self.update_zero_and_negative_flags(self.register_x)
    }

    fn dey(&mut self) {
        self.register_y = self.register_y.wrapping_sub(1);
        self.update_zero_and_negative_flags(self.register_y)
    }

    fn sta(&mut self, mode: &AddressingMode) {
        let addr = self.get_operand_address(mode);
        self.mem_write(addr, self.register_a);
    }

    fn stx(&mut self, mode: &AddressingMode) {
        let addr = self.get_operand_address(mode);
        self.mem_write(addr, self.register_x);
    }

    fn sty(&mut self, mode: &AddressingMode) {
        let addr = self.get_operand_address(mode);
        self.mem_write(addr, self.register_y);
    }

    fn lda(&mut self, mode: &AddressingMode) {
        let addr = self.get_operand_address(mode);
        let val = self.mem_read(addr);

        self.register_a = val;
        self.update_zero_and_negative_flags(self.register_a);
    }

    fn ldx(&mut self, mode: &AddressingMode) {
        let addr = self.get_operand_address(mode);
        let val = self.mem_read(addr);

        self.register_x = val;
        self.update_zero_and_negative_flags(self.register_x);
    }


    fn ldy(&mut self, mode: &AddressingMode) {
        let addr = self.get_operand_address(mode);
        let val = self.mem_read(addr);

        self.register_y = val;
        self.update_zero_and_negative_flags(self.register_y);
    }


    fn ora(&mut self, mode: &AddressingMode) {
        let addr = self.get_operand_address(mode);
        let val = self.mem_read(addr);

        self.register_a |= val;
        self.update_zero_and_negative_flags(self.register_a);
    }

    fn pla(&mut self) {
        let data = self.stack_pop();
        self.register_a = data;
    }

    fn tax(&mut self) {
        self.register_x = self.register_a;
        self.update_zero_and_negative_flags(self.register_x);
    }

    fn tay(&mut self) {
        self.register_y = self.register_a;
        self.update_zero_and_negative_flags(self.register_y);
    }

    fn tsx(&mut self) {
        self.register_x = self.stack_pointer;
        self.update_zero_and_negative_flags(self.register_x);
    }

    fn txa(&mut self) {
        self.register_a = self.register_x;
        self.update_zero_and_negative_flags(self.register_a);
    }

    fn tya(&mut self) {
        self.register_a = self.register_y;
        self.update_zero_and_negative_flags(self.register_y);
    }

    fn inc(&mut self, mode: &AddressingMode) {
        let addr = self.get_operand_address(mode);
        let val = self.mem_read(addr);

        self.mem_write(addr, val.wrapping_add(1));
        self.update_zero_and_negative_flags(self.register_x);
    }

    fn inx(&mut self) {
        self.register_x = self.register_x.wrapping_add(1);
        self.update_zero_and_negative_flags(self.register_x);
    }

    fn iny(&mut self) {
        self.register_y = self.register_y.wrapping_add(1);
        self.update_zero_and_negative_flags(self.register_y);
    }

    fn update_zero_and_negative_flags(&mut self, result: u8) {
        if result == 0 {
            self.status.insert(CPUFlags::ZERO); 
        } else {
            self.status.remove(CPUFlags::ZERO);
        }

        if result & 0b1000_0000 != 0 {
            self.status.insert(CPUFlags::NEGATIVE);
        } else {
            self.status.remove(CPUFlags::NEGATIVE);
        }
    }

    pub fn run(&mut self) {
        loop {
            let code = self.mem_read(self.program_counter);
            self.program_counter += 1;

            let opcode = CPU_OPS_CODES.iter().find(|opcode| opcode.code == code).expect("Invalid code");

            match opcode.op {
                "ADC" => todo!(),
                "AND" => self.and(&opcode.addressing_mode),
                "ASL" => self.asl(&opcode.addressing_mode),
                "BCC" => todo!(),
                "BCS" => todo!(),
                "BEQ" => todo!(),
                "BIT" => todo!(),
                "BMI" => todo!(),
                "BNE" => todo!(),
                "BPL" => todo!(),
                "BRK" => return,
                "BVC" => todo!(),
                "BVS" => todo!(),
                "CLC" => self.status.remove(CPUFlags::CARRY),
                "CLD" => self.status.remove(CPUFlags::DECIMAL_MODE),
                "CLI" => self.status.remove(CPUFlags::INTERRUPT_DISABLE),
                "CLV" => self.status.remove(CPUFlags::OVERFLOW),
                "CMP" => todo!(),
                "CPX" => todo!(),
                "CPY" => todo!(),
                "DEC" => self.dec(&opcode.addressing_mode),
                "DEX" => self.dex(),
                "DEY" => self.dey(),
                "EOR" => self.eor(&opcode.addressing_mode),
                "INC" => self.inc(&opcode.addressing_mode),
                "INX" => self.inx(),
                "INY" => self.iny(),
                "JMP" => todo!(),
                "JSR" => todo!(),
                "LDA" => self.lda(&opcode.addressing_mode),
                "LDX" => self.ldx(&opcode.addressing_mode),
                "LDY" => self.ldy(&opcode.addressing_mode),
                "LSR" => todo!(),
                "NOP" => (),
                "ORA" => self.ora(&opcode.addressing_mode),
                "PHA" => todo!(),
                "PHP" => todo!(),
                "PLA" => self.register_a = self.stack_pop(),
                "PLP" => todo!(), // what to do with breaks?
                "ROL" => todo!(),
                "ROR" => todo!(),
                "RTI" => todo!(),
                "RTS" => todo!(),
                "SBC" => todo!(),
                "SEC" => self.status.insert(CPUFlags::CARRY),
                "SED" => self.status.insert(CPUFlags::DECIMAL_MODE),
                "SEI" => self.status.insert(CPUFlags::INTERRUPT_DISABLE),
                "STA" => self.sta(&opcode.addressing_mode),
                "STX" => self.stx(&opcode.addressing_mode),
                "STY" => self.sty(&opcode.addressing_mode),
                "TAX" => self.tax(),
                "TAY" => self.tay(),
                "TSX" => self.tsx(),
                "TXA" => self.txa(),
                "TXS" => self.stack_pointer = self.register_x,
                "TYA" => self.tya(),
                _ => panic!("Invalid code"),
            }

            // -1 because we already incremented program_counter to account for the instruction
            self.program_counter += (opcode.bytes - 1) as u16;
        }
    }
}


#[cfg(test)]
mod test {
   use super::*;

   #[test]
   fn test_0xa9_lda_immediate_load_data() {
       let mut cpu = CPU::new();
       cpu.load_and_run(vec![0xa9, 0x05, 0x00]);
       assert_eq!(cpu.register_a, 0x05);
    //    assert!(cpu.status & 0b0000_0010 == 0b00);
    //    assert!(cpu.status & 0b1000_0000 == 0);
   }

    #[test]
    fn test_0xa9_lda_zero_flag() {
        let mut cpu = CPU::new();
        cpu.load_and_run(vec![0xa9, 0x00, 0x00]);
        // assert!(cpu.status & 0b0000_0010 == 0b10);
    }

    #[test]
    fn test_5_ops_working_together() {
        let mut cpu = CPU::new();

        cpu.load_and_run(vec![0xa9, 0xc0, 0xaa, 0xe8, 0x00]);
  
        assert_eq!(cpu.register_x, 0xc1)
    }
    #[test]
    fn test_inx_overflow() {
        let mut cpu = CPU::new();
        // LDA (0xff)
        // TAX
        // INX
        // INX
        // BRK
        cpu.load_and_run(vec![0xa9, 0xff, 0xaa, 0xe8, 0xe8, 0x00]);

        assert_eq!(cpu.register_x, 1)
    }    
    #[test]
    fn test_lda_from_memory() {
        let mut cpu = CPU::new();
        cpu.mem_write(0x10, 0x55);

        cpu.load_and_run(vec![0xa5, 0x10, 0x00]);

        assert_eq!(cpu.register_a, 0x55);
    }
    #[test]
    fn test_lda_sta_dec_and() {
        let mut cpu = CPU::new();
        cpu.load_and_run(vec![
            0xA9, 0b1010_0010,      // LDA
            0x85, 0x87,             // STA, store 0x87 -> 0b1010_0010
            0xC6, 0x87,             // DEC
            0xC6, 0x87,             // DEC, register A now = 0b1010_0000
            0x25, 0x87              // AND
        ]);

        assert_eq!(cpu.register_a, 0b1010_0000)
    }
    #[test]
    fn test_lda_eor_and() {
        let mut cpu = CPU::new();
        cpu.load_and_run(vec![
            0xA9, 0b0111_0110,      // LDA
            0x49, 0b1010_1100,      // EOR, A = 0b1101_1010
            0x29, 0b1010_1100,      // AND
        ]);

        assert_eq!(cpu.register_a, 0b1000_1000)
    }
    #[test]
    fn test_inc_ora() {
        let mut cpu = CPU::new();
        cpu.load_and_run(vec![
            0xE6, 0x26,             // INC
            0x05, 0x26              // ORA
        ]);

        assert_eq!(cpu.register_a, 1)
    }
}