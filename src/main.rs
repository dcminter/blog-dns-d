use std::convert::TryFrom;
use std::error::Error;
use std::net::UdpSocket;
use std::str;
use zerocopy::byteorder::network_endian::{I32, U16};
use zerocopy::{AsBytes, FromBytes, FromZeroes};

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

const QR_MASK: u16 = 0b1000000000000001;
const OPCODE_MASK: u16 = 0b0111100000000000;
const AA_MASK: u16 = 0b0000010000000000;
const TC_MASK: u16 = 0b0000001000000000;
const RD_MASK: u16 = 0b0000000100000000;
const RA_MASK: u16 = 0b0000000010000000;
const Z_MASK: u16 = 0b0000000001110000;
const RCODE_MASK: u16 = 0b0000000000001111;

trait FlagsAware {
    fn qr(&self) -> bool;
    fn opcode(&self) -> u8;
    fn aa(&self) -> bool;
    fn tc(&self) -> bool;
    fn rd(&self) -> bool;
    fn ra(&self) -> bool;
    fn z(&self) -> bool;
    fn rcode(&self) -> u8;
}

impl FlagsAware for Header {
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

    fn z(&self) -> bool {
        (self.flags_and_codes.get() & Z_MASK) != 0
    }

    fn rcode(&self) -> u8 {
        ((self.flags_and_codes.get() & RCODE_MASK) >> 12) as u8
    }
}


const QUERY: bool = false;
const INITIAL_OFFSET: u8 = 12;
const MAX_UDP_QUERY_SIZE: usize = 512;

fn main() -> std::io::Result<()> {
    print!("Binding socket...");
    let socket = UdpSocket::bind("127.0.0.1:53")?;
    println!("bound.");

    let mut query_buffer = [0; MAX_UDP_QUERY_SIZE];
    let mut response_buffer = [0; MAX_UDP_QUERY_SIZE];

    loop {
        println!("Listening...");
        let (_amt, src) = socket.recv_from(&mut query_buffer)?;

        let header = Header::ref_from(&query_buffer[0..12]).unwrap();
        println!("ID: {}, (Flags), QD: {}, AN: {}, NS: {}, AR: {}", header.id, header.qdcount, header.ancount, header.nscount, header.arcount);
        println!("QR: {}, OPCODE: {}, AA: {}, TC: {}, RD: {}, RA: {}, Z: {}, RCODE: {}", header.qr(), header.opcode(), header.aa(), header.tc(), header.rd(), header.ra(), header.z(), header.rcode());

        // TODO: Handle bad record counts

        if header.qr() == QUERY {
            let (qname, next_offset, raw_query_name) = match read_qname(INITIAL_OFFSET, &query_buffer) {
                Ok((qname, next_offset)) => {
                    // Returning...
                    // qname as a nice normal string
                    // the offset of the next octet in the input buffer
                    // the raw slice representing the qname so we can steal it for building the response buffer
                    (qname, next_offset, &query_buffer[INITIAL_OFFSET as usize..next_offset])
                }
                Err(err) => {
                    println!("ERROR: {:?}", err);
                    // Something went horribly wrong; not even trying for a response here...
                    continue;
                }
            };
            println!("Query name: {}", qname);

            let type_and_class = TypeAndClass::ref_from(&query_buffer[next_offset..next_offset + 4]).unwrap();
            println!("Query Type: {}, Query Class: {}", query_type_to_string_slice(type_and_class.query_type.get()), query_class_to_string_slice(type_and_class.query_class.get()));

            if !qname.eq_ignore_ascii_case("blog.paperstack.com") || type_and_class.query_type.get() != TYPE_TXT {
                println!("Not a suitable qname query, or not expecting TXT type");
                // TODO: Fail more politely!
                respond_with_error();
                continue;
            }

            println!("Creating header response");
            let mut response_flags_and_codes: u16 = header.flags_and_codes.get() & 0b0_1111_0_0_1_0_000_0000; // Copy OPCODE from input
            response_flags_and_codes |= 0b1000000000000000; // Set QR to say this is a query response

            let response_header = Header {
                id: header.id,
                flags_and_codes: U16::from(response_flags_and_codes),
                qdcount: U16::from(1), // 1 question record
                ancount: U16::from(1), // 1 answer records
                nscount: U16::from(0), // No authority records
                arcount: U16::from(0), // No additional records
            };

            // Clear out any previous responses
            _ = &response_buffer.fill(0);

            let mut index = 0;
            append_to_buffer(&mut response_buffer, &response_header.as_bytes(), &mut index);

            println!("Creating qname response");

            let raw_qname_index = index;
            let raw_qname_as_bytes: &[u8] = &raw_query_name.as_bytes();
            append_to_buffer(&mut response_buffer, raw_qname_as_bytes, &mut index);
            append_to_buffer(&mut response_buffer, &type_and_class.as_bytes(), &mut index);

            println!("Creating resource record response");

            let response_text = "OUTPUT".as_bytes();
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

            append_to_buffer(&mut response_buffer, &answer_record.name_offset.as_bytes(), &mut index);
            append_to_buffer(&mut response_buffer, &answer_record.record_type.as_bytes(), &mut index);
            append_to_buffer(&mut response_buffer, &answer_record.record_class.as_bytes(), &mut index);
            append_to_buffer(&mut response_buffer, &answer_record.ttl.as_bytes(), &mut index);
            append_to_buffer(&mut response_buffer, &answer_record.record_length.as_bytes(), &mut index);
            append_to_buffer(&mut response_buffer, &answer_record.record_data.as_slice(), &mut index);

            print!("Sending response...");

            socket.send_to(&response_buffer[0..index], &src)?;

            println!("sent.");
        } else {
            respond_with_error();
        }
    }
}

// TODO: Make this actually return a proper error response
fn respond_with_error() {
    // Build a header with the error code set
    // Build the qname (if one was provided)
    // Send the response
    println!("ERROR");
}

fn append_to_buffer(response_buffer: &mut [u8; 512], value: &[u8], index: &mut usize) {
    _ = &response_buffer[*index..*index + value.len()].copy_from_slice(value);
    *index += value.len();
}


fn read_qname(initial_offset: u8, buf: &[u8; 512]) -> Result<(String, usize), Box<dyn Error>> {
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

const TYPE_A: u16 = 1;        // 1 a host address
const TYPE_NS: u16 = 2;       // 2 an authoritative name server
const TYPE_MD: u16 = 3;       // 3 a mail destination (Obsolete - use MX)
const TYPE_MF: u16 = 4;       // 4 a mail forwarder (Obsolete - use MX)
const TYPE_CNAME: u16 = 5;    // 5 the canonical name for an alias
const TYPE_SOA: u16 = 6;      // 6 marks the start of a zone of authority
const TYPE_MB: u16 = 7;       // 7 a mailbox domain name (EXPERIMENTAL)
const TYPE_MG: u16 = 8;       // 8 a mail group member (EXPERIMENTAL)
const TYPE_MR: u16 = 9;       // 9 a mail rename domain name (EXPERIMENTAL)
const TYPE_NULL: u16 = 10;    // 10 a null RR (EXPERIMENTAL)
const TYPE_WKS: u16 = 11;     // 11 a well known service description
const TYPE_PTR: u16 = 12;     // 12 a domain name pointer
const TYPE_HINFO: u16 = 13;   // 13 host information
const TYPE_MINFO: u16 = 14;   // 14 mailbox or mail list information
const TYPE_MX: u16 = 15;      // 15 mail exchange
const TYPE_TXT: u16 = 16;     // 16 text strings
const QTYPE_AXFR: u16 = 252;  // 252 A request for a transfer of an entire zone
const QTYPE_MAILB: u16 = 253; // 253 A request for mailbox-related records (MB, MG or MR)
const QTYPE_MAILA: u16 = 254; // 254 A request for mail agent RRs (Obsolete - see MX)

fn query_type_to_string_slice(p: u16) -> &'static str {
    match p {
        TYPE_A => "A",
        TYPE_NS => "NS",
        TYPE_MD => "MD",
        TYPE_MF => "MF",
        TYPE_CNAME => "CNAME",
        TYPE_SOA => "SOA",
        TYPE_MB => "MB",
        TYPE_MG => "MG",
        TYPE_MR => "MR",
        TYPE_NULL => "NULL",
        TYPE_WKS => "WKS",
        TYPE_PTR => "PTR",
        TYPE_HINFO => "HINFO",
        TYPE_MINFO => "MINFO",
        TYPE_MX => "MX",
        TYPE_TXT => "TXT",
        QTYPE_AXFR => "AXFR",
        QTYPE_MAILB => "MAILB",
        QTYPE_MAILA => "MAILA",
        _ => "UNKNOWN"
    }
}

const QCLASS_IN: u16 = 1; // IN - INternet
const QCLASS_CS: u16 = 2; // CS - CSNET, obsolete
const QCLASS_CH: u16 = 3; // CH - CHaosNet, obsolete
const QCLASS_HS: u16 = 4; // HS - HeSiod, obsolte
const QCLASS_ANY: u16 = 255; // * - ANY class

fn query_class_to_string_slice(p: u16) -> &'static str {
    match p {
        QCLASS_IN => "IN",
        QCLASS_CS => "CS",
        QCLASS_CH => "CH",
        QCLASS_HS => "HS",
        QCLASS_ANY => "*",
        _ => "UNKNOWN"
    }
}