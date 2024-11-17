# 1.0.4

- Drop `thiserror` dependency.

# 1.0.3

- Do not zero-initialize cmsg (control message) buffers when sending/receivng socket messages.

# 1.0.2

- Fix compilation on FreeBSD.

# 1.0.1

- Include license file in the package.

# 1.0.0

- Remove `impl Transport for Box<dyn Transport>`.

# 0.1.0

- First release.
