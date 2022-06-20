#![allow(dead_code)]

#[derive(Debug)]
#[allow(clippy::upper_case_acronyms)]
pub enum Function<'p> {
    GraphicChar(char),
    Unsupported,
    Invalid,

    // C0 set
    NUL,
    SOH,
    STX,
    EOT,
    ENQ,
    ACK,
    BEL,
    BS,
    HT,
    LF,
    VT,
    FF,
    CR,
    SO,
    SI,
    DLE,
    DC1,
    DC2,
    DC3,
    DC4,
    NAK,
    SYN,
    ETB,
    CAN,
    EM,
    SUB,
    ESC,
    IS4,
    IS3,
    IS2,
    IS1,

    // C1 set
    BPH,
    NBH,
    NEL,
    SSA,
    ESA,
    HTS,
    HTJ,
    VTS,
    PLD,
    PLU,
    RI,
    SS2,
    SS3,
    DCS,
    PU1,
    PU2,
    STS,
    CCH,
    MW,
    SPA,
    EPA,
    SOS,
    SCI,
    ST,
    OSC,
    PM,
    APC,

    // Control Sequence (w/o intermediate bytes)
    ICH,
    CUU(u16),
    CUD(u16),
    CUF(u16),
    CUB(u16),
    CNL,
    CPL,
    CHA,
    CUP(u16, u16),
    CHT,
    ED(u16),
    EL(u16),
    IL,
    DL,
    EF,
    EA,
    DCH,
    SSE,
    CPR,
    SU,
    SD,
    NP,
    PP,
    CTC,
    ECH(u16),
    CVT,
    CBT,
    SRS,
    PTX,
    SDS,
    SIMD,
    HPA,
    HPR,
    REP,
    DA,
    VPA,
    VPR,
    HVP,
    TBC,
    SM,
    MC,
    HPB,
    VPB,
    RM,
    SGR(&'p [u16]),
    DSR,
    DAQ,

    // Control Sequence (w/ a single intermediate byte 0x20)
    SL,
    SR,
    GSM,
    GSS,
    FNT,
    TSS,
    JFY,
    SPI,
    QUAD,
    SSU,
    PFS,
    SHS,
    SVS,
    IGS,
    IDCS,
    PPA,
    PPR,
    PPB,
    SPD,
    DTA,
    SHL,
    SLL,
    FNK,
    SPQR,
    SEF,
    PEC,
    SSW,
    SACS,
    SAPV,
    STAB,
    GCC,
    TATE,
    TALE,
    TAC,
    TCC,
    TSR,
    SCO,
    SRCS,
    SCS,
    SLS,
    SCP,
}

struct Buffer {
    params: Vec<u16>,
    intermediate: u8,
}

impl Default for Buffer {
    fn default() -> Self {
        let mut buf = Self {
            params: Vec::with_capacity(16),
            intermediate: 0,
        };
        buf.clear();
        buf
    }
}

impl Buffer {
    fn clear(&mut self) {
        self.params.clear();
        self.params.push(0); // default value
        self.intermediate = 0;
    }
}

enum State {
    Normal,
    EscapeSeq,
    ControlSeq,
}

impl State {
    fn parse_normal<'b>(&mut self, ch: char) -> Option<Function<'b>> {
        match ch {
            '\x00' => None,
            '\x01' => None,
            '\x02' => None,
            '\x03' => None,
            '\x04' => None,
            '\x05' => None,
            '\x06' => None,
            '\x07' => Some(Function::BEL),
            '\x08' => Some(Function::BS),
            '\x09' => Some(Function::HT),
            '\x0A' => Some(Function::LF),
            '\x0B' => Some(Function::VT),
            '\x0C' => Some(Function::FF),
            '\x0D' => Some(Function::CR),
            '\x0E' => Some(Function::SO),
            '\x0F' => Some(Function::SI),
            '\x10' => None,
            '\x11' => Some(Function::DC1),
            '\x12' => None,
            '\x13' => Some(Function::DC3),
            '\x14' => None,
            '\x15' => None,
            '\x16' => None,
            '\x17' => None,
            '\x18' => Some(Function::CAN),
            '\x19' => None,
            '\x1A' => Some(Function::SUB),
            '\x1B' => {
                *self = State::EscapeSeq;
                None
            }
            '\x7F' => None,

            _ => Some(Function::GraphicChar(ch)),
        }
    }

    fn parse_escape_sequence<'b>(&mut self, ch: char) -> Option<Function<'b>> {
        match ch {
            // Restart
            '\x1B' => None,

            // CSI
            '[' => {
                *self = State::ControlSeq;
                None
            }

            _ => None,
        }
    }

    fn parse_control_sequence<'b>(
        &mut self,
        ch: char,
        buf: &'b mut Buffer,
    ) -> Option<Function<'b>> {
        use Function::*;
        match ch {
            // Restart
            '\x1B' => {
                buf.clear();
                *self = State::EscapeSeq;
                None
            }

            // parameter sub-string
            '0'..='9' => {
                let digit = ch.to_digit(10).unwrap() as u16;
                let last_param = buf.params.last_mut().unwrap();
                *last_param = (*last_param) * 10 + digit;
                None
            }
            ':' => {
                log::warn!("a separator in a parameter sub-string is not supported");
                Some(Unsupported)
            }

            // parameter separator
            ';' => {
                buf.params.push(0);
                None
            }

            // intermediate bytes
            '\x20'..='\x2F' => {
                buf.intermediate = ch as u8;
                None
            }

            // final bytes (w/o intermediate bytes)
            fin @ '\x40'..='\x7E' if buf.intermediate == 0 => {
                let ps = buf.params.as_slice();
                match (fin, ps) {
                    ('\x40', _) => Some(ICH),
                    ('\x41', &[pn]) => Some(CUU(pn)),
                    ('\x42', &[pn]) => Some(CUD(pn)),
                    ('\x43', &[pn]) => Some(CUF(pn)),
                    ('\x44', &[pn]) => Some(CUB(pn)),
                    ('\x45', _) => Some(CNL),
                    ('\x46', _) => Some(CPL),
                    ('\x47', _) => Some(CHA),
                    ('\x48', &[pn1, pn2]) => Some(CUP(pn1, pn2)),
                    ('\x49', _) => Some(CHT),
                    ('\x4A', &[ps @ 0..=2]) => Some(ED(ps)),
                    ('\x4B', &[ps @ 0..=2]) => Some(EL(ps)),
                    ('\x4C', _) => Some(IL),
                    ('\x4D', _) => Some(DL),
                    ('\x4E', _) => Some(EF),
                    ('\x4F', _) => Some(EA),

                    ('\x50', _) => Some(DCH),
                    ('\x51', _) => Some(SSE),
                    ('\x52', _) => Some(CPR),
                    ('\x53', _) => Some(SU),
                    ('\x54', _) => Some(SD),
                    ('\x55', _) => Some(NP),
                    ('\x56', _) => Some(PP),
                    ('\x57', _) => Some(CTC),
                    ('\x58', &[pn]) => Some(ECH(pn)),
                    ('\x59', _) => Some(CVT),
                    ('\x5A', _) => Some(CBT),
                    ('\x5B', _) => Some(SRS),
                    ('\x5C', _) => Some(PTX),
                    ('\x5D', _) => Some(SDS),
                    ('\x5E', _) => Some(SIMD),
                    ('\x5F', _) => Some(Unsupported),

                    ('\x60', _) => Some(HPA),
                    ('\x61', _) => Some(HPR),
                    ('\x62', _) => Some(REP),
                    ('\x63', _) => Some(DA),
                    ('\x64', _) => Some(VPA),
                    ('\x65', _) => Some(VPR),
                    ('\x66', _) => Some(HVP),
                    ('\x67', _) => Some(TBC),
                    ('\x68', _) => Some(SM),
                    ('\x69', _) => Some(MC),
                    ('\x6A', _) => Some(HPB),
                    ('\x6B', _) => Some(VPB),
                    ('\x6C', _) => Some(RM),
                    ('\x6D', _) => Some(SGR(ps)),
                    ('\x6E', _) => Some(DSR),
                    ('\x6F', _) => Some(DAQ),

                    ('\x70'..='\x7E', _) => {
                        log::trace!("undefined private sequence");
                        Some(Unsupported)
                    }

                    _ => Some(Invalid),
                }
            }

            // final bytes (w/ a single intermediate byte 0x20)
            fin @ '\x40'..='\x7E' if buf.intermediate == b'\x20' => {
                let ps = buf.params.as_slice();
                match (fin, ps) {
                    ('\x40', _) => Some(SL),
                    ('\x41', _) => Some(SR),
                    ('\x42', _) => Some(GSM),
                    ('\x43', _) => Some(GSS),
                    ('\x44', _) => Some(FNT),
                    ('\x45', _) => Some(TSS),
                    ('\x46', _) => Some(JFY),
                    ('\x47', _) => Some(SPI),
                    ('\x48', _) => Some(QUAD),
                    ('\x49', _) => Some(SSU),
                    ('\x4A', _) => Some(PFS),
                    ('\x4B', _) => Some(SHS),
                    ('\x4C', _) => Some(SVS),
                    ('\x4D', _) => Some(IGS),
                    ('\x4E', _) => Some(Unsupported),
                    ('\x4F', _) => Some(IDCS),

                    ('\x50', _) => Some(PPA),
                    ('\x51', _) => Some(PPR),
                    ('\x52', _) => Some(PPB),
                    ('\x53', _) => Some(SPD),
                    ('\x54', _) => Some(DTA),
                    ('\x55', _) => Some(SHL),
                    ('\x56', _) => Some(SLL),
                    ('\x57', _) => Some(FNK),
                    ('\x58', _) => Some(SPQR),
                    ('\x59', _) => Some(SEF),
                    ('\x5A', _) => Some(PEC),
                    ('\x5B', _) => Some(SSW),
                    ('\x5C', _) => Some(SACS),
                    ('\x5D', _) => Some(SAPV),
                    ('\x5E', _) => Some(STAB),
                    ('\x5F', _) => Some(GCC),

                    ('\x60', _) => Some(TATE),
                    ('\x61', _) => Some(TALE),
                    ('\x62', _) => Some(TAC),
                    ('\x63', _) => Some(TCC),
                    ('\x64', _) => Some(TSR),
                    ('\x65', _) => Some(SCO),
                    ('\x66', _) => Some(SRCS),
                    ('\x67', _) => Some(SCS),
                    ('\x68', _) => Some(SLS),
                    ('\x69', _) => Some(Unsupported),
                    ('\x6A', _) => Some(Unsupported),
                    ('\x6B', _) => Some(SCP),
                    ('\x6C', _) => Some(Unsupported),
                    ('\x6D', _) => Some(Unsupported),
                    ('\x6E', _) => Some(Unsupported),
                    ('\x6F', _) => Some(Unsupported),

                    ('\x70'..='\x7E', _) => {
                        log::trace!("undefined private sequence");
                        Some(Unsupported)
                    }

                    _ => unreachable!(),
                }
            }

            '\x40'..='\x7E' => {
                log::trace!("unsupported control sequence");
                Some(Unsupported)
            }

            _ => {
                log::warn!("invalid control sequence");
                Some(Invalid)
            }
        }
    }

    fn feed<'b>(&mut self, ch: char, buf: &'b mut Buffer) -> Option<Function<'b>> {
        let func = match self {
            State::Normal => {
                buf.clear();
                self.parse_normal(ch)
            }
            State::EscapeSeq => self.parse_escape_sequence(ch),
            State::ControlSeq => self.parse_control_sequence(ch, buf),
        };

        if func.is_some() {
            *self = State::Normal;
        }

        func
    }
}

pub struct Parser {
    state: State,
    buf: Buffer,
}

impl Parser {
    pub fn feed(&mut self, ch: char) -> Option<Function> {
        self.state.feed(ch, &mut self.buf)
    }
}

impl Default for Parser {
    fn default() -> Self {
        Self {
            state: State::Normal,
            buf: Buffer::default(),
        }
    }
}
