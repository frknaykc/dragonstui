# dragonstui-adapter-host

Optional process-isolated adapter hosting for [DragonsTUI](https://github.com/frknaykc/dragonstui).
The core terminal UI framework does not require this crate.

This pre-1.0 crate provides newline-delimited JSON protocol v1, supervised provider
processes, an authenticated local controller, registry installation and a plain-terminal
`dragonstui-adapter` CLI. Install does not start providers; lifecycle control is explicit.

## Source documentation

- [Protocol and SDK specification](https://github.com/frknaykc/dragonstui/blob/master/docs/adapter-sdk-specification.md)
- [Distribution and management](https://github.com/frknaykc/dragonstui/blob/master/docs/adapter-management.md)
- [Limits and exclusions](https://github.com/frknaykc/dragonstui/blob/master/docs/adapter-limits.md)
- [Release readiness](https://github.com/frknaykc/dragonstui/blob/master/docs/adapter-release-readiness.md)

The controller is not a sandbox. Providers run with the user's permissions; SHA-256
artifact checking is not publisher authentication. The reference mock is a fixture,
not a production integration. Consult the documentation at the source revision matching
your package; `master` links may describe later changes. This README does not claim a
registry publication or cross-platform acceptance.

Licensed under MIT; see the included LICENSE.
