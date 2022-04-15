# blog-dns-d - abusing DNS for micro-blogging

The idea is that this will allow micro content (limited to 512 bytes minus the DNS query/answer
overhead) to be rendered when a suitable nerd polls the DNS server

Why? For fun. For small values of fun.

## Status

Lots of hard-coded and duplication, but working for simple
DNS queries for A and TXT records, and writing useful
info to stdout as it goes.

Execute (requires su because it runs on port 53)
```bash
$ sudo ./target/debug/blog-dns-d 
```

Then invoke via dig:
```bash
$ dig +short @127.0.0.1 dnsblog.paperstack.com IN TXT
127.0.0.1
```
or
```bash
$ dig +short @127.0.0.1 dnsblog.paperstack.com IN A
"example=content"
```

## Next steps:

  * Get rid of the duplication
  * See if I can get sane output with a SINK text record (does dig even support it?)
  * Clean up (e.g. error handling, logging) and parameterise things
  * Work out an approach for content creation and storage!
  * Deploy it somewhere
  * Brag

## Resources

  * [RFC 1035](https://www.ietf.org/rfc/rfc1035.txt) - the original DNS spec.
  * [Some nicely formatted (pdf) class notes on DNS](https://mislove.org/teaching/cs4700/spring11/handouts/project1-primer.pdf) - a little easier to skim than the RFC.
  * [RFC 2181](https://datatracker.ietf.org/doc/html/rfc2181) - Updates and clarifications to the DNS spec.
  * [The Kitchen Sink Resource Record (draft)](https://www.ietf.org/archive/id/draft-eastlake-kitchen-sink-02.txt) - record type 40 (SINK) is what I hope to abuse for my blog content record.
  * [RFC 2065](https://datatracker.ietf.org/doc/html/rfc2065) - the DNSSEC spec (something to play with when I have things more-or-less working).
  * [List of DNS record types](https://en.wikipedia.org/wiki/List_of_DNS_record_types) - wherein one finds SINK and other goodies.

At some point a 3 bit `z` field in the flags header became three booleans `z`, `ad`, and `cd` - I'm not sure which spec made the change (they're the old definition in the first two links above).

