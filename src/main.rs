use async_std::net::{SocketAddr, UdpSocket};
use clap::Parser;
use env_logger::{Builder, Env};
use log;
use std::collections::HashSet;
use std::convert::TryFrom;
use std::error::Error;
use std::str;
use zerocopy::byteorder::network_endian::{I32, U16};
use zerocopy::{AsBytes, FromBytes, FromZeroes};

#[derive(Parser, Debug)]
#[command(
    version = env ! ("CARGO_PKG_VERSION"),
    author = env ! ("CARGO_PKG_AUTHORS"),
    about,
    long_about = None
)]
struct Opts {
    /// The message to return
    #[arg(short, long, default_value = "Hello world!")]
    message: Option<String>,

    /// The value to store in the dictionary
    #[arg(short, long)]
    qname: String,

    /// Logging level (if any)
    #[arg(short, long, default_value = "off")]
    logs: Option<String>,
}

#[derive(Debug, FromZeroes, FromBytes, AsBytes)]
#[repr(packed)]
struct Header {
    id: U16,
    flags_and_codes: U16,
    qdcount: U16,
    ancount: U16,
    nscount: U16,
    arcount: U16,
}

#[derive(Debug, FromZeroes, FromBytes, AsBytes)]
#[repr(packed)]
struct TypeAndClass {
    query_type: U16,
    query_class: U16,
}

#[derive(Debug)]
struct ResourceRecord {
    name_offset: U16,
    record_type: U16,
    record_class: U16,
    ttl: I32,
    record_length: U16,
    record_data: Vec<u8>,
}

const QR_MASK: u16 = 0b1000000000000000;
const OPCODE_MASK: u16 = 0b0111100000000000;
const AA_MASK: u16 = 0b0000010000000000;
const TC_MASK: u16 = 0b0000001000000000;
const RD_MASK: u16 = 0b0000000100000000;
const RA_MASK: u16 = 0b0000000010000000;
const RCODE_MASK: u16 = 0b0000000000001111;

#[derive(PartialEq, Eq, Hash, Debug)]
#[allow(dead_code)]
enum OpCode {
    QUERY,
    IQUERY,
    STATUS,
}

impl OpCode {
    fn value(&self) -> u16 {
        match *self {
            OpCode::QUERY => 0b0_0000_00000000000,
            OpCode::IQUERY => 0b0_0001_00000000000,
            OpCode::STATUS => 0b0_0010_00000000000,
        }
    }
}

#[derive(PartialEq, Eq, Hash, Debug)]
#[allow(dead_code)]
enum RCode {
    NoError,
    FormatError,
    ServerFailure,
    NameError,
    NotImplemented,
    Refused,
}

impl RCode {
    fn value(&self) -> u16 {
        match *self {
            RCode::NoError => 0b000000000000_0000,
            RCode::FormatError => 0b000000000000_0001,
            RCode::ServerFailure => 0b000000000000_0010,
            RCode::NameError => 0b000000000000_0011,
            RCode::NotImplemented => 0b000000000000_0100,
            RCode::Refused => 0b000000000000_0101,
        }
    }
}

#[derive(PartialEq, Eq, Hash, Debug)]
#[allow(dead_code)]
enum HeaderFlag {
    QR,
    OPCODE(OpCode),
    AA,
    TC,
    RD,
    RA,
    // Z bits are reserved and must be zero, so not included in this enum
    RCODE(RCode),
}

impl HeaderFlag {
    fn mask(&self) -> u16 {
        match *self {
            HeaderFlag::QR => 0b1000000000000000,
            HeaderFlag::OPCODE(_) => 0b0111100000000000,
            HeaderFlag::AA => 0b0000010000000000,
            HeaderFlag::TC => 0b0000001000000000,
            HeaderFlag::RD => 0b0000000100000000,
            HeaderFlag::RA => 0b0000000010000000,
            HeaderFlag::RCODE(_) => 0b0000000000001111,
        }
    }
}

impl Header {
    fn qr(&self) -> bool {
        (self.flags_and_codes.get() & QR_MASK) != 0
    }

    fn opcode(&self) -> u8 {
        ((self.flags_and_codes.get() & OPCODE_MASK) >> 1) as u8
    }

    fn aa(&self) -> bool {
        (self.flags_and_codes.get() & AA_MASK) != 0
    }

    fn tc(&self) -> bool {
        (self.flags_and_codes.get() & TC_MASK) != 0
    }

    fn rd(&self) -> bool {
        (self.flags_and_codes.get() & RD_MASK) != 0
    }

    fn ra(&self) -> bool {
        (self.flags_and_codes.get() & RA_MASK) != 0
    }

    fn rcode(&self) -> u8 {
        ((self.flags_and_codes.get() & RCODE_MASK) >> 12) as u8
    }
}

const QUERY: bool = false;
const INITIAL_OFFSET: u8 = 12;
const MAX_UDP_QUERY_SIZE: usize = 512;

const DEFAULT_LOGGING_ENV_VAR: &str = "BLOG_DNSD_LOG";

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let options: Opts = Opts::parse();
    let logging_level = options.logs.unwrap();

    Builder::from_env(Env::default().filter_or(DEFAULT_LOGGING_ENV_VAR, logging_level)).init();

    let message = options.message.unwrap_or("No message".to_string());
    let target_qname = options.qname;

    log::debug!("Binding socket.");
    let socket = UdpSocket::bind("127.0.0.1:53").await?;
    log::debug!("Socket bound.");

    log::info!(
        "Will respond with '{}' for domain '{}'",
        message,
        target_qname
    );

    let mut query_buffer = [0; MAX_UDP_QUERY_SIZE];
    let mut output_buffer = [0; MAX_UDP_QUERY_SIZE];

    loop {
        log::debug!("Listening...");
        let (_amt, src) = socket.recv_from(&mut query_buffer).await?;

        // TODO:: Kick off a thread/task/something to do this instead of blocking the next connection...?

        let input_header = Header::ref_from(&query_buffer[0..12]).unwrap();
        log::debug!(
            "ID: {}, (Flags), QD: {}, AN: {}, NS: {}, AR: {}",
            input_header.id,
            input_header.qdcount,
            input_header.ancount,
            input_header.nscount,
            input_header.arcount
        );
        log::debug!(
            "QR: {}, OPCODE: {}, AA: {}, TC: {}, RD: {}, RA: {}, RCODE: {}",
            input_header.qr(),
            input_header.opcode(),
            input_header.aa(),
            input_header.tc(),
            input_header.rd(),
            input_header.ra(),
            input_header.rcode()
        );

        // TODO: Handle bad record counts
        if input_header.qr() == QUERY {
            match input_header.qdcount.get() {
                1 => {}
                0 => {
                    // ERROR: Not supposed to be 0 because QR == QUERY
                    respond_with_basic_error(
                        RCode::FormatError,
                        input_header,
                        &mut output_buffer,
                        &socket,
                        &src,
                    )
                    .await?;
                    continue;
                }
                _ => {
                    // ERROR: We don't handle multiple queries
                    respond_with_basic_error(
                        RCode::Refused,
                        input_header,
                        &mut output_buffer,
                        &socket,
                        &src,
                    )
                    .await?;
                    continue;
                }
            }

            let (qname, next_offset, raw_query_name) =
                match read_qname(INITIAL_OFFSET, &query_buffer) {
                    Ok((qname, next_offset)) => {
                        // Returning...
                        // qname as a nice normal string
                        // the offset of the next octet in the input buffer
                        // the raw slice representing the qname so we can steal it for building the response buffer
                        (
                            qname,
                            next_offset,
                            &query_buffer[INITIAL_OFFSET as usize..next_offset],
                        )
                    }
                    Err(err) => {
                        log::error!("ERROR: {:?}", err);
                        // Something went horribly wrong; not even trying for a response here...
                        continue;
                    }
                };
            log::info!("Query name: {}", qname);

            let type_and_class =
                TypeAndClass::ref_from(&query_buffer[next_offset..next_offset + 4]).unwrap();
            log::info!(
                "Query Type: {}, Query Class: {}",
                query_type_to_string_slice(type_and_class.query_type.get()),
                query_class_to_string_slice(type_and_class.query_class.get())
            );

            if !qname.eq_ignore_ascii_case(&target_qname)
                || type_and_class.query_type.get() != TYPE_TXT
            {
                log::error!("Not a suitable qname query, or not expecting TXT type");
                // reason: RCode, input_header:&Header, raw_query_name: &[u8], type_and_class: &TypeAndClass, response_buffer: &mut [u8; 512], socket: &UdpSocket, src: &SocketAddr
                respond_with_qname_error(
                    RCode::Refused,
                    input_header,
                    raw_query_name,
                    type_and_class,
                    &mut output_buffer,
                    &socket,
                    &src,
                )
                .await?;
                continue;
            }

            log::debug!("Creating header response");
            // Copy OPCODE directly from input and set QR to say this is a query response
            let response_flags_and_codes: u16 = (input_header.flags_and_codes.get()
                & 0b0_1111_0_0_1_0_000_0000)
                | create_flags_and_codes(HashSet::from([HeaderFlag::QR]));
            let mut output_index = write_out_header_data(
                false,
                &mut output_buffer,
                input_header,
                response_flags_and_codes,
            );

            log::debug!("Creating qname response");
            let raw_qname_index = output_index;
            let raw_qname_as_bytes: &[u8] = &raw_query_name.as_bytes();
            write_out_qname_data(
                &mut output_buffer,
                &type_and_class,
                &mut output_index,
                raw_qname_as_bytes,
            );

            log::debug!("Creating resource record response");

            let response_text = message.as_bytes();
            let response_text_length: u8 = response_text.len() as u8;
            let mut response_data = Vec::new();
            response_data.push(response_text_length);
            response_data.extend_from_slice(response_text);

            let answer_record = ResourceRecord {
                name_offset: U16::from(0b1100_0000_0000_0000 as u16 | raw_qname_index as u16),
                record_type: U16::from(TYPE_TXT),
                record_class: U16::from(QCLASS_IN),
                ttl: I32::from(86400i32),
                record_length: U16::from(response_data.len() as u16),
                record_data: response_data.into(),
            };

            append_to_output_buffer(
                &mut output_buffer,
                &answer_record.name_offset.as_bytes(),
                &mut output_index,
            );
            append_to_output_buffer(
                &mut output_buffer,
                &answer_record.record_type.as_bytes(),
                &mut output_index,
            );
            append_to_output_buffer(
                &mut output_buffer,
                &answer_record.record_class.as_bytes(),
                &mut output_index,
            );
            append_to_output_buffer(
                &mut output_buffer,
                &answer_record.ttl.as_bytes(),
                &mut output_index,
            );
            append_to_output_buffer(
                &mut output_buffer,
                &answer_record.record_length.as_bytes(),
                &mut output_index,
            );
            append_to_output_buffer(
                &mut output_buffer,
                &answer_record.record_data.as_slice(),
                &mut output_index,
            );

            log::debug!("Sending response.");

            socket
                .send_to(&output_buffer[0..output_index], &src)
                .await?;

            log::info!("Response sent.");
        } else {
            respond_with_basic_error(
                RCode::Refused,
                input_header,
                &mut output_buffer,
                &socket,
                &src,
            )
            .await?;
        }
    }
}

fn write_out_qname_data(
    mut response_buffer: &mut [u8; MAX_UDP_QUERY_SIZE],
    type_and_class: &&TypeAndClass,
    mut output_index: &mut usize,
    raw_qname_as_bytes: &[u8],
) {
    append_to_output_buffer(&mut response_buffer, raw_qname_as_bytes, &mut output_index);
    append_to_output_buffer(
        &mut response_buffer,
        &type_and_class.as_bytes(),
        &mut output_index,
    );
}

fn write_out_header_data(
    error: bool,
    mut response_buffer: &mut [u8; MAX_UDP_QUERY_SIZE],
    header: &Header,
    response_flags_and_codes: u16,
) -> usize {
    let ancount = if error { 0 } else { 1 };
    let response_header = Header {
        id: header.id,
        flags_and_codes: U16::from(response_flags_and_codes),
        qdcount: U16::from(1),       // 1 question record
        ancount: U16::from(ancount), // 1 answer records
        nscount: U16::from(0),       // No authority records
        arcount: U16::from(0),       // No additional records
    };

    // Clear out any previous responses
    _ = &response_buffer.fill(0);

    let mut index = 0;
    append_to_output_buffer(
        &mut response_buffer,
        &response_header.as_bytes(),
        &mut index,
    );
    index
}

fn create_flags_and_codes(flags: HashSet<HeaderFlag>) -> u16 {
    let mut result: u16 = 0;
    flags.iter().for_each(|flag| match flag {
        HeaderFlag::QR | HeaderFlag::AA | HeaderFlag::TC | HeaderFlag::RD | HeaderFlag::RA => {
            result |= flag.mask();
        }
        HeaderFlag::OPCODE(code) => {
            result |= code.value();
        }
        HeaderFlag::RCODE(code) => {
            result |= code.value();
        }
    });
    result
}

async fn respond_with_qname_error(
    reason: RCode,
    input_header: &Header,
    raw_query_name: &[u8],
    type_and_class: &TypeAndClass,
    response_buffer: &mut [u8; MAX_UDP_QUERY_SIZE],
    socket: &UdpSocket,
    src: &SocketAddr,
) -> Result<(), Box<dyn Error>> {
    log::error!("QName Error: {:?}", reason);

    log::debug!("Creating header response");
    // Copy OPCODE directly from input and set QR to say this is a query response
    let response_flags_and_codes: u16 = (input_header.flags_and_codes.get()
        & 0b0_1111_0_0_1_0_000_0000)
        | create_flags_and_codes(HashSet::from([HeaderFlag::QR, HeaderFlag::RCODE(reason)]));
    let mut output_index = write_out_header_data(
        true,
        response_buffer,
        input_header,
        response_flags_and_codes,
    );

    log::debug!("Creating qname response");
    let raw_qname_as_bytes: &[u8] = &raw_query_name.as_bytes();
    write_out_qname_data(
        response_buffer,
        &type_and_class,
        &mut output_index,
        raw_qname_as_bytes,
    );

    log::debug!("Sending error response.");
    socket
        .send_to(&response_buffer[0..output_index], &src)
        .await?;
    log::info!("Error response sent.");
    Ok(())
}

async fn respond_with_basic_error(
    reason: RCode,
    input_header: &Header,
    response_buffer: &mut [u8; MAX_UDP_QUERY_SIZE],
    socket: &UdpSocket,
    src: &SocketAddr,
) -> Result<(), Box<dyn Error>> {
    log::error!("Basic Error: {:?}", reason);

    log::debug!("Creating header response");
    let response_flags_and_codes: u16 = input_header.flags_and_codes.get()
        | create_flags_and_codes(HashSet::from([HeaderFlag::QR, HeaderFlag::RCODE(reason)]));
    let output_index = write_out_header_data(
        true,
        response_buffer,
        input_header,
        response_flags_and_codes,
    );
    log::debug!("Sending error response.");
    socket
        .send_to(&response_buffer[0..output_index], &src)
        .await?;
    log::info!("Error response sent.");
    Ok(())
}

#[test]
fn test_append_to_output_buffer() {
    let mut output_buffer = [0; MAX_UDP_QUERY_SIZE];
    let mut output_index: usize = 0;
    output_buffer.fill(0);
    append_to_output_buffer(&mut output_buffer, "HELLO".as_bytes(), &mut output_index);
    assert_eq!(output_index, 5);
    assert_eq!(&output_buffer[0..6], [72, 69, 76, 76, 79, 0]);
}

fn append_to_output_buffer(
    output_buffer: &mut [u8; MAX_UDP_QUERY_SIZE],
    value: &[u8],
    output_index: &mut usize,
) {
    _ = &output_buffer[*output_index..*output_index + value.len()].copy_from_slice(value);
    *output_index += value.len();
}

fn read_qname(
    initial_offset: u8,
    buf: &[u8; MAX_UDP_QUERY_SIZE],
) -> Result<(String, usize), Box<dyn Error>> {
    let mut qname = Vec::new();
    let mut offset: u8 = initial_offset;
    let mut lsize = buf[usize::try_from(offset)?];
    while lsize > 0 {
        offset += 1;
        let range_begin = usize::try_from(offset)?;
        let range_end: usize = usize::try_from(lsize + offset)?;
        let label = str::from_utf8(&buf[range_begin..range_end])?;
        qname.push(label);

        offset += lsize;
        lsize = buf[usize::try_from(offset)?];
    }

    // Per the RFC no alignment needed here:
    // > The domain name terminates with the zero length octet for the null label of the root.  Note
    // > that this field may be an odd number of octets; no padding is used.

    Ok((qname.join("."), usize::try_from(offset + 1)?))
}

const TYPE_TXT: u16 = 16;
fn query_type_to_string_slice(p: u16) -> &'static str {
    match p {
        1 => "A",          // 1 a host address
        2 => "NS",         // 2 an authoritative name server
        3 => "MD",         // 3 a mail destination (Obsolete - use MX)
        4 => "MF",         // 4 a mail forwarder (Obsolete - use MX)
        5 => "CNAME",      // 5 the canonical name for an alias
        6 => "SOA",        // 6 marks the start of a zone of authority
        7 => "MB",         // 7 a mailbox domain name (EXPERIMENTAL)
        8 => "MG",         // 8 a mail group member (EXPERIMENTAL)
        9 => "MR",         // 9 a mail rename domain name (EXPERIMENTAL)
        10 => "NULL",      // 10 a null RR (EXPERIMENTAL)
        11 => "WKS",       // 11 a well known service description
        12 => "PTR",       // 12 a domain name pointer
        13 => "HINFO",     // 13 host information
        14 => "MINFO",     // 14 mailbox or mail list information
        15 => "MX",        // 15 mail exchange
        TYPE_TXT => "TXT", // 16 text strings
        252 => "AXFR",     // 252 A request for a transfer of an entire zone
        253 => "MAILB",    // 253 A request for mailbox-related records (MB, MG or MR)
        254 => "MAILA",    // 254 A request for mail agent RRs (Obsolete - see MX)
        _ => "UNKNOWN",
    }
}

const QCLASS_IN: u16 = 1;
const QCLASS_ANY: u16 = 5;

fn query_class_to_string_slice(p: u16) -> &'static str {
    match p {
        QCLASS_IN => "IN", // IN - INternet
        2 => "CS",         // CS - CSNET, obsolete
        3 => "CH",         // CH - CHaosNet, obsolete
        4 => "HS",         // HS - HeSiod, obsolte
        QCLASS_ANY => "*", // * - ANY class
        _ => "UNKNOWN",
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_append_to_output_buffer() {
        let mut output_buffer = [0; MAX_UDP_QUERY_SIZE];
        let mut output_index: usize = 0;
        output_buffer.fill(0);
        append_to_output_buffer(&mut output_buffer, "HELLO".as_bytes(), &mut output_index);
        assert_eq!(output_index, 5);
        assert_eq!(&output_buffer[0..6], [72, 69, 76, 76, 79, 0]);
    }

    #[test]
    fn test_set_opcode_flag() {
        assert_eq!(
            create_flags_and_codes(HashSet::from([HeaderFlag::OPCODE(OpCode::QUERY)])),
            0b0_0000_00000000000
        );
        assert_eq!(
            create_flags_and_codes(HashSet::from([HeaderFlag::OPCODE(OpCode::IQUERY)])),
            0b0_0001_00000000000
        );
        assert_eq!(
            create_flags_and_codes(HashSet::from([HeaderFlag::OPCODE(OpCode::STATUS)])),
            0b0_0010_00000000000
        );
    }

    #[test]
    fn test_set_rpcode_flag() {
        assert_eq!(
            create_flags_and_codes(HashSet::from([HeaderFlag::RCODE(RCode::NoError)])),
            0b000000000000_0000
        );
        assert_eq!(
            create_flags_and_codes(HashSet::from([HeaderFlag::RCODE(RCode::FormatError)])),
            0b000000000000_0001
        );
        assert_eq!(
            create_flags_and_codes(HashSet::from([HeaderFlag::RCODE(RCode::ServerFailure)])),
            0b000000000000_0010
        );
        assert_eq!(
            create_flags_and_codes(HashSet::from([HeaderFlag::RCODE(RCode::NameError)])),
            0b000000000000_0011
        );
        assert_eq!(
            create_flags_and_codes(HashSet::from([HeaderFlag::RCODE(RCode::NotImplemented)])),
            0b000000000000_0100
        );
        assert_eq!(
            create_flags_and_codes(HashSet::from([HeaderFlag::RCODE(RCode::Refused)])),
            0b000000000000_0101
        );
    }

    #[test]
    fn test_combine_some_flag() {
        assert_eq!(
            create_flags_and_codes(HashSet::from([
                HeaderFlag::QR,
                HeaderFlag::AA,
                HeaderFlag::TC,
                HeaderFlag::RD,
                HeaderFlag::RA
            ])),
            0b1_0000_1_1_1_1_000_0000
        );
    }
}
