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

                // TODO: Log these and only in debug level(s)
                println!("Query Name: {}", &qname);
                println!("id: {}, qr: {}, opcode: {}, aa: {}, tc: {}, rd: {}, ra: {}, z: {}, ad: {}, cd: {}, rcode: {}, qdcount: {}, ancount: {}, nscount: {}, arcount: {}",
                         &header.id, &header.qr, &header.opcode, &header.aa, &header.tc, &header.rd, &header.ra, &header.z, &header.ad, &header.cd, &header.rcode, &header.qdcount, &header.ancount, &header.nscount, &header.arcount);
                println!("qtype: {}", &qtype); // Always 1 for IN(ternet)
                println!("qclass: {}", &qclass);

                // TODO: Build and return a meaningful response!
                buf[2] = buf[2] | 0b1000_0000; // Set response bit


                socket.send_to(&buf, &src)?;
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