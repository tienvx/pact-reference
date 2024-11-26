![Logo of the project](https://raw.githubusercontent.com/pact-foundation/pact-reference/master/images/logo.svg)

[![Pact-Rust Build](https://github.com/pact-foundation/pact-reference/workflows/Pact-Rust%20Build/badge.svg)](https://github.com/pact-foundation/pact-reference/actions?query=workflow%3A%22Pact-Rust+Build%22)
[![Pact-Rust FFI Build](https://github.com/pact-foundation/pact-reference/actions/workflows/build-ffi.yml/badge.svg)](https://github.com/pact-foundation/pact-reference/actions/workflows/build-ffi.yml)

# Pact Reference Implementation
> Reference implementations for Pact Specification written in Rust

This project contains a reference implementation of the [Pact specifications](https://github.com/pact-foundation/pact-specification)
written in Rust, as well as example projects in JavaScript and C (and a few others) that use the mock server library.

## Usage

### Rust

For Rust projects, you can use the Rust crates from this library in your project directly. Refer to the [Rust project
readme](rust/README.md). Requires minimum Rust 1.71.0.

### Other languages

This project contains dynamic libraries that expose the core functionality through FFI (Foreign Function Interface).

For examples:

* [C - Consumer](c/consumer-verification)
* [C - Provider](c/provider-verification)
* [Various Languages](https://github.com/YOU54F/hello_ffi)
  
For implementations:

* [Javascript via pact-js-core](https://github.com/pact-foundation/pact-js-core)
* [Ruby via pact-ruby-ffi](https://github.com/YOU54F/pact-ruby-ffi)
* [PHP via pact-php](https://github.com/pact-foundation/pact-php)
* [GoLang via pact-go](https://github.com/pact-foundation/pact-go)
* [.NET via pact-net](https://github.com/pact-foundation/pact-net)
* [Swift via pact-swift](https://github.com/surpher/PactSwift)
* [Dart via pact-dart](https://github.com/matthewshirley/pact_dart)
* [C++ via pact-cplusplus](https://github.com/pact-foundation/pact-cplusplus)
* [Python via pact-python](https://github.com/pact-foundation/pact-python)

## Building

To build the libraries in this project, you need a working Rust environment.  Requires minimum Rust 1.59.0.
Refer to the [Rust Guide](https://www.rust-lang.org/learn/get-started).

The build tool used is `cargo`.

```shell
cd rust
cargo build
```

This will compile all the libraries and put the generated files in `rust/target/debug`.

## Contributing

See [CONTRIBUTING](CONTRIBUTING.md) (PRs are always welcome!).

## Documentation

Rust library documentation is published to the Rust documentation site. Refer to the [Rust project README](rust/README.md).

Additional documentation can be found at the main [Pact website](https://pact.io).

## Contact

Join us in slack: [![slack](https://slack.pact.io/badge.svg)](https://slack.pact.io)

or

- Twitter: [@pact_up](https://twitter.com/pact_up)
- Stack Overflow: [stackoverflow.com/questions/tagged/pact](https://stackoverflow.com/questions/tagged/pact)

## Licensing

The code in this project is licensed under a MIT license. See [LICENSE](LICENSE).
