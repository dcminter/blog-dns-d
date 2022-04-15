use std::convert::TryFrom;
use std::net::UdpSocket;
use std::str;

use bitlab::*;
use byteorder::{ByteOrder, NetworkEndian};

struct Header {
    id: u16,
    qr: bool,
    opcode: u8,
    aa: bool,
    tc: bool,
    rd: bool,
    ra: bool,
    z: bool,
    ad: bool,
    cd: bool,
    rcode: u8,
    qdcount: u16,
    ancount: u16,
    nscount: u16,
    arcount: u16,
}

static QUERY: bool = false;
static INITIAL_OFFSET: u8 = 12;
static TXT_CLASS:u16 = 16; // (SINK is 40)
static A_CLASS:u16 = 1; // A

fn main() -> std::io::Result<()> {
    {
        let socket = UdpSocket::bind("127.0.0.1:53")?;

        loop {
            let mut buf = [0; 512];
            let (_amt, src) = socket.recv_from(&mut buf)?;
            println!("Buffer: {:?}", &buf);

            let header = extract_header(&mut buf);
            if &header.qr == &QUERY {
                // TODO: Could iterate over (and enforce) qdcount times!

                let (qname, mut next_offset) = read_qname(INITIAL_OFFSET, &mut buf);
                let qtype = NetworkEndian::read_u16(&buf[next_offset..(next_offset+2)]);
                next_offset+=2;
                let qclass = NetworkEndian::read_u16(&buf[next_offset..(next_offset+2)]);
                next_offset+=2;

                // TODO: Log these and only in debug level(s)
                println!("Query Name: {}", &qname);
                println!("id: {}, qr: {}, opcode: {}, aa: {}, tc: {}, rd: {}, ra: {}, z: {}, ad: {}, cd: {}, rcode: {}, qdcount: {}, ancount: {}, nscount: {}, arcount: {}",
                         &header.id, &header.qr, &header.opcode, &header.aa, &header.tc, &header.rd, &header.ra, &header.z, &header.ad, &header.cd, &header.rcode, &header.qdcount, &header.ancount, &header.nscount, &header.arcount);
                println!("qtype: {}", &qtype); // 1 is A, 5 is CNAME, 2 is Name Servers, F is mail servers, 40 is SINK etc.!
                println!("qclass: {}", &qclass); // Always 1 for IN(ternet)

                // TODO: Build and return a meaningful response!
                buf[2] |= 0b1000_0000; // Set response bit
                buf[3] |= 0b1000_0000; // Set recursion available bit

                if qtype == TXT_CLASS {
                    println!("TXT class record requested!");
                    // Set the right header values
                    NetworkEndian::write_u16( &mut buf[6 .. 8], 1); // 1 answer
                    NetworkEndian::write_u16( &mut buf[10 .. 12], 0); // 0 additional records

                    // Then we repeat a bunch of stuff (more or less)!

                    // qname - point to the received one
                    NetworkEndian::write_u16(&mut buf[next_offset .. (next_offset+2)], 12); // Offset 12 (max is 2^14 as 2 bits used to indicate pointer)
                    buf[next_offset] = 0b1100_0000; // Pointer type
                    next_offset += 2;

                    // type
                    NetworkEndian::write_u16(&mut buf[next_offset .. (next_offset+2)], qtype);
                    next_offset += 2;

                    // class
                    NetworkEndian::write_u16(&mut buf[next_offset .. (next_offset+2)], qclass);
                    next_offset += 2;


                    NetworkEndian::write_u32( &mut buf[next_offset .. (next_offset+4)], 0); // TTL
                    next_offset += 4;

                    let text = b"example=content";
                    let rdlength:u16 = (text.len() + 1) as u16;
                    NetworkEndian::write_u16(&mut buf[next_offset .. (next_offset+2)], rdlength); // RDLENGTH (4 octets)
                    next_offset += 2;

                    buf[next_offset] = text.len() as u8;
                    next_offset += 1;

                    buf[next_offset .. next_offset + usize::try_from(text.len()).unwrap()].clone_from_slice(text);
                    next_offset += usize::try_from(rdlength).unwrap();

                    let output = &buf[0 .. next_offset];
                    println!("Output: {:?}", output);
                    socket.send_to(output, &src)?;
                } else if qtype == A_CLASS {
                    println!("A class record requested!");

                    // Set the right header values
                    NetworkEndian::write_u16( &mut buf[6 .. 8], 1); // 1 answer
                    NetworkEndian::write_u16( &mut buf[10 .. 12], 0); // 0 additional records

                    // Then we repeat a bunch of stuff (more or less)!

                    // qname - point to the received one
                    NetworkEndian::write_u16(&mut buf[next_offset .. (next_offset+2)], 12); // Offset 12 (max is 2^14 as 2 bits used to indicate pointer)
                    buf[next_offset] = 0b1100_0000; // Pointer type
                    next_offset += 2;

                    // type
                    NetworkEndian::write_u16(&mut buf[next_offset .. (next_offset+2)], qtype);
                    next_offset += 2;

                    // class
                    NetworkEndian::write_u16(&mut buf[next_offset .. (next_offset+2)], qclass);
                    next_offset += 2;

                    // Now the actual answer stuff...
                    NetworkEndian::write_u32( &mut buf[next_offset .. (next_offset+4)], 0); // TTL
                    next_offset += 4;
                    NetworkEndian::write_u16(&mut buf[next_offset .. (next_offset+2)], 4); // RDLENGTH (4 octets)
                    next_offset += 2;

                    // DATA...
                    buf[next_offset] = 127;
                    next_offset += 1;
                    buf[next_offset] = 0;
                    next_offset += 1;
                    buf[next_offset] = 0;
                    next_offset += 1;
                    buf[next_offset] = 1;
                    next_offset += 1;

                    let output = &buf[0 .. next_offset];
                    println!("Output: {:?}", output);
                    socket.send_to(output, &src)?;
                } else {
                    socket.send_to(&buf, &src)?;
                }

            } else {
                println!("Incoming packet wasn't a query! id: {}", &header.id);
            }
        }
    }
}

fn read_qname(initial_offset: u8, buf: &mut [u8; 512]) -> (String, usize) {
    let mut qname = Vec::new();
    let mut offset: u8 = initial_offset;
    let mut lsize = buf[usize::try_from(offset).unwrap()];
    while lsize > 0 {
        offset += 1;
        let range_begin = usize::try_from(offset).unwrap();
        let range_end: usize = usize::try_from(lsize + offset).unwrap();
        let label = str::from_utf8(&buf[range_begin..range_end]).unwrap();
        qname.push(label);

        offset += lsize;
        lsize = buf[usize::try_from(offset).unwrap()];
    }

    ( qname.join("."), usize::try_from(offset+1).unwrap() )
}

// TODO: Is there a better way to do the unwrap stuff here?
fn extract_header(buf: &mut [u8; 512]) -> Header {
    let flags: u16 = NetworkEndian::read_u16(&buf[2..4]);
    let header = Header {
        id: NetworkEndian::read_u16(&buf[0..2]),
        qr: flags.get_bit(0).unwrap(),
        opcode: flags.get_u8(1, 3).unwrap(),
        aa: flags.get_bit(5).unwrap(),

        tc: flags.get_bit(6).unwrap(),
        rd: flags.get_bit(7).unwrap(),
        ra: flags.get_bit(8).unwrap(),
        z: flags.get_bit(9).unwrap(),
        ad: flags.get_bit(10).unwrap(),
        cd: flags.get_bit(11).unwrap(),

        rcode: flags.get_u8(12, 4).unwrap(),

        qdcount: NetworkEndian::read_u16(&buf[4..6]),
        ancount: NetworkEndian::read_u16(&buf[6..8]),
        nscount: NetworkEndian::read_u16(&buf[8..10]),
        arcount: NetworkEndian::read_u16(&buf[10..12]),
    };
    header
}