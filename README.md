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
