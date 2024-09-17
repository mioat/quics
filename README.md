# QUICS
## Introduction
A high-performance tunnel proxy, utilizing the QUIC protocol for data transmission and supporting SOCKS5, providing a fast and secure network access experience.

[![Apache 2.0 Licensed][license-badge]][license-url]
[![Build Status][actions-badge]][actions-url]

## Features
- **High Performance**: Based on the QUIC protocol, providing low latency and high throughput.
- **Security**: QUIC provides encrypted transmission, ensuring data security.

## Example
### Server
```shell
quics-server -l [::]:443 --tls-cert ./fullchain.pem --tls-key ./privkey.pem
```

```shell
quics-server -h

Usage: quics-server [OPTIONS] --listen <LISTEN> --tls-cert <TLS_CERT> --tls-key <TLS_KEY>

Options:
  -l, --listen <LISTEN>
          Server listening address
      --tls-cert <TLS_CERT>
          TLS certificate file path
      --tls-key <TLS_KEY>
          TLS Private key file path
      --initial-congestion-window <INITIAL_CONGESTION_WINDOW>
          Initial congestion window size in bytes
  -h, --help
          Print help
  -V, --version
          Print version
```

### Client
```shell
quics-client -l 127.0.0.1:1080 -r example.com:443
```

```shell
quics-client -h

Usage: quics-client [OPTIONS] --remote <REMOTE>

Options:
  -r, --remote <REMOTE>
          Remote server IP address or domain name. e.g. example.com:port
  -l, --listen <LISTEN>
          SOCKS server listening [default: 127.0.0.1:1080]
      --bind <BIND>
          IO provider address for the client [default: 0.0.0.0:0]
      --tls-sni <TLS_SNI>
          Remote server name for TLS SNI, if None will use remote address as SNI
      --tls-cert <TLS_CERT>
          Path to the TLS certificate file
      --initial-congestion-window <INITIAL_CONGESTION_WINDOW>
          Initial congestion window size in bytes
      --tracing-level <TRACING_LEVEL>
          Logging level e.g. INFO WARN ERROR [default: WARN]
  -h, --help
          Print help
  -V, --version
          Print version
```

## License
This project is licensed under the [Apache-2.0 License](./LICENSE).


[license-badge]: https://img.shields.io/badge/license-apache-blue.svg
[license-url]: https://github.com/mioat/quics/blob/main/LICENSE
[actions-badge]: https://github.com/mioat/quics/workflows/continuous-integration/badge.svg
[actions-url]: https://github.com/mioat/quics/actions/workflows/continuous-integration.yml?query=branch%3Amain