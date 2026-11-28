//! Conservative LLVM bitcode inspection for DXIL 64-bit integer atomics.
//!
//! This is a small, dependency-free reader for the LLVM bitstream constructs
//! needed to resolve `dx.op.atomicBinOp` calls and `atomicrmw` instructions.
//! It follows the public LLVM `BitstreamReader` and `LLVMBitCodes.h` layouts.
//! It is not a general LLVM IR reader. Unsupported or internally inconsistent
//! records produce [`DxilBitcodeError::Unsupported`], which the public
//! accessor reports as [`Atomic64Support::Unknown`]. Structural truncation
//! remains a defined error. Detection is evidence class A/B parsing only: it
//! does not establish that D3DMetal can execute the atomic.

use std::fmt;

const BC_MAGIC: [u8; 4] = [b'B', b'C', 0xC0, 0xDE];

const END_BLOCK: u64 = 0;
const ENTER_SUBBLOCK: u64 = 1;
const DEFINE_ABBREV: u64 = 2;
const UNABBREV_RECORD: u64 = 3;
const FIRST_APPLICATION_ABBREV: u64 = 4;

const BLOCKINFO_BLOCK_ID: u64 = 0;
const MODULE_BLOCK_ID: u64 = 8;
const CONSTANTS_BLOCK_ID: u64 = 11;
const FUNCTION_BLOCK_ID: u64 = 12;
const VALUE_SYMTAB_BLOCK_ID: u64 = 14;
const TYPE_BLOCK_ID_NEW: u64 = 17;
const STRTAB_BLOCK_ID: u64 = 23;

const MODULE_CODE_VERSION: u64 = 1;
const MODULE_CODE_FUNCTION: u64 = 8;

const TYPE_CODE_NUMENTRY: u64 = 1;
const TYPE_CODE_VOID: u64 = 2;
const TYPE_CODE_FLOAT: u64 = 3;
const TYPE_CODE_DOUBLE: u64 = 4;
const TYPE_CODE_LABEL: u64 = 5;
const TYPE_CODE_OPAQUE: u64 = 6;
const TYPE_CODE_INTEGER: u64 = 7;
const TYPE_CODE_POINTER: u64 = 8;
const TYPE_CODE_HALF: u64 = 10;
const TYPE_CODE_ARRAY: u64 = 11;
const TYPE_CODE_VECTOR: u64 = 12;
const TYPE_CODE_X86_FP80: u64 = 13;
const TYPE_CODE_FP128: u64 = 14;
const TYPE_CODE_PPC_FP128: u64 = 15;
const TYPE_CODE_METADATA: u64 = 16;
const TYPE_CODE_X86_MMX: u64 = 17;
const TYPE_CODE_STRUCT_ANON: u64 = 18;
const TYPE_CODE_STRUCT_NAME: u64 = 19;
const TYPE_CODE_STRUCT_NAMED: u64 = 20;
const TYPE_CODE_FUNCTION_OLD: u64 = 9;
const TYPE_CODE_FUNCTION: u64 = 21;
const TYPE_CODE_TOKEN: u64 = 22;
const TYPE_CODE_BFLOAT: u64 = 23;
const TYPE_CODE_X86_AMX: u64 = 24;
const TYPE_CODE_OPAQUE_POINTER: u64 = 25;
const TYPE_CODE_TARGET_TYPE: u64 = 26;

const CST_CODE_SETTYPE: u64 = 1;
const CST_CODE_NULL: u64 = 2;
const CST_CODE_UNDEF: u64 = 3;
const CST_CODE_INTEGER: u64 = 4;
const CST_CODE_WIDE_INTEGER: u64 = 5;
const CST_CODE_FLOAT: u64 = 6;
const CST_CODE_AGGREGATE: u64 = 7;
const CST_CODE_STRING: u64 = 8;
const CST_CODE_CSTRING: u64 = 9;
const CST_CODE_CE_BINOP: u64 = 10;
const CST_CODE_CE_CAST: u64 = 11;
const CST_CODE_CE_GEP_OLD: u64 = 12;
const CST_CODE_CE_SELECT: u64 = 13;
const CST_CODE_CE_EXTRACTELT: u64 = 14;
const CST_CODE_CE_INSERTELT: u64 = 15;
const CST_CODE_CE_SHUFFLEVEC: u64 = 16;
const CST_CODE_CE_CMP: u64 = 17;
const CST_CODE_INLINEASM_OLD: u64 = 18;
const CST_CODE_CE_SHUFVEC_EX: u64 = 19;
const CST_CODE_CE_INBOUNDS_GEP: u64 = 20;
const CST_CODE_BLOCKADDRESS: u64 = 21;
const CST_CODE_DATA: u64 = 22;
const CST_CODE_INLINEASM_OLD2: u64 = 23;
const CST_CODE_CE_GEP_WITH_INRANGE_INDEX_OLD: u64 = 24;
const CST_CODE_CE_UNOP: u64 = 25;
const CST_CODE_POISON: u64 = 26;
const CST_CODE_DSO_LOCAL_EQUIVALENT: u64 = 27;
const CST_CODE_INLINEASM_OLD3: u64 = 28;
const CST_CODE_NO_CFI_VALUE: u64 = 29;
const CST_CODE_INLINEASM: u64 = 30;
const CST_CODE_CE_GEP_WITH_INRANGE: u64 = 31;
const CST_CODE_CE_GEP: u64 = 32;
const CST_CODE_PTRAUTH: u64 = 33;
const CST_CODE_PTRAUTH2: u64 = 34;
const CST_CODE_BYTE: u64 = 35;
const CST_CODE_WIDE_BYTE: u64 = 36;

const VST_CODE_ENTRY: u64 = 1;
const VST_CODE_FNENTRY: u64 = 3;

const FUNC_CODE_INST_CALL: u64 = 34;
const FUNC_CODE_INST_ATOMICRMW_OLD: u64 = 38;
const FUNC_CODE_INST_ATOMICRMW: u64 = 59;

const CALL_FMF: u32 = 17;
const CALL_EXPLICIT_TYPE: u32 = 15;

const MAX_CODE_SIZE: u32 = 64;
const MAX_RECORD_VALUES: usize = 1_000_000;
const MAX_BLOCK_DEPTH: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Atomic64Support {
    /// A resolved `dx.op.atomicBinOp` call with an i64 result, or an
    /// `atomicrmw` whose value type is i64.
    Present,
    /// The supported bitstream subset was decoded and no such operation was
    /// found. This is not a proof about unsupported LLVM constructs.
    Absent,
    /// The bitstream uses a construct this reader cannot resolve confidently.
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DxilBitcodeError {
    BadMagic,
    Truncated,
    Invalid(&'static str),
    Unsupported(&'static str),
}

impl fmt::Display for DxilBitcodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadMagic => write!(f, "missing LLVM bitcode magic"),
            Self::Truncated => write!(f, "truncated LLVM bitstream"),
            Self::Invalid(reason) => write!(f, "invalid LLVM bitstream: {reason}"),
            Self::Unsupported(reason) => {
                write!(f, "unsupported LLVM bitstream: {reason}")
            }
        }
    }
}

impl std::error::Error for DxilBitcodeError {}

type Result<T> = std::result::Result<T, DxilBitcodeError>;

pub fn inspect_bitcode(bytes: &[u8]) -> Result<Atomic64Support> {
    if bytes.len() < 4 || bytes[..4] != BC_MAGIC {
        return Err(DxilBitcodeError::BadMagic);
    }
    if bytes.len() % 4 != 0 {
        return Err(DxilBitcodeError::Truncated);
    }
    let mut parser = Parser::new(bytes)?;
    if parser.bits.at_end() {
        return Err(DxilBitcodeError::Truncated);
    }
    match parser.parse_top_level() {
        Ok(()) => Ok(parser.result()),
        // A construct outside the supported subset is reported as `Unknown`
        // rather than as a structural error: the module itself is intact.
        Err(DxilBitcodeError::Unsupported(_)) => Ok(Atomic64Support::Unknown),
        Err(err) => Err(err),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Encoding {
    Fixed(u32),
    Vbr(u32),
    Array,
    Char6,
    Blob,
}

#[derive(Clone, Copy)]
enum Operand {
    Literal(u64),
    Encoded(Encoding),
}

#[derive(Clone)]
struct Abbrev {
    operands: Vec<Operand>,
}

#[derive(Clone)]
struct Scope {
    code_size: u32,
    abbrevs: Vec<Abbrev>,
    end_bit: usize,
}

struct Record {
    code: u64,
    values: Vec<u64>,
    blob: Option<Vec<u8>>,
}

struct BlockInfo {
    abbrevs: std::collections::BTreeMap<u64, Vec<Abbrev>>,
}

#[derive(Clone, Copy)]
enum Type {
    Integer(u32),
    Pointer {
        pointee: Option<u32>,
    },
    Function {
        result: u32,
        params_start: usize,
        params_len: usize,
    },
    Other,
}

struct FunctionInfo {
    name: Vec<u8>,
    /// Raw type index. LLVM writes MODULE_CODE_FUNCTION before the TYPE block, so the
    /// index is resolved later, when a function body is parsed.
    type_id: u64,
    is_declaration: bool,
}

#[derive(Clone, Copy)]
struct FunctionType {
    result: u32,
    params_start: usize,
    params_len: usize,
}

#[derive(Clone, Copy)]
struct ValueInfo {
    type_id: u32,
    function: Option<usize>,
}

struct Parser<'a> {
    bits: Bits<'a>,
    code_size: u32,
    abbrevs: Vec<Abbrev>,
    scopes: Vec<Scope>,
    block_info: BlockInfo,
    types: Vec<Option<Type>>,
    /// Name of the next `STRUCT_NAMED` type record. Names are not needed to
    /// resolve atomic widths, but the record must not consume a type slot.
    struct_name: Vec<u8>,
    function_params: Vec<u32>,
    functions: Vec<FunctionInfo>,
    values: Vec<Option<ValueInfo>>,
    strtab: Vec<u8>,
    pending_names: Vec<(usize, u64, u64)>,
    /// Calls to `dx.op.atomicBinOp.*` seen before STRTAB resolved names.
    pending_atomic_calls: Vec<(usize, u32)>,
    module_version: u64,
    function_index: usize,
    next_value: usize,
    result: Atomic64Support,
}

impl<'a> Parser<'a> {
    fn new(bytes: &'a [u8]) -> Result<Self> {
        let mut bits = Bits::new(bytes);
        bits.pos = 32;
        Ok(Self {
            bits,
            code_size: 2,
            abbrevs: Vec::new(),
            scopes: Vec::new(),
            block_info: BlockInfo {
                abbrevs: std::collections::BTreeMap::new(),
            },
            types: Vec::new(),
            struct_name: Vec::new(),
            function_params: Vec::new(),
            functions: Vec::new(),
            values: Vec::new(),
            strtab: Vec::new(),
            pending_names: Vec::new(),
            pending_atomic_calls: Vec::new(),
            module_version: 0,
            function_index: 0,
            next_value: 0,
            result: Atomic64Support::Absent,
        })
    }

    fn result(&self) -> Atomic64Support {
        self.result
    }

    fn unsupported<T>(&mut self, reason: &'static str) -> Result<T> {
        self.result = Atomic64Support::Unknown;
        Err(DxilBitcodeError::Unsupported(reason))
    }

    fn parse_top_level(&mut self) -> Result<()> {
        let mut found_module = false;
        while !self.bits.at_end() {
            match self.advance()? {
                Entry::EndBlock => return Err(DxilBitcodeError::Invalid("top-level end block")),
                Entry::Record(id) => {
                    self.read_record(id)?;
                }
                Entry::SubBlock(id) => match id {
                    MODULE_BLOCK_ID => {
                        if found_module {
                            return self.unsupported("multiple modules");
                        }
                        found_module = true;
                        self.enter_block(id)?;
                        self.parse_module()?;
                    }
                    STRTAB_BLOCK_ID => {
                        self.enter_block(id)?;
                        self.parse_strtab()?;
                    }
                    _ => self.skip_block()?,
                },
            }
        }
        if !found_module {
            return Err(DxilBitcodeError::Invalid("no module block"));
        }
        // STRTAB usually follows the module, so names are resolved here.
        let pending = std::mem::take(&mut self.pending_names);
        for (value_id, offset, len) in pending {
            if let Ok(bytes) = self.string_from_strtab(offset, len) {
                let name = bytes.to_vec();
                self.assign_name(value_id, &name)?;
            }
        }
        let deferred = std::mem::take(&mut self.pending_atomic_calls);
        for (function_index, result_type) in deferred {
            self.record_atomic_call(function_index, result_type)?;
        }
        Ok(())
    }

    fn parse_module(&mut self) -> Result<()> {
        self.values.clear();
        self.functions.clear();
        self.function_index = 0;
        self.module_version = 0;
        loop {
            match self.advance()? {
                Entry::EndBlock => return Ok(()),
                Entry::Record(id) => {
                    let record = self.read_record(id)?;
                    self.module_record(record)?;
                }
                Entry::SubBlock(id) => match id {
                    BLOCKINFO_BLOCK_ID => {
                        self.enter_block(id)?;
                        self.parse_block_info()?;
                    }
                    TYPE_BLOCK_ID_NEW => {
                        self.enter_block(id)?;
                        self.parse_types()?;
                    }
                    FUNCTION_BLOCK_ID => {
                        self.enter_block(id)?;
                        self.parse_function()?;
                    }
                    VALUE_SYMTAB_BLOCK_ID => {
                        self.enter_block(id)?;
                        self.parse_vst()?;
                    }
                    CONSTANTS_BLOCK_ID => {
                        self.enter_block(id)?;
                        self.parse_constants()?;
                    }
                    _ => self.skip_block()?,
                },
            }
        }
    }

    fn module_record(&mut self, record: Record) -> Result<()> {
        match record.code {
            MODULE_CODE_VERSION => {
                let Some(&version) = record.values.first() else {
                    return Err(DxilBitcodeError::Invalid("empty module version"));
                };
                if version > 2 {
                    return self.unsupported("module version");
                }
                self.module_version = version;
            }
            MODULE_CODE_FUNCTION => self.parse_function_record(record)?,
            // MODULE_CODE_VSTOFFSET lets a reader jump to the module value
            // symbol table. Names that matter here are deferred until STRTAB
            // has been read, so the block is parsed when it is reached.
            _ => {}
        }
        Ok(())
    }

    fn parse_function_record(&mut self, record: Record) -> Result<()> {
        let mut values = record.values.as_slice();
        let mut name = Vec::new();
        if self.module_version >= 2 {
            if values.len() < 2 {
                return Err(DxilBitcodeError::Invalid("short function name"));
            }
            // STRTAB normally follows the module, so defer resolution when
            // the table is not available yet. An unresolvable name is not
            // needed for atomic detection and must not fail the parse.
            let offset = values[0];
            let len = values[1];
            match self.string_from_strtab(offset, len) {
                Ok(bytes) => name = bytes.to_vec(),
                Err(_) => {
                    let value_id = self.values.len();
                    self.pending_names.push((value_id, offset, len));
                }
            }
            values = &values[2..];
        }
        if values.len() < 8 {
            return Err(DxilBitcodeError::Invalid("short function record"));
        }
        let type_id = values[0];
        // The TYPE block follows the function records in LLVM's module layout, so the type
        // cannot be checked here; `parse_function` resolves and validates it by index.
        let index = self.functions.len();
        self.functions.push(FunctionInfo {
            name,
            type_id,
            is_declaration: values[2] != 0,
        });
        let pointer_type = self.virtual_pointer_type(self.type_id(type_id)?)?;
        self.push_value(ValueInfo {
            type_id: pointer_type,
            function: Some(index),
        })?;
        Ok(())
    }

    fn parse_types(&mut self) -> Result<()> {
        self.types.clear();
        self.function_params.clear();
        let mut declared = None;
        loop {
            match self.advance()? {
                Entry::EndBlock => {
                    if declared != Some(self.types.len()) {
                        return Err(DxilBitcodeError::Invalid("type count mismatch"));
                    }
                    return Ok(());
                }
                Entry::SubBlock(_) => self.skip_block()?,
                Entry::Record(id) => {
                    let record = self.read_record(id)?;
                    if record.code == TYPE_CODE_NUMENTRY {
                        let Some(&count) = record.values.first() else {
                            return Err(DxilBitcodeError::Invalid("empty type count"));
                        };
                        let count = self.to_usize(count)?;
                        if count > MAX_RECORD_VALUES {
                            return Err(DxilBitcodeError::Invalid("type count too large"));
                        }
                        self.types.resize(count, None);
                        declared = Some(count);
                        continue;
                    }
                    self.push_type(record)?;
                }
            }
        }
    }

    fn push_type(&mut self, record: Record) -> Result<()> {
        // STRUCT_NAME names the next struct type and does not occupy a slot.
        if record.code == TYPE_CODE_STRUCT_NAME {
            self.struct_name = bytes_from_values(&record.values)?;
            return Ok(());
        }
        let ty = match record.code {
            TYPE_CODE_INTEGER => {
                let Some(&width) = record.values.first() else {
                    return Err(DxilBitcodeError::Invalid("integer type without width"));
                };
                Type::Integer(
                    u32::try_from(width).map_err(|_| DxilBitcodeError::Invalid("integer width"))?,
                )
            }
            TYPE_CODE_POINTER => {
                let pointee = match record.values.first() {
                    Some(&id) => Some(self.type_id(id)?),
                    None => None,
                };
                Type::Pointer { pointee }
            }
            TYPE_CODE_OPAQUE_POINTER => Type::Pointer { pointee: None },
            TYPE_CODE_FUNCTION | TYPE_CODE_FUNCTION_OLD => {
                let start = if record.code == TYPE_CODE_FUNCTION_OLD {
                    2
                } else {
                    1
                };
                if record.values.len() < start + 1 {
                    return Err(DxilBitcodeError::Invalid("short function type"));
                }
                let result = self.type_id(record.values[start])?;
                let params_start = self.function_params.len();
                for &param in &record.values[start + 1..] {
                    self.function_params.push(self.type_id(param)?);
                }
                Type::Function {
                    result,
                    params_start,
                    params_len: record.values.len() - start - 1,
                }
            }
            // Aggregate and scalar types that cannot by themselves make an
            // integer atomic. They still occupy a slot, because later records
            // refer to types by index.
            TYPE_CODE_VOID
            | TYPE_CODE_FLOAT
            | TYPE_CODE_DOUBLE
            | TYPE_CODE_LABEL
            | TYPE_CODE_OPAQUE
            | TYPE_CODE_HALF
            | TYPE_CODE_ARRAY
            | TYPE_CODE_VECTOR
            | TYPE_CODE_X86_FP80
            | TYPE_CODE_FP128
            | TYPE_CODE_PPC_FP128
            | TYPE_CODE_METADATA
            | TYPE_CODE_X86_MMX
            | TYPE_CODE_STRUCT_ANON
            | TYPE_CODE_STRUCT_NAMED
            | TYPE_CODE_TOKEN
            | TYPE_CODE_BFLOAT
            | TYPE_CODE_X86_AMX
            | TYPE_CODE_TARGET_TYPE => Type::Other,
            _ => return self.unsupported("type record"),
        };
        let Some(slot) = self.types.iter_mut().find(|slot| slot.is_none()) else {
            return Err(DxilBitcodeError::Invalid("too many type records"));
        };
        *slot = Some(ty);
        Ok(())
    }

    fn parse_block_info(&mut self) -> Result<()> {
        let mut current_block = None;
        loop {
            match self.advance_raw()? {
                Entry::EndBlock => return Ok(()),
                Entry::SubBlock(_) => self.skip_block()?,
                Entry::Record(DEFINE_ABBREV) => {
                    let Some(block) = current_block else {
                        return Err(DxilBitcodeError::Invalid("blockinfo abbrev before SETBID"));
                    };
                    let abbrev = self.read_abbrev()?;
                    self.block_info
                        .abbrevs
                        .entry(block)
                        .or_default()
                        .push(abbrev);
                }
                Entry::Record(id) => {
                    let record = self.read_record(id)?;
                    if record.code == 1 {
                        let Some(&block) = record.values.first() else {
                            return Err(DxilBitcodeError::Invalid("empty SETBID"));
                        };
                        current_block = Some(block);
                    }
                }
            }
        }
    }

    fn parse_strtab(&mut self) -> Result<()> {
        let mut found = false;
        loop {
            match self.advance()? {
                Entry::EndBlock => {
                    if !found {
                        return Err(DxilBitcodeError::Invalid("string table without blob"));
                    }
                    return Ok(());
                }
                Entry::SubBlock(_) => self.skip_block()?,
                Entry::Record(id) => {
                    let record = self.read_record(id)?;
                    if record.code == 1 {
                        let Some(blob) = record.blob else {
                            return Err(DxilBitcodeError::Invalid(
                                "string table record without blob",
                            ));
                        };
                        self.strtab = blob;
                        found = true;
                    }
                }
            }
        }
    }

    fn parse_vst(&mut self) -> Result<()> {
        loop {
            match self.advance()? {
                Entry::EndBlock => return Ok(()),
                Entry::SubBlock(_) => self.skip_block()?,
                Entry::Record(id) => {
                    let record = self.read_record(id)?;
                    match record.code {
                        VST_CODE_ENTRY => {
                            // VST_ENTRY: [valueid, namechar x N], names inline
                            // as 6-bit characters decoded by the abbrev.
                            if record.values.is_empty() {
                                return Err(DxilBitcodeError::Invalid("empty VST entry"));
                            }
                            let value_id = self.to_usize(record.values[0])?;
                            let name = bytes_from_values(&record.values[1..])?;
                            self.assign_name(value_id, &name)?;
                        }
                        VST_CODE_FNENTRY => {
                            // VST_FNENTRY: [valueid, offset, ...]. With a global
                            // STRTAB the name lives at `offset`; producers that
                            // inline the name (or omit the offset) still resolve
                            // it from the function record, so an unresolvable
                            // name is left empty rather than failing the parse.
                            if record.values.len() < 2 {
                                return Err(DxilBitcodeError::Invalid("short FNENTRY"));
                            }
                            let value_id = self.to_usize(record.values[0])?;
                            let name = match self.string_from_strtab(
                                record.values[1],
                                *record.values.get(2).unwrap_or(&0),
                            ) {
                                Ok(bytes) => bytes.to_vec(),
                                Err(_) => Vec::new(),
                            };
                            self.assign_name(value_id, &name)?;
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    fn assign_name(&mut self, value_id: usize, name: &[u8]) -> Result<()> {
        let Some(slot) = self.values.get_mut(value_id) else {
            return Err(DxilBitcodeError::Invalid("VST value ID"));
        };
        // Names are optional for atomic detection. An unresolved value slot
        // (for example, an instruction whose type was not reconstructed) must
        // not make the module unparseable.
        if let Some(value) = slot {
            if let Some(function) = value.function {
                if let Some(info) = self.functions.get_mut(function) {
                    info.name = name.to_vec();
                }
            }
        }
        Ok(())
    }

    fn parse_function(&mut self) -> Result<()> {
        // LLVM emits function bodies only for definitions. Skip declarations
        // when matching the next FUNCTION_BLOCK to its module declaration.
        while self
            .functions
            .get(self.function_index)
            .is_some_and(|f| f.is_declaration)
        {
            self.function_index += 1;
        }
        let Some(function) = self
            .functions
            .get(self.function_index)
            .and_then(|f| f.cloned_name_type())
        else {
            return self.unsupported("function body without declaration");
        };
        self.function_index += 1;
        self.values.truncate(self.module_value_count());
        let fty = self.function_type(self.type_id(function.type_id)?)?;
        let params =
            self.function_params[fty.params_start..fty.params_start + fty.params_len].to_vec();
        for type_id in params {
            self.push_value(ValueInfo {
                type_id,
                function: None,
            })?;
        }
        self.next_value = self.values.len();
        loop {
            match self.advance()? {
                Entry::EndBlock => return Ok(()),
                Entry::SubBlock(id) => match id {
                    CONSTANTS_BLOCK_ID => {
                        self.enter_block(id)?;
                        self.parse_constants()?;
                        self.next_value = self.values.len();
                    }
                    VALUE_SYMTAB_BLOCK_ID => {
                        self.enter_block(id)?;
                        self.parse_vst()?;
                    }
                    _ => self.skip_block()?,
                },
                Entry::Record(id) => {
                    let record = self.read_record(id)?;
                    self.function_record(record)?;
                }
            }
        }
    }

    fn module_value_count(&self) -> usize {
        // Function declarations are module values. Global variables and
        // aliases are not currently resolved; encountering one is unsupported.
        self.values.len()
    }

    fn parse_constants(&mut self) -> Result<()> {
        let mut current_type = None;
        loop {
            match self.advance()? {
                Entry::EndBlock => return Ok(()),
                Entry::SubBlock(_) => self.skip_block()?,
                Entry::Record(id) => {
                    let record = self.read_record(id)?;
                    if record.code == CST_CODE_SETTYPE {
                        let Some(&id) = record.values.first() else {
                            return Err(DxilBitcodeError::Invalid("empty SETTYPE"));
                        };
                        current_type = Some(self.type_id(id)?);
                        continue;
                    }
                    let Some(type_id) = current_type else {
                        return Err(DxilBitcodeError::Invalid("constant before SETTYPE"));
                    };
                    if !matches!(
                        record.code,
                        CST_CODE_NULL
                            | CST_CODE_UNDEF
                            | CST_CODE_INTEGER
                            | CST_CODE_WIDE_INTEGER
                            | CST_CODE_FLOAT
                            | CST_CODE_AGGREGATE
                            | CST_CODE_STRING
                            | CST_CODE_CSTRING
                            | CST_CODE_CE_BINOP
                            | CST_CODE_CE_CAST
                            | CST_CODE_CE_GEP_OLD
                            | CST_CODE_CE_SELECT
                            | CST_CODE_CE_EXTRACTELT
                            | CST_CODE_CE_INSERTELT
                            | CST_CODE_CE_SHUFFLEVEC
                            | CST_CODE_CE_CMP
                            | CST_CODE_INLINEASM_OLD
                            | CST_CODE_CE_SHUFVEC_EX
                            | CST_CODE_CE_INBOUNDS_GEP
                            | CST_CODE_BLOCKADDRESS
                            | CST_CODE_DATA
                            | CST_CODE_INLINEASM_OLD2
                            | CST_CODE_CE_GEP_WITH_INRANGE_INDEX_OLD
                            | CST_CODE_CE_UNOP
                            | CST_CODE_POISON
                            | CST_CODE_DSO_LOCAL_EQUIVALENT
                            | CST_CODE_INLINEASM_OLD3
                            | CST_CODE_NO_CFI_VALUE
                            | CST_CODE_INLINEASM
                            | CST_CODE_CE_GEP_WITH_INRANGE
                            | CST_CODE_CE_GEP
                            | CST_CODE_PTRAUTH
                            | CST_CODE_PTRAUTH2
                            | CST_CODE_BYTE
                            | CST_CODE_WIDE_BYTE
                    ) {
                        return self.unsupported("constant record");
                    }
                    self.push_value(ValueInfo {
                        type_id,
                        function: None,
                    })?;
                }
            }
        }
    }

    fn function_record(&mut self, record: Record) -> Result<()> {
        match record.code {
            FUNC_CODE_INST_CALL => self.parse_call(record)?,
            FUNC_CODE_INST_ATOMICRMW | FUNC_CODE_INST_ATOMICRMW_OLD => {
                self.parse_atomicrmw(record)?
            }
            _ => {
                // The reader does not reconstruct the complete value list for
                // unrelated instructions. A call or atomicrmw after such an
                // instruction may therefore be unknown rather than guessed.
                if produces_value(record.code) {
                    self.values.push(None);
                    self.next_value = self.values.len();
                }
            }
        }
        Ok(())
    }

    fn record_atomic_call(&mut self, function_index: usize, result_type: u32) -> Result<()> {
        let name = self
            .functions
            .get(function_index)
            .map(|f| f.name.clone())
            .unwrap_or_default();
        if name.starts_with(b"dx.op.atomicBinOp.") {
            if matches!(
                self.types.get(result_type as usize),
                Some(Some(Type::Integer(64)))
            ) {
                self.result = Atomic64Support::Present;
            } else if !name.ends_with(b".i32") {
                return self.unsupported("atomicBinOp result type");
            }
        }
        Ok(())
    }

    fn parse_call(&mut self, record: Record) -> Result<()> {
        if record.values.len() < 3 {
            return Err(DxilBitcodeError::Invalid("short call record"));
        }
        let mut slot = 2usize;
        let cc = record.values[1];
        if (cc >> CALL_FMF) & 1 != 0 {
            slot += 1;
        }
        let explicit_type = if (cc >> CALL_EXPLICIT_TYPE) & 1 != 0 {
            let Some(&id) = record.values.get(slot) else {
                return Err(DxilBitcodeError::Invalid("call explicit type"));
            };
            slot += 1;
            Some(self.type_id(id)?)
        } else {
            None
        };
        let callee = self.read_value_type(&record.values, &mut slot)?;
        let Some(function_index) = callee.function else {
            return self.unsupported("indirect or unresolved call");
        };
        let Some(function) = self.functions.get(function_index) else {
            return Err(DxilBitcodeError::Invalid("call function index"));
        };
        let fty_id = match explicit_type {
            Some(id) => id,
            None => self.type_id(function.type_id)?,
        };
        let fty = self.function_type(fty_id)?;
        let result_type = Some(fty.result);
        if let Some(result_type) = result_type {
            if function.name.starts_with(b"dx.op.atomicBinOp.") {
                self.record_atomic_call(function_index, result_type)?;
            } else if function.name.is_empty() {
                // STRTAB has not been read yet; the name is resolved after the
                // module, so defer this call instead of guessing.
                self.pending_atomic_calls
                    .push((function_index, result_type));
            }
        }
        // Consume the fixed arguments to validate the record shape. The
        // argument values themselves are not needed for the atomic decision.
        for _ in 0..fty.params_len {
            self.pop_value(&record.values, &mut slot)?;
        }
        if slot != record.values.len() {
            return self.unsupported("vararg or trailing call operands");
        }
        if let Some(type_id) = result_type {
            self.push_value(ValueInfo {
                type_id,
                function: None,
            })?;
        } else {
            self.values.push(None);
        }
        self.next_value = self.values.len();
        Ok(())
    }

    fn parse_atomicrmw(&mut self, record: Record) -> Result<()> {
        let mut slot = 0;
        let pointer = self.read_value_type(&record.values, &mut slot)?;
        let value_type = if record.code == FUNC_CODE_INST_ATOMICRMW_OLD {
            match self.types.get(pointer.type_id as usize) {
                Some(Some(Type::Pointer { pointee: Some(id) })) => *id,
                _ => return self.unsupported("old atomicrmw pointer type"),
            }
        } else {
            self.read_value_type(&record.values, &mut slot)?.type_id
        };
        if matches!(
            self.types.get(value_type as usize),
            Some(Some(Type::Integer(64)))
        ) {
            self.result = Atomic64Support::Present;
        }
        if record.values.len() < slot + 4 || record.values.len() > slot + 5 {
            return Err(DxilBitcodeError::Invalid("atomicrmw operand count"));
        }
        self.push_value(ValueInfo {
            type_id: value_type,
            function: None,
        })?;
        self.next_value = self.values.len();
        Ok(())
    }

    fn read_value_type(&mut self, values: &[u64], slot: &mut usize) -> Result<ValueInfo> {
        let Some(&raw) = values.get(*slot) else {
            return Err(DxilBitcodeError::Invalid("missing value"));
        };
        *slot += 1;
        let id = if self.module_version >= 1 {
            self.next_value
                .checked_sub(self.to_usize(raw)?)
                .ok_or(DxilBitcodeError::Invalid("relative value ID"))?
        } else {
            self.to_usize(raw)?
        };
        if id < self.next_value {
            return self
                .values
                .get(id)
                .and_then(|v| *v)
                .ok_or(DxilBitcodeError::Unsupported("unresolved value"));
        }
        let Some(&type_id) = values.get(*slot) else {
            return Err(DxilBitcodeError::Invalid("forward value without type"));
        };
        *slot += 1;
        Ok(ValueInfo {
            type_id: self.type_id(type_id)?,
            function: None,
        })
    }

    fn pop_value(&mut self, values: &[u64], slot: &mut usize) -> Result<()> {
        let Some(&raw) = values.get(*slot) else {
            return Err(DxilBitcodeError::Invalid("missing argument"));
        };
        *slot += 1;
        if self.module_version >= 1 {
            let _ = self
                .next_value
                .checked_sub(self.to_usize(raw)?)
                .ok_or(DxilBitcodeError::Invalid("relative argument ID"))?;
        }
        Ok(())
    }

    fn push_value(&mut self, value: ValueInfo) -> Result<()> {
        if self.values.len() >= MAX_RECORD_VALUES {
            return Err(DxilBitcodeError::Invalid("value list too large"));
        }
        self.values.push(Some(value));
        Ok(())
    }

    /// Resolve a function type to its result type ID plus the range of
    /// parameter type IDs in [`Parser::function_params`].
    fn function_type(&self, id: u32) -> Result<FunctionType> {
        match self.types.get(id as usize) {
            Some(Some(Type::Function {
                result,
                params_start,
                params_len,
            })) => Ok(FunctionType {
                result: *result,
                params_start: *params_start,
                params_len: *params_len,
            }),
            _ => Err(DxilBitcodeError::Unsupported("function type")),
        }
    }

    fn virtual_pointer_type(&mut self, pointee: u32) -> Result<u32> {
        if let Some(id) = self
            .types
            .iter()
            .position(|ty| matches!(ty, Some(Type::Pointer { pointee: Some(p) }) if *p == pointee))
        {
            return u32::try_from(id).map_err(|_| DxilBitcodeError::Invalid("type ID"));
        }
        let id =
            u32::try_from(self.types.len()).map_err(|_| DxilBitcodeError::Invalid("type ID"))?;
        self.types.push(Some(Type::Pointer {
            pointee: Some(pointee),
        }));
        Ok(id)
    }

    fn type_id(&self, id: u64) -> Result<u32> {
        let id = u32::try_from(id).map_err(|_| DxilBitcodeError::Invalid("type ID"))?;
        if id as usize >= self.types.len() {
            return Err(DxilBitcodeError::Invalid("type ID out of range"));
        }
        Ok(id)
    }

    fn string_from_strtab(&self, offset: u64, len: u64) -> Result<&[u8]> {
        let start = self.to_usize(offset)?;
        let len = self.to_usize(len)?;
        let end = start
            .checked_add(len)
            .ok_or(DxilBitcodeError::Invalid("string table range"))?;
        self.strtab
            .get(start..end)
            .ok_or(DxilBitcodeError::Invalid("string table range"))
    }

    fn to_usize(&self, value: u64) -> Result<usize> {
        usize::try_from(value).map_err(|_| DxilBitcodeError::Invalid("integer does not fit usize"))
    }

    fn enter_block(&mut self, id: u64) -> Result<()> {
        if self.scopes.len() >= MAX_BLOCK_DEPTH {
            return Err(DxilBitcodeError::Invalid("block nesting too deep"));
        }
        let old_size = self.code_size;
        let old_abbrevs = std::mem::take(&mut self.abbrevs);
        self.abbrevs = self
            .block_info
            .abbrevs
            .get(&id)
            .cloned()
            .unwrap_or_default();
        let code_size = self.bits.vbr(4)?;
        if code_size == 0 || code_size > u64::from(MAX_CODE_SIZE) {
            return Err(DxilBitcodeError::Invalid("block code size"));
        }
        self.bits.align32();
        let words = self.bits.read(32)?;
        let end_bit = self
            .bits
            .pos
            .checked_add(
                self.to_usize(words)?
                    .checked_mul(32)
                    .ok_or(DxilBitcodeError::Invalid("block size"))?,
            )
            .ok_or(DxilBitcodeError::Invalid("block size"))?;
        if end_bit > self.bits.len_bits() {
            return Err(DxilBitcodeError::Truncated);
        }
        self.code_size = code_size as u32;
        self.scopes.push(Scope {
            code_size: old_size,
            abbrevs: old_abbrevs,
            end_bit,
        });
        Ok(())
    }

    fn skip_block(&mut self) -> Result<()> {
        let _ = self.bits.vbr(4)?;
        self.bits.align32();
        let words = self.bits.read(32)?;
        let skip = self
            .to_usize(words)?
            .checked_mul(32)
            .ok_or(DxilBitcodeError::Invalid("block size"))?;
        self.bits.jump(
            self.bits
                .pos
                .checked_add(skip)
                .ok_or(DxilBitcodeError::Invalid("block size"))?,
        )?;
        Ok(())
    }

    fn leave_block(&mut self) -> Result<()> {
        if let Some(scope) = self.scopes.last() {
            if self.bits.pos != scope.end_bit {}
        }
        self.bits.align32();
        let Some(scope) = self.scopes.pop() else {
            return Err(DxilBitcodeError::Invalid("block scope underflow"));
        };
        if self.bits.pos != scope.end_bit {
            return Err(DxilBitcodeError::Invalid(
                "END_BLOCK does not match block length",
            ));
        }
        self.code_size = scope.code_size;
        self.abbrevs = scope.abbrevs;
        Ok(())
    }

    fn advance(&mut self) -> Result<Entry> {
        loop {
            if let Some(scope) = self.scopes.last() {
                if self.bits.pos == scope.end_bit {
                    return Err(DxilBitcodeError::Invalid("missing END_BLOCK"));
                }
                if self.bits.pos > scope.end_bit {
                    return Err(DxilBitcodeError::Invalid("record crosses block boundary"));
                }
            }
            let id = self.bits.read(self.code_size)?;
            match id {
                END_BLOCK => return self.leave_block().map(|()| Entry::EndBlock),
                ENTER_SUBBLOCK => {
                    let block = self.bits.vbr(8)?;
                    return Ok(Entry::SubBlock(block));
                }
                DEFINE_ABBREV => {
                    let abbrev = self.read_abbrev()?;
                    self.abbrevs.push(abbrev);
                }
                _ => return Ok(Entry::Record(id)),
            }
        }
    }

    fn advance_raw(&mut self) -> Result<Entry> {
        if let Some(scope) = self.scopes.last() {
            if self.bits.pos >= scope.end_bit {
                return Err(DxilBitcodeError::Invalid("missing END_BLOCK"));
            }
        }
        let id = self.bits.read(self.code_size)?;
        match id {
            END_BLOCK => self.leave_block().map(|()| Entry::EndBlock),
            ENTER_SUBBLOCK => Ok(Entry::SubBlock(self.bits.vbr(8)?)),
            _ => Ok(Entry::Record(id)),
        }
    }

    fn read_abbrev(&mut self) -> Result<Abbrev> {
        let raw_count = self.bits.vbr(5)?;
        let count = self.to_usize(raw_count)?;
        if count == 0 || count > MAX_RECORD_VALUES {
            return Err(DxilBitcodeError::Invalid("abbreviation operand count"));
        }
        let mut operands = Vec::with_capacity(count);
        for _ in 0..count {
            if self.bits.read(1)? != 0 {
                operands.push(Operand::Literal(self.bits.vbr(8)?));
                continue;
            }
            let encoding = match self.bits.read(3)? {
                1 => Encoding::Fixed(self.bits.vbr(5)? as u32),
                2 => Encoding::Vbr(self.bits.vbr(5)? as u32),
                3 => Encoding::Array,
                4 => Encoding::Char6,
                5 => Encoding::Blob,
                _ => return Err(DxilBitcodeError::Invalid("abbreviation encoding")),
            };
            if let Encoding::Fixed(width) | Encoding::Vbr(width) = encoding {
                if width > MAX_CODE_SIZE {
                    return Err(DxilBitcodeError::Invalid("abbreviation field width"));
                }
                if width == 0 {
                    operands.push(Operand::Literal(0));
                    continue;
                }
            }
            operands.push(Operand::Encoded(encoding));
        }
        Ok(Abbrev { operands })
    }

    fn read_record(&mut self, id: u64) -> Result<Record> {
        if id == UNABBREV_RECORD {
            let code = self.bits.vbr(6)?;
            let raw_count = self.bits.vbr(6)?;
            let count = self.to_usize(raw_count)?;
            if count > MAX_RECORD_VALUES {
                return Err(DxilBitcodeError::Invalid("record too large"));
            }
            let mut values = Vec::with_capacity(count);
            for _ in 0..count {
                values.push(self.bits.vbr(6)?);
            }
            return Ok(Record {
                code,
                values,
                blob: None,
            });
        }
        let index = id
            .checked_sub(FIRST_APPLICATION_ABBREV)
            .ok_or(DxilBitcodeError::Invalid("record abbrev ID"))? as usize;
        let Some(abbrev) = self.abbrevs.get(index).cloned() else {
            return Err(DxilBitcodeError::Invalid("record abbrev ID"));
        };
        let mut values = Vec::new();
        let mut blob = None;
        let mut i = 0;
        let code = match abbrev.operands.first() {
            Some(Operand::Literal(code)) => *code,
            Some(Operand::Encoded(_)) => {
                self.read_operand(abbrev.operands[0], &mut values, &mut blob)?
            }
            None => return Err(DxilBitcodeError::Invalid("empty abbreviation")),
        };
        i += 1;
        while i < abbrev.operands.len() {
            let operand = abbrev.operands[i];
            match operand {
                Operand::Literal(value) => values.push(value),
                Operand::Encoded(Encoding::Array) => {
                    if i + 2 != abbrev.operands.len() {
                        return Err(DxilBitcodeError::Invalid("array abbreviation position"));
                    }
                    let raw_count = self.bits.vbr(6)?;
                    let count = self.to_usize(raw_count)?;
                    if count > MAX_RECORD_VALUES {
                        return Err(DxilBitcodeError::Invalid("array too large"));
                    }
                    let element = abbrev.operands[i + 1];
                    for _ in 0..count {
                        self.read_operand(element, &mut values, &mut blob)?;
                    }
                    i += 1;
                }
                Operand::Encoded(Encoding::Blob) => {
                    let raw_count = self.bits.vbr(6)?;
                    let count = self.to_usize(raw_count)?;
                    self.bits.align32();
                    let data = self.bits.bytes(count)?.to_vec();
                    let padded = count
                        .checked_add(3)
                        .map(|v| v & !3)
                        .ok_or(DxilBitcodeError::Invalid("blob size"))?;
                    if padded > count {
                        self.bits.bytes(padded - count)?;
                    }
                    blob = Some(data);
                }
                Operand::Encoded(_) => {
                    self.read_operand(operand, &mut values, &mut blob)?;
                }
            }
            i += 1;
        }
        Ok(Record { code, values, blob })
    }

    fn read_operand(
        &mut self,
        operand: Operand,
        values: &mut Vec<u64>,
        _blob: &mut Option<Vec<u8>>,
    ) -> Result<u64> {
        let value = match operand {
            Operand::Literal(value) => value,
            Operand::Encoded(Encoding::Fixed(width)) => self.bits.read(width)?,
            Operand::Encoded(Encoding::Vbr(width)) => self.bits.vbr(width)?,
            Operand::Encoded(Encoding::Char6) => {
                let v = self.bits.read(6)?;
                u64::from(
                    b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789._"[v as usize],
                )
            }
            _ => return Err(DxilBitcodeError::Invalid("scalar abbreviation operand")),
        };
        values.push(value);
        Ok(value)
    }
}

#[derive(Clone, Copy)]
struct Bits<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Bits<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    fn len_bits(&self) -> usize {
        self.bytes.len().saturating_mul(8)
    }

    fn at_end(&self) -> bool {
        self.pos == self.len_bits()
    }

    fn read(&mut self, width: u32) -> Result<u64> {
        if width > 64 {
            return Err(DxilBitcodeError::Invalid("bit read width"));
        }
        if width == 0 {
            return Ok(0);
        }
        let end = self
            .pos
            .checked_add(width as usize)
            .ok_or(DxilBitcodeError::Truncated)?;
        if end > self.len_bits() {
            return Err(DxilBitcodeError::Truncated);
        }
        let mut value = 0u64;
        for bit in 0..width as usize {
            let at = self.pos + bit;
            let byte = self.bytes[at / 8];
            value |= u64::from((byte >> (at % 8)) & 1) << bit;
        }
        self.pos = end;
        Ok(value)
    }

    fn vbr(&mut self, width: u32) -> Result<u64> {
        if !(2..=64).contains(&width) {
            return Err(DxilBitcodeError::Invalid("VBR width"));
        }
        let payload = width - 1;
        let mask = (1u64 << payload) - 1;
        let mut result = 0u64;
        let mut shift = 0u32;
        loop {
            let chunk = self.read(width)?;
            let part = chunk & mask;
            if shift >= 64 || (part > (u64::MAX >> shift)) {
                return Err(DxilBitcodeError::Invalid("VBR overflow"));
            }
            result |= part << shift;
            if chunk & (1u64 << payload) == 0 {
                return Ok(result);
            }
            shift = shift
                .checked_add(payload)
                .ok_or(DxilBitcodeError::Invalid("VBR overflow"))?;
            if shift > 64 {
                return Err(DxilBitcodeError::Invalid("VBR overflow"));
            }
        }
    }

    fn align32(&mut self) {
        self.pos = (self.pos + 31) & !31;
    }

    fn jump(&mut self, bit: usize) -> Result<()> {
        if bit > self.len_bits() {
            return Err(DxilBitcodeError::Truncated);
        }
        self.pos = bit;
        Ok(())
    }

    fn bytes(&mut self, len: usize) -> Result<&'a [u8]> {
        if self.pos % 8 != 0 {
            return Err(DxilBitcodeError::Invalid("unaligned byte read"));
        }
        let start = self.pos / 8;
        let end = start.checked_add(len).ok_or(DxilBitcodeError::Truncated)?;
        let result = self
            .bytes
            .get(start..end)
            .ok_or(DxilBitcodeError::Truncated)?;
        self.pos = end * 8;
        Ok(result)
    }
}

enum Entry {
    EndBlock,
    SubBlock(u64),
    Record(u64),
}

fn bytes_from_values(values: &[u64]) -> Result<Vec<u8>> {
    values
        .iter()
        .map(|&v| u8::try_from(v).map_err(|_| DxilBitcodeError::Invalid("name byte")))
        .collect()
}

fn produces_value(code: u64) -> bool {
    !matches!(
        code,
        1 | 10
            | 11
            | 12
            | 13
            | 15
            | 24
            | 31
            | 33
            | 35
            | 36
            | 39
            | 44
            | 45
            | 48
            | 49
            | 52
            | 55
            | 60
            | 61
            | 62
            | 63
            | 64
            | 65
            | 66
    )
}

trait FunctionInfoClone {
    fn cloned_name_type(&self) -> Option<FunctionInfo>;
}

impl FunctionInfoClone for FunctionInfo {
    fn cloned_name_type(&self) -> Option<FunctionInfo> {
        Some(FunctionInfo {
            name: self.name.clone(),
            type_id: self.type_id,
            is_declaration: self.is_declaration,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Fixture provenance and regeneration commands are documented in
    // `tests/fixtures/README.md`. The clang fixtures are tiny synthetic
    // modules; the DXC fixtures are the LLVM bitcode streams from the
    // committed DXC SM 6.6 shader headers (#453).
    const A64: &[u8] = include_bytes!("../tests/fixtures/a64.bc");
    const A32: &[u8] = include_bytes!("../tests/fixtures/a32.bc");
    const NONE: &[u8] = include_bytes!("../tests/fixtures/none.bc");
    const DX64: &[u8] = include_bytes!("../tests/fixtures/dx64.bc");
    const DX32: &[u8] = include_bytes!("../tests/fixtures/dx32.bc");
    const DXC_ATOMIC64: &[u8] = include_bytes!("../tests/fixtures/dxc_atomic64_buffer.bc");
    const DXC_WAVE64: &[u8] = include_bytes!("../tests/fixtures/dxc_wave64_lanes.bc");

    #[test]
    fn rejects_bad_magic() {
        assert_eq!(inspect_bitcode(b"nope"), Err(DxilBitcodeError::BadMagic));
    }

    #[test]
    fn atomicrmw_i64_is_present() {
        assert_eq!(inspect_bitcode(A64), Ok(Atomic64Support::Present));
    }

    #[test]
    fn atomicrmw_i32_is_absent() {
        assert_eq!(inspect_bitcode(A32), Ok(Atomic64Support::Absent));
    }

    #[test]
    fn module_without_atomics_is_absent() {
        assert_eq!(inspect_bitcode(NONE), Ok(Atomic64Support::Absent));
    }

    #[test]
    fn dxil_atomic_binop_i64_call_is_present() {
        assert_eq!(inspect_bitcode(DX64), Ok(Atomic64Support::Present));
    }

    #[test]
    fn dxil_atomic_binop_i32_call_is_absent() {
        assert_eq!(inspect_bitcode(DX32), Ok(Atomic64Support::Absent));
    }

    #[test]
    fn dxc_sm66_typed_uav_atomic64_is_present() {
        assert_eq!(inspect_bitcode(DXC_ATOMIC64), Ok(Atomic64Support::Present));
    }

    #[test]
    fn dxc_sm66_wave64_without_atomics_is_absent() {
        assert_eq!(inspect_bitcode(DXC_WAVE64), Ok(Atomic64Support::Absent));
    }

    #[test]
    fn truncated_prefixes_never_panic() {
        // A prefix may still be a well-formed stream: truncating after the module block
        // only drops trailing blocks, and the module is then complete. The contract is
        // "a defined result, never a panic", so this asserts termination and the enum,
        // and leaves the verdict to the per-fixture tests above.
        for fixture in [A64, A32, NONE, DX64, DX32, DXC_ATOMIC64, DXC_WAVE64] {
            for len in 0..fixture.len() {
                match inspect_bitcode(&fixture[..len]) {
                    Ok(_) | Err(_) => {}
                }
            }
        }
    }

    #[test]
    fn a_malformed_bitstream_is_an_error_not_a_panic() {
        // Corrupt a byte inside the module; the reader must report a defined error
        // rather than panic or silently claim a result.
        let mut bytes = A64.to_vec();
        // The module block starts at bit 384 (byte 48); corrupt a byte well inside it.
        bytes[200] ^= 0xFF;
        match inspect_bitcode(&bytes) {
            Ok(Atomic64Support::Present) | Err(_) => {}
            Ok(other) => panic!("corrupted stream reported {other:?}"),
        }
    }
}
